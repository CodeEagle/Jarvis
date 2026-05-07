//! HandoffSnapshot — AI-led session handoff (PRD v1.9 §1.2).
//!
//! When CapacityAdvisor signals `handoff_recommended` / `_required`,
//! the orchestrator generates a `HandoffSnapshot` describing what's
//! done, what's still open, and what the new session should be told
//! up front. The user accepts → a fresh session starts and inherits
//! the cold-start payload; declines → the snapshot stays archived.
//!
//! The current implementation is **deterministic** — it builds the
//! sections from the Session row + recent ActivityCard / Artifact /
//! Memory state without calling an LLM. The PRD reserves spec room
//! for a higher-quality LLM-backed planner; this lays the data
//! contract so swapping in an LLM later doesn't break callers.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::session::{Session, SessionStatus};
use jarvis_core::time::now;
use jarvis_db::handoff_repo::{self, HandoffRow};
use jarvis_db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSections {
    pub title: String,
    pub completed_so_far: Vec<String>,
    pub open_threads: Vec<String>,
    pub must_know_constraints: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub memory_refs: Vec<String>,
    /// First message the user is suggested to send in the new session.
    pub suggested_first_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffColdStart {
    pub rolling_summary_excerpt: String,
    pub pinned_memory_ids: Vec<String>,
    pub pinned_artifact_ids: Vec<String>,
}

/// Full HandoffSnapshot in memory. Loaded from / persisted to
/// `handoff_snapshots` via this module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSnapshot {
    pub id: String,
    pub source_session_id: String,
    pub target_session_id: Option<String>,
    pub trace_id: Option<String>,
    pub sections: HandoffSections,
    pub cold_start: HandoffColdStart,
    pub user_decision: HandoffDecision,
    pub advisory_level: String,
    pub benefit_score: f32,
    pub pressure_ratio: f32,
    pub generated_at: chrono::DateTime<Utc>,
    pub decided_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffDecision {
    Pending,
    Accepted,
    Declined,
    Deferred,
}

impl HandoffDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            HandoffDecision::Pending => "pending",
            HandoffDecision::Accepted => "accepted",
            HandoffDecision::Declined => "declined",
            HandoffDecision::Deferred => "deferred",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "deferred" => Self::Deferred,
            _ => Self::Pending,
        }
    }
}

/// Inputs the deterministic planner reads. The caller assembles them
/// from the session row + recent activity / artifact / memory state.
#[derive(Debug, Clone)]
pub struct PlanInputs<'a> {
    pub session: &'a Session,
    pub trace_id: Option<&'a str>,
    pub advisory_level: &'a str,
    pub benefit_score: f32,
    pub pressure_ratio: f32,
    /// Most recent activity descriptions (e.g. ActivityCard.title +
    /// status). Newest first. Used to populate completed / open
    /// threads.
    pub recent_activity: Vec<ActivitySummary>,
    /// Tier 1 / Tier 2 memory ids the new session should keep close.
    pub pinned_memory_ids: Vec<String>,
    /// Artifact ids for files / docs / patches still in flight.
    pub pinned_artifact_ids: Vec<String>,
    /// Excerpt of the rolling summary (caller decides how much).
    pub rolling_summary_excerpt: String,
    /// Constraints the new session must not break (e.g. CONTEXT.md
    /// "do_not_break" entries flattened across active sub-tasks).
    pub must_know_constraints: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ActivitySummary {
    pub title: String,
    /// One of ActivityCard CardStatus::as_str() values.
    pub status: String,
}

