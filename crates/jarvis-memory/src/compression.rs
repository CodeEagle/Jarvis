//! Compression: TurnSummary writes, Rolling Summary versioning, and the
//! three-dimensional dynamic policy from Section 9.5.2a.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

// ── dynamic compression policy (Section 9.5.2a) ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    Simple,
    Medium,
    Complex,
}

#[derive(Debug, Clone, Copy)]
pub struct CompressionPolicy {
    /// Current context fill ratio (0..=1).
    pub pressure_ratio: f32,
    pub task_complexity: TaskComplexity,
    /// Daily-budget consumption ratio (0..=1).
    pub usage_budget_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggressiveness {
    Light,
    Normal,
    Aggressive,
}

/// Section 9.5.2a — dynamic threshold computation.
///
/// `simple` tasks compress earlier (0.45), `complex` later (0.65); usage
/// pressure shaves 0.10 off when above 0.80, with a hard floor of 0.35.
pub fn compression_threshold(p: &CompressionPolicy) -> f32 {
    let mut base: f32 = match p.task_complexity {
        TaskComplexity::Simple => 0.45,
        TaskComplexity::Medium => 0.55,
        TaskComplexity::Complex => 0.65,
    };
    if p.usage_budget_ratio > 0.8 {
        base -= 0.10;
    }
    if p.usage_budget_ratio > 0.9 {
        base -= 0.05;
    }
    base.max(0.35)
}

pub fn aggressiveness(p: &CompressionPolicy) -> Aggressiveness {
    if p.pressure_ratio > 0.85 || p.usage_budget_ratio > 0.9 {
        Aggressiveness::Aggressive
    } else if p.task_complexity == TaskComplexity::Simple {
        Aggressiveness::Normal
    } else if p.task_complexity == TaskComplexity::Complex {
        Aggressiveness::Light
    } else {
        Aggressiveness::Normal
    }
}

/// True when current pressure exceeds the policy threshold and we should
/// kick off a Rolling Summary update.
pub fn should_trigger(p: &CompressionPolicy) -> bool {
    p.pressure_ratio >= compression_threshold(p)
}

// ── persistence ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSummaryDraft {
    pub session_id: String,
    pub trace_id: Option<String>,
    pub task_id: Option<String>,
    pub user_goal: String,
    pub actions_taken: Vec<String>,
    pub result: String,
    pub entities: Vec<String>,
    pub unresolved: Vec<String>,
    pub source_message_ids: Vec<String>,
    pub token_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurnSummary {
    pub id: String,
    pub session_id: String,
    pub user_goal: String,
    pub result: String,
    pub created_at: DateTime<Utc>,
    pub token_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryTrigger {
    TaskComplete,
    IdleTimeout,
    TokenThreshold,
    CountThreshold,
    Manual,
}

impl SummaryTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            SummaryTrigger::TaskComplete => "task_complete",
            SummaryTrigger::IdleTimeout => "idle_timeout",
            SummaryTrigger::TokenThreshold => "token_threshold",
            SummaryTrigger::CountThreshold => "count_threshold",
            SummaryTrigger::Manual => "manual",
        }
    }
}

#[derive(Clone)]
pub struct CompressionStore {
    db: Db,
}

impl CompressionStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn write_turn_summary(&self, draft: &TurnSummaryDraft) -> DbResult<String> {
        let id = new_id_with_prefix("turn");
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO turn_summaries
                    (id, session_id, trace_id, task_id, user_goal,
                     actions_taken_json, result, entities_json,
                     unresolved_json, source_message_ids_json,
                     token_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    draft.session_id,
                    draft.trace_id,
                    draft.task_id,
                    draft.user_goal,
                    serde_json::to_string(&draft.actions_taken)
                        .unwrap_or_else(|_| "[]".into()),
                    draft.result,
                    serde_json::to_string(&draft.entities).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&draft.unresolved)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&draft.source_message_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    draft.token_count,
                    now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })?;
        Ok(id)
    }

    pub fn list_turn_summaries(
        &self,
        session_id: &str,
        limit: usize,
    ) -> DbResult<Vec<StoredTurnSummary>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, user_goal, result, created_at, token_count
                   FROM turn_summaries
                  WHERE session_id = ?1
                  ORDER BY created_at DESC
                  LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![session_id, limit as i64], |row| {
                    let ts_str: String = row.get(4)?;
                    let ts = DateTime::parse_from_rfc3339(&ts_str)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    Ok(StoredTurnSummary {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        user_goal: row.get(2)?,
                        result: row.get(3)?,
                        created_at: ts,
                        token_count: row.get::<_, i64>(5)? as u32,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Append a new Rolling Summary version. Each call creates an
    /// immutable historical record. The session's `long_summary` is
    /// updated in the same transaction so callers always see a
    /// consistent view.
    pub fn append_rolling_version(
        &self,
        session_id: &str,
        content: &str,
        token_count: u32,
        trigger: SummaryTrigger,
        source_turn_summary_ids: &[String],
    ) -> DbResult<u32> {
        self.db.with(|conn| {
            let tx = conn.unchecked_transaction()?;

            // Determine next version
            let next: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1
                       FROM rolling_summary_versions
                      WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .unwrap_or(1);

            tx.execute(
                "INSERT INTO rolling_summary_versions
                    (id, session_id, version, content, token_count,
                     trigger_reason, source_turn_summary_ids_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new_id_with_prefix("rs"),
                    session_id,
                    next,
                    content,
                    token_count,
                    trigger.as_str(),
                    serde_json::to_string(source_turn_summary_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    now().to_rfc3339(),
                ],
            )?;

            tx.execute(
                "UPDATE sessions
                    SET long_summary = ?1,
                        updated_at = ?2
                  WHERE id = ?3",
                params![content, now().to_rfc3339(), session_id],
            )?;

            tx.commit()?;
            Ok(next as u32)
        })
    }

    pub fn current_rolling_version(&self, session_id: &str) -> DbResult<Option<u32>> {
        self.db.with(|conn| {
            let n: Option<i64> = conn
                .query_row(
                    "SELECT MAX(version) FROM rolling_summary_versions WHERE session_id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            Ok(n.map(|v| v as u32))
        })
    }

    pub fn rolling_version_history(
        &self,
        session_id: &str,
    ) -> DbResult<Vec<(u32, String, String)>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT version, content, trigger_reason
                   FROM rolling_summary_versions
                  WHERE session_id = ?1
                  ORDER BY version ASC",
            )?;
            let rows = stmt
                .query_map(params![session_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u32,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}
