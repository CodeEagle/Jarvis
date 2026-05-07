//! Subcommand dispatch — pure functions returning `String` /
//! collected lines so the binary just has to print them.
//!
//! Anything that would block on real I/O (chat REPL, HTTP server) is
//! intentionally left in `main.rs`. These are the deterministic
//! pieces.

use jarvis_core::memory::{EmotionPolarity, MemoryType, SourceType};
use jarvis_db::Db;
use jarvis_memory::manager::{MemoryManager, WriteRequest};

use crate::render;

#[derive(Debug, thiserror::Error)]
pub enum CmdError {
    #[error("missing required argument: {0}")]
    MissingArg(&'static str),
    #[error("storage: {0}")]
    Db(#[from] jarvis_db::error::DbError),
    #[error("router: {0}")]
    Router(String),
}

pub fn cmd_route(db: &Db, input: &str) -> Result<String, CmdError> {
    if input.trim().is_empty() {
        return Err(CmdError::MissingArg("user input"));
    }
    let router = jarvis_router::Router::new(db.clone());
    let (decision, _) = router
        .route(jarvis_router::RouterInput {
            user_input: input,
            session_id_hint: None,
            running_agent_types: &[],
        })
        .map_err(|e| CmdError::Router(e.to_string()))?;
    serde_json::to_string_pretty(&decision)
        .map_err(|e| CmdError::Router(e.to_string()))
}

/// Probe an `LlmJudge` adapter end-to-end: send a fixed canned input,
/// measure wall-clock latency, and report whether a coherent
/// `JudgeOutcome` came back. Designed to surface auth / binary /
/// network problems before they manifest as silent rule fallbacks
/// during a real `route` call.
pub async fn cmd_judge_probe<J>(judge: &J) -> Result<String, CmdError>
where
    J: jarvis_router::LlmJudge,
{
    let started = std::time::Instant::now();
    let allowed = vec!["coding".to_string(), "general".to_string()];
    let outcome = judge
        .judge(jarvis_router::JudgeInputs {
            user_input: "probe: please return any valid RouteDecision",
            trace_id: "trc-probe",
            task_id: "t-probe",
            rule_hints: &[],
            recent_session_titles: &[],
            allowed_agents: &allowed,
        })
        .await;
    let elapsed_ms = started.elapsed().as_millis();
    match outcome {
        Some(o) => Ok(format!(
            "judge probe ok ({elapsed_ms}ms): agent={} confidence={:.2} fallback_used=false notes={}",
            o.agent_type,
            o.confidence,
            render::truncate(&o.router_notes, 120),
        )),
        None => Err(CmdError::Router(format!(
            "judge probe FAILED after {elapsed_ms}ms: adapter returned None (auth / binary / network?)"
        ))),
    }
}

/// Async variant that consults an external `LlmJudge`. Used by the
/// `JARVIS_JUDGE=codex` (or other adapter) path on the CLI.
pub async fn cmd_route_with_judge<J>(
    db: &Db,
    input: &str,
    judge: &J,
) -> Result<String, CmdError>
where
    J: jarvis_router::LlmJudge,
{
    if input.trim().is_empty() {
        return Err(CmdError::MissingArg("user input"));
    }
    let router = jarvis_router::Router::new(db.clone());
    let (decision, _) = router
        .route_with_judge(
            jarvis_router::RouterInput {
                user_input: input,
                session_id_hint: None,
                running_agent_types: &[],
            },
            judge,
        )
        .await
        .map_err(|e| CmdError::Router(e.to_string()))?;
    serde_json::to_string_pretty(&decision)
        .map_err(|e| CmdError::Router(e.to_string()))
}

pub fn cmd_memory_write(
    db: &Db,
    content: &str,
    scope: &str,
) -> Result<String, CmdError> {
    if content.trim().is_empty() {
        return Err(CmdError::MissingArg("memory content"));
    }
    let mgr = MemoryManager::new(db.clone());
    let outcome = mgr.write(WriteRequest {
        r#type: MemoryType::PreferenceMemory,
        scope,
        content,
        entities: vec![],
        source_type: SourceType::UserExplicit,
        source_trace_id: None,
        tier: 1,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        reason: Some("CLI"),
    })?;
    Ok(format!(
        "wrote {} status={:?} trust={:.2}",
        outcome.id, outcome.status, outcome.trust_score
    ))
}

pub fn cmd_memory_list(db: &Db, scope: &str) -> Result<Vec<String>, CmdError> {
    let r = jarvis_memory::Retrieval::new(db.clone());
    let results = r.retrieve(scope, "", None, 100, 0.0)?;
    Ok(results
        .into_iter()
        .map(|hit| {
            render::format_memory_line(
                hit.memory.r#type.as_str(),
                &hit.memory.content,
                hit.trust_now,
            )
        })
        .collect())
}

pub fn cmd_memory_search(
    db: &Db,
    scope: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<String>, CmdError> {
    if query.trim().is_empty() {
        return Err(CmdError::MissingArg("memory search query"));
    }
    let r = jarvis_memory::Retrieval::new(db.clone());
    let hits = r.retrieve(scope, query, None, top_k, 0.0)?;
    Ok(hits
        .into_iter()
        .map(|h| {
            format!(
                "{:>8.3}  trust={:.2}  [{}] {}  id={}",
                h.hybrid_score,
                h.trust_now,
                h.memory.r#type.as_str(),
                render::truncate(&h.memory.content, 80),
                h.memory.id,
            )
        })
        .collect())
}

/// Mark a memory as `Deprecated` and emit a Deprecated entry in
/// `memory_change_log`. Mirrors the planned `POST /memory/{id}/forget`
/// API. Returns Err if the id doesn't exist.
pub fn cmd_memory_forget(
    db: &Db,
    id: &str,
    reason: Option<&str>,
) -> Result<String, CmdError> {
    if id.trim().is_empty() {
        return Err(CmdError::MissingArg("memory id"));
    }
    let mut mem = jarvis_db::memory_repo::get(db, id)?
        .ok_or_else(|| CmdError::Router(format!("memory not found: {id}")))?;
    if mem.status == jarvis_core::memory::MemoryStatus::Deprecated {
        return Ok(format!("memory {id} already deprecated"));
    }
    mem.status = jarvis_core::memory::MemoryStatus::Deprecated;
    mem.updated_at = jarvis_core::time::now();
    jarvis_db::memory_repo::upsert(
        db,
        &mem,
        jarvis_db::memory_repo::WriteOpts {
            change_type: jarvis_core::memory::MemoryChangeType::Deprecated,
            source_module: Some("cli"),
            reason,
        },
    )?;
    Ok(format!(
        "deprecated {id} (reason={})",
        reason.unwrap_or("(none)")
    ))
}

pub fn cmd_raw_log(db: &Db, session_id: &str, limit: usize) -> Result<Vec<String>, CmdError> {
    if session_id.is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let rows = jarvis_db::raw_event_log::list_for_session(db, session_id, limit)?;
    Ok(rows
        .into_iter()
        .map(|r| render::format_raw_event(r.seq, r.ts, &r.event_type, &r.raw_content))
        .collect())
}

pub fn cmd_audit(db: &Db, session_id: &str, limit: usize) -> Result<Vec<String>, CmdError> {
    if session_id.is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let rows = jarvis_db::audit_log::list_for_session(db, session_id, limit)?;
    Ok(rows
        .into_iter()
        .map(|e| {
            render::format_audit_line(
                e.ts,
                &e.actor,
                &e.action,
                e.target.as_deref(),
                e.status.as_str(),
                e.output_summary.as_deref(),
            )
        })
        .collect())
}

pub fn cmd_trace_view(db: &Db, trace_id: &str) -> Result<Vec<String>, CmdError> {
    if trace_id.is_empty() {
        return Err(CmdError::MissingArg("trace id"));
    }
    let events = jarvis_db::provenance::trace_events(db, trace_id)?;
    let mut out = Vec::with_capacity(events.len() + 4);
    out.push(format!(
        "─── trace {trace_id} ─── {} events ───",
        events.len()
    ));
    for e in &events {
        let safe = e.safe_content.as_deref().unwrap_or(&e.raw_content);
        out.push(format!(
            "  [{}] {:>5} {} agent={} session={:?} {}",
            e.ts.format("%H:%M:%S%.3f"),
            e.seq,
            e.event_type,
            e.agent_id.as_deref().unwrap_or("-"),
            e.session_id,
            render::truncate(safe, 180),
        ));
    }
    Ok(out)
}

pub fn cmd_memory_history(db: &Db, memory_id: &str) -> Result<Vec<String>, CmdError> {
    if memory_id.is_empty() {
        return Err(CmdError::MissingArg("memory id"));
    }
    let history = jarvis_db::provenance::memory_history(db, memory_id)?;
    let mut out = Vec::with_capacity(history.len() + 1);
    out.push(format!(
        "─── memory {memory_id} ─── {} entries ───",
        history.len()
    ));
    for h in history {
        out.push(format!(
            "  [{}] {} module={} reason={:?}",
            h.ts.format("%Y-%m-%d %H:%M:%S"),
            h.change_type,
            h.source_module.as_deref().unwrap_or("-"),
            h.reason.as_deref().unwrap_or(""),
        ));
    }
    Ok(out)
}

/// Create a new active session with a generated id. Mirrors the
/// macOS-desktop wishlist `POST /sessions` API.
pub fn cmd_sessions_new(
    db: &Db,
    title: &str,
    domain: &str,
) -> Result<String, CmdError> {
    if title.trim().is_empty() {
        return Err(CmdError::MissingArg("session title"));
    }
    let now = jarvis_core::time::now();
    let id = jarvis_core::ids::new_id_with_prefix("sess");
    let sess = jarvis_core::session::Session {
        id: id.clone(),
        title: title.into(),
        domain: domain.into(),
        topic: String::new(),
        summary: String::new(),
        long_summary: String::new(),
        active_entities: vec![],
        unresolved: vec![],
        resolved: vec![],
        recent_message_ids: vec![],
        memory_refs: vec![],
        skill_refs: vec![],
        status: jarvis_core::session::SessionStatus::Active,
        created_at: now,
        updated_at: now,
        last_active_at: now,
    };
    jarvis_db::session_repo::upsert_session(db, &sess)?;
    Ok(format!("created {id} title={title:?} domain={domain}"))
}

/// Move a session into the `archived` state so it stops showing up in
/// `sessions list`. Idempotent. Mirrors `DELETE /sessions/{id}` (we
/// archive instead of hard-delete to preserve audit history).
pub fn cmd_sessions_archive(db: &Db, id: &str) -> Result<String, CmdError> {
    if id.trim().is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let mut sess = jarvis_db::session_repo::get_session(db, id)?
        .ok_or_else(|| CmdError::Router(format!("session not found: {id}")))?;
    if sess.status == jarvis_core::session::SessionStatus::Archived {
        return Ok(format!("session {id} already archived"));
    }
    sess.status = jarvis_core::session::SessionStatus::Archived;
    sess.updated_at = jarvis_core::time::now();
    jarvis_db::session_repo::upsert_session(db, &sess)?;
    Ok(format!("archived {id}"))
}

pub fn cmd_persona_get(db: &Db, scope: &str) -> Result<String, CmdError> {
    match jarvis_db::persona_repo::get(db, scope)? {
        Some(p) => Ok(format!(
            "scope={} updated_at={} content={}",
            p.scope,
            p.updated_at.to_rfc3339(),
            p.content_json
        )),
        None => Ok(format!("(no persona for scope={scope})")),
    }
}

pub fn cmd_persona_set(
    db: &Db,
    scope: &str,
    content: &str,
) -> Result<String, CmdError> {
    if content.trim().is_empty() {
        return Err(CmdError::MissingArg("persona content"));
    }
    // Accept either raw JSON or arbitrary text; if text, wrap as JSON
    // so consumers always parse the same way.
    let content_json = match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => content.to_string(),
        Err(_) => serde_json::Value::String(content.to_string()).to_string(),
    };
    jarvis_db::persona_repo::upsert(db, scope, &content_json)?;
    Ok(format!("persona scope={scope} updated"))
}

pub fn cmd_activity_cards(
    db: &Db,
    session_id: &str,
) -> Result<Vec<String>, CmdError> {
    if session_id.trim().is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let store = jarvis_orchestrator::activity_card::ActivityCardStore::new(db.clone());
    let cards = store.list_for_session(session_id)?;
    Ok(cards
        .into_iter()
        .map(|c| {
            format!(
                "{}  agent={}  status={}  title={}  started_at={}",
                c.id,
                c.agent_type,
                c.status.as_str(),
                render::truncate(&c.title, 60),
                c.started_at.to_rfc3339(),
            )
        })
        .collect())
}

pub fn cmd_sessions_list(db: &Db) -> Result<Vec<String>, CmdError> {
    let sessions = jarvis_db::session_repo::list_recent(db, 50)?;
    Ok(sessions
        .into_iter()
        .map(|s| {
            format!(
                "{}  {}  domain={}  last_active={}",
                s.id,
                render::truncate(&s.title, 40),
                s.domain,
                s.last_active_at.to_rfc3339(),
            )
        })
        .collect())
}

pub fn cmd_session_messages(
    db: &Db,
    session_id: &str,
    limit: usize,
) -> Result<Vec<String>, CmdError> {
    if session_id.is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let msgs = jarvis_db::session_repo::recent_messages(db, session_id, limit)?;
    Ok(msgs
        .into_iter()
        .map(|m| {
            let role = match m.role {
                jarvis_core::session::MessageRole::User => "user",
                jarvis_core::session::MessageRole::Assistant => "assistant",
                jarvis_core::session::MessageRole::System => "system",
                jarvis_core::session::MessageRole::Tool => "tool",
            };
            format!(
                "{}  [{role:<9}] {}",
                m.created_at.to_rfc3339(),
                render::truncate(&m.content, 200)
            )
        })
        .collect())
}

pub fn cmd_skills_list(db: &Db) -> Result<Vec<String>, CmdError> {
    let reg = jarvis_growth::SkillRegistry::new(db.clone());
    let skills = reg.list()?;
    Ok(skills
        .into_iter()
        .map(|s| {
            format!(
                "{}  {}  status={}  success={}  failure={}  v{}",
                s.id,
                render::truncate(&s.name, 30),
                s.status.as_str(),
                s.success_count,
                s.failure_count,
                s.version,
            )
        })
        .collect())
}

pub fn cmd_walkthrough_list(
    db: &Db,
    session_id: &str,
) -> Result<Vec<String>, CmdError> {
    if session_id.is_empty() {
        return Err(CmdError::MissingArg("session id"));
    }
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let docs = store.list_for_session(session_id)?;
    Ok(docs
        .into_iter()
        .map(|d| {
            format!(
                "{}  approval={}  verification={}  '{}'",
                d.id,
                d.approval_status.as_str(),
                d.verification_status.as_str(),
                render::truncate(&d.title, 40),
            )
        })
        .collect())
}

pub fn cmd_walkthrough_approve(
    db: &Db,
    doc_id: &str,
    actor: &str,
) -> Result<String, CmdError> {
    if doc_id.is_empty() {
        return Err(CmdError::MissingArg("walkthrough id"));
    }
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = store.approve(doc_id, actor)?;
    Ok(format!(
        "approved {} by {} at {}",
        doc.id,
        doc.approved_by.as_deref().unwrap_or("?"),
        doc.approved_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_default()
    ))
}

pub fn cmd_walkthrough_reject(
    db: &Db,
    doc_id: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<String, CmdError> {
    if doc_id.is_empty() {
        return Err(CmdError::MissingArg("walkthrough id"));
    }
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = store.reject(doc_id, actor, reason)?;
    Ok(format!(
        "rejected {} by {}",
        doc.id,
        doc.approved_by.as_deref().unwrap_or("?")
    ))
}

pub fn cmd_outbox_pending(db: &Db) -> Result<String, CmdError> {
    let n = jarvis_db::outbox::pending_count(db)?;
    Ok(format!("pending outbox rows: {n}"))
}

pub fn cmd_dashboard_summary(db: &Db) -> Result<String, CmdError> {
    let active_sessions = jarvis_db::session_repo::list_recent(db, 1000)?.len();
    let raw_events = jarvis_db::raw_event_log::count(db)?;
    let memories = jarvis_db::memory_repo::count(db)?;
    let pending_outbox = jarvis_db::outbox::pending_count(db)?;
    Ok(render::format_dashboard_summary(
        active_sessions,
        raw_events,
        memories,
        pending_outbox,
    ))
}

/// JSON-formatted variant of the dashboard summary, used by --json
/// CLI flag and machine-readable consumers.
pub fn cmd_dashboard_summary_json(db: &Db) -> Result<String, CmdError> {
    let active_sessions = jarvis_db::session_repo::list_recent(db, 1000)?.len();
    let raw_events = jarvis_db::raw_event_log::count(db)?;
    let memories = jarvis_db::memory_repo::count(db)?;
    let pending_outbox = jarvis_db::outbox::pending_count(db)?;
    let collector = jarvis_growth::Collector::new(db.clone());
    let route_decisions = collector.count_of("route_decision")?;
    let payload = serde_json::json!({
        "active_sessions": active_sessions,
        "raw_events": raw_events,
        "memories": memories,
        "pending_outbox": pending_outbox,
        "route_decisions": route_decisions,
    });
    Ok(payload.to_string())
}

#[cfg(test)]
mod tests;