/// Build the deterministic HandoffSnapshot from the planning inputs.
/// Side-effect free — the caller persists via `persist_snapshot`.
pub fn plan(inputs: &PlanInputs<'_>) -> HandoffSnapshot {
    let title = format!(
        "{} — handoff (advisory={})",
        if inputs.session.title.trim().is_empty() {
            "Session".into()
        } else {
            inputs.session.title.clone()
        },
        inputs.advisory_level
    );

    let mut completed = Vec::new();
    let mut open = Vec::new();
    for a in &inputs.recent_activity {
        match a.status.as_str() {
            "success" | "done" | "completed" => completed.push(a.title.clone()),
            "running" | "waiting_user" | "pending" | "suspended" => {
                open.push(a.title.clone())
            }
            _ => {}
        }
    }
    if completed.is_empty() && !inputs.session.resolved.is_empty() {
        completed = inputs.session.resolved.clone();
    }
    if open.is_empty() && !inputs.session.unresolved.is_empty() {
        open = inputs.session.unresolved.clone();
    }

    let suggested_first_message = if !open.is_empty() {
        format!("继续 {}：{}", inputs.session.title, open[0])
    } else if !completed.is_empty() {
        format!("基于上次「{}」继续推进", completed[0])
    } else {
        "继续上次的任务".into()
    };

    let sections = HandoffSections {
        title,
        completed_so_far: completed,
        open_threads: open,
        must_know_constraints: inputs.must_know_constraints.clone(),
        artifact_refs: inputs.pinned_artifact_ids.clone(),
        memory_refs: inputs.pinned_memory_ids.clone(),
        suggested_first_message,
    };

    let cold_start = HandoffColdStart {
        rolling_summary_excerpt: inputs.rolling_summary_excerpt.clone(),
        pinned_memory_ids: inputs.pinned_memory_ids.clone(),
        pinned_artifact_ids: inputs.pinned_artifact_ids.clone(),
    };

    HandoffSnapshot {
        id: new_id_with_prefix("hand"),
        source_session_id: inputs.session.id.clone(),
        target_session_id: None,
        trace_id: inputs.trace_id.map(str::to_string),
        sections,
        cold_start,
        user_decision: HandoffDecision::Pending,
        advisory_level: inputs.advisory_level.to_string(),
        benefit_score: inputs.benefit_score,
        pressure_ratio: inputs.pressure_ratio,
        generated_at: now(),
        decided_at: None,
    }
}

pub fn persist_snapshot(db: &Db, snap: &HandoffSnapshot) -> Result<()> {
    let row = HandoffRow {
        id: snap.id.clone(),
        source_session_id: snap.source_session_id.clone(),
        target_session_id: snap.target_session_id.clone(),
        trace_id: snap.trace_id.clone(),
        sections_json: serde_json::to_string(&snap.sections)
            .context("encode sections")?,
        cold_start_payload_json: serde_json::to_string(&snap.cold_start)
            .context("encode cold_start")?,
        user_decision: snap.user_decision.as_str().to_string(),
        advisory_level: snap.advisory_level.clone(),
        benefit_score: snap.benefit_score,
        pressure_ratio: snap.pressure_ratio,
        generated_at: snap.generated_at,
        decided_at: snap.decided_at,
    };
    handoff_repo::insert(db, &row).map_err(|e| anyhow!("insert handoff: {e}"))
}

pub fn load(db: &Db, id: &str) -> Result<Option<HandoffSnapshot>> {
    let Some(row) = handoff_repo::get(db, id).map_err(|e| anyhow!("get: {e}"))? else {
        return Ok(None);
    };
    Ok(Some(row_to_snapshot(row)?))
}

pub fn list_for_session(
    db: &Db,
    source_session_id: &str,
    limit: usize,
) -> Result<Vec<HandoffSnapshot>> {
    let rows = handoff_repo::list_for_session(db, source_session_id, limit)
        .map_err(|e| anyhow!("list_for_session: {e}"))?;
    rows.into_iter().map(row_to_snapshot).collect()
}

pub fn list_pending(db: &Db, limit: usize) -> Result<Vec<HandoffSnapshot>> {
    let rows = handoff_repo::list_pending(db, limit)
        .map_err(|e| anyhow!("list_pending: {e}"))?;
    rows.into_iter().map(row_to_snapshot).collect()
}

/// Accept the handoff: archive the source session, create a new
/// session seeded with the cold-start payload, and freeze the row.
/// Returns the new target session id.
pub fn accept(
    db: &Db,
    snap_id: &str,
    new_title: Option<String>,
) -> Result<String> {
    let snap = load(db, snap_id)?
        .ok_or_else(|| anyhow!("handoff snapshot not found: {snap_id}"))?;
    if snap.user_decision == HandoffDecision::Accepted
        || snap.user_decision == HandoffDecision::Declined
    {
        return Err(anyhow!(
            "snapshot already finalised as {}",
            snap.user_decision.as_str()
        ));
    }
    // Archive source session.
    if let Some(mut src) = jarvis_db::session_repo::get_session(db, &snap.source_session_id)
        .map_err(|e| anyhow!("get source session: {e}"))?
    {
        src.status = SessionStatus::Archived;
        src.updated_at = now();
        jarvis_db::session_repo::upsert_session(db, &src)
            .map_err(|e| anyhow!("archive source: {e}"))?;
    }
    // Create the new session, inheriting domain + resolved/unresolved
    // from the snapshot's open threads.
    let n = now();
    let new_id = new_id_with_prefix("sess");
    let title = new_title.unwrap_or_else(|| {
        format!(
            "{} (continued)",
            snap.sections
                .title
                .trim_end_matches(|c: char| c == ')' || c.is_whitespace())
        )
    });
    let domain = jarvis_db::session_repo::get_session(db, &snap.source_session_id)
        .map_err(|e| anyhow!("get source session domain: {e}"))?
        .map(|s| s.domain)
        .unwrap_or_else(|| "general".into());
    let new_sess = Session {
        id: new_id.clone(),
        title,
        domain,
        topic: String::new(),
        summary: snap.cold_start.rolling_summary_excerpt.chars().take(200).collect(),
        long_summary: snap.cold_start.rolling_summary_excerpt.clone(),
        active_entities: vec![],
        unresolved: snap.sections.open_threads.clone(),
        resolved: snap.sections.completed_so_far.clone(),
        recent_message_ids: vec![],
        memory_refs: snap.cold_start.pinned_memory_ids.clone(),
        skill_refs: vec![],
        status: SessionStatus::Active,
        created_at: n,
        updated_at: n,
        last_active_at: n,
    };
    jarvis_db::session_repo::upsert_session(db, &new_sess)
        .map_err(|e| anyhow!("create target: {e}"))?;
    // Freeze the snapshot row.
    handoff_repo::accept(db, snap_id, &new_id)
        .map_err(|e| anyhow!("accept: {e}"))?;
    Ok(new_id)
}

pub fn decline(db: &Db, snap_id: &str) -> Result<()> {
    handoff_repo::decline(db, snap_id).map_err(|e| anyhow!("decline: {e}"))
}

pub fn defer(db: &Db, snap_id: &str) -> Result<()> {
    handoff_repo::defer(db, snap_id).map_err(|e| anyhow!("defer: {e}"))
}

fn row_to_snapshot(row: HandoffRow) -> Result<HandoffSnapshot> {
    let sections: HandoffSections = serde_json::from_str(&row.sections_json)
        .context("decode sections_json")?;
    let cold_start: HandoffColdStart =
        serde_json::from_str(&row.cold_start_payload_json)
            .context("decode cold_start_payload_json")?;
    Ok(HandoffSnapshot {
        id: row.id,
        source_session_id: row.source_session_id,
        target_session_id: row.target_session_id,
        trace_id: row.trace_id,
        sections,
        cold_start,
        user_decision: HandoffDecision::parse(&row.user_decision),
        advisory_level: row.advisory_level,
        benefit_score: row.benefit_score,
        pressure_ratio: row.pressure_ratio,
        generated_at: row.generated_at,
        decided_at: row.decided_at,
    })
}
