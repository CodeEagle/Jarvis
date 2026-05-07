//! Stateless handlers — each takes a `Db` and returns a response.

use bytes::Bytes;
use http_body_util::Full;
use hyper::Response;
use jarvis_db::Db;

use crate::{json_ok, ApiError};

pub fn recent_sessions(db: &Db) -> Result<Response<Full<Bytes>>, ApiError> {
    let sessions = jarvis_db::session_repo::list_recent(db, 25)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&sessions))
}

pub fn get_session(db: &Db, id: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let session = jarvis_db::session_repo::get_session(db, id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    match session {
        Some(s) => Ok(json_ok(&s)),
        None => Err(ApiError::NotFound(id.to_string())),
    }
}

pub fn list_memories(db: &Db, scope: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let memories = jarvis_db::memory_repo::list_by_scope(db, scope, 100)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&memories))
}

pub fn raw_log(db: &Db, session: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let events = jarvis_db::raw_event_log::list_for_session(db, session, 200)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&events))
}

pub fn trace(db: &Db, trace_id: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let events = jarvis_db::provenance::trace_events(db, trace_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&events))
}

pub fn audit(db: &Db, session: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let entries = jarvis_db::audit_log::list_for_session(db, session, 200)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&entries))
}

pub fn growth_events(db: &Db) -> Result<Response<Full<Bytes>>, ApiError> {
    let collector = jarvis_growth::Collector::new(db.clone());
    let events = collector
        .list_events_for_event_type("route_decision", 100)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&events))
}

pub fn growth_artifacts(db: &Db) -> Result<Response<Full<Bytes>>, ApiError> {
    let collector = jarvis_growth::Collector::new(db.clone());
    let arts = collector
        .list_artifacts(None, None)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&arts))
}

pub fn walkthrough(db: &Db, session: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let docs = store
        .list_for_session(session)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&docs))
}

pub fn session_messages(db: &Db, session: &str) -> Result<Response<Full<Bytes>>, ApiError> {
    let messages = jarvis_db::session_repo::recent_messages(db, session, 200)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&messages))
}

/// GET /conversation/{session}/ownership — current OwnershipRecord
/// (PRD §23.3). Returns null when no Agent currently holds the bus.
pub fn conversation_ownership(
    db: &Db,
    session_id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(db.clone());
    let record = bus
        .current_ownership(session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&record))
}

/// GET /conversation/{session}/sub-channels — active SubChannels.
pub fn conversation_sub_channels(
    db: &Db,
    session_id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(db.clone());
    let channels = bus
        .list_active_sub_channels(session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&channels))
}

/// GET /conversation/{session}/activity — ActivityCard list (PRD §8.13).
pub fn conversation_activity(
    db: &Db,
    session_id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let store = jarvis_orchestrator::activity_card::ActivityCardStore::new(db.clone());
    let cards = store
        .list_for_session(session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&cards))
}

/// GET /conversation/{session}/pending — pending user messages waiting
/// for the owner Agent.
pub fn conversation_pending(
    db: &Db,
    session_id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(db.clone());
    let msgs = bus
        .list_pending_messages(session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&msgs))
}

// ── v1.9 handoff endpoints ─────────────────────────────────────────────

pub fn capacity_for_session(
    db: &Db,
    session_id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let sess = jarvis_db::session_repo::get_session(db, session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(session_id.to_string()))?;
    let rolling_tokens = (sess.long_summary.len() / 4) as u32;
    let budget_tokens: u32 = 8000;
    let pressure = (rolling_tokens as f32 / budget_tokens as f32).min(1.0);
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(db.clone());
    let waiting_user = bus
        .current_ownership(session_id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .map(|o| {
            o.interaction_mode
                == jarvis_orchestrator::conversation_bus::InteractionMode::WaitingUser
        })
        .unwrap_or(false);
    let health = jarvis_control::ContextHealth {
        session_id: session_id.into(),
        trace_id: String::new(),
        injected_tokens: rolling_tokens,
        budget_tokens,
        pressure_ratio: pressure,
        benefit_score: 0.6,
        benefit_trend: vec![],
        rolling_summary_tokens: rolling_tokens,
        force_compress_hits: 0,
        waiting_user,
        seconds_since_last_advisory: u64::MAX,
        steer_just_injected: false,
    };
    let level = jarvis_control::evaluate_capacity(
        &health,
        jarvis_control::AdvisorPolicy::defaults(),
    );
    Ok(json_ok(&serde_json::json!({
        "session_id": session_id,
        "advisory_level": level.as_str(),
        "pressure_ratio": pressure,
        "benefit_score": health.benefit_score,
        "rolling_summary_tokens": rolling_tokens,
        "waiting_user": waiting_user,
    })))
}

pub fn handoff_get(
    db: &Db,
    id: &str,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let snap = jarvis_orchestrator::handoff::load(db, id)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(id.to_string()))?;
    Ok(json_ok(&snap))
}

pub fn handoff_list_pending(
    db: &Db,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let snaps = jarvis_orchestrator::handoff::list_pending(db, 50)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&snaps))
}

pub fn dashboard_metrics(db: &Db) -> Result<Response<Full<Bytes>>, ApiError> {
    let active_sessions = jarvis_db::session_repo::list_recent(db, 1000)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .len();
    let raw_event_count = jarvis_db::raw_event_log::count(db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let memory_count = jarvis_db::memory_repo::count(db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let outbox_pending = jarvis_db::outbox::pending_count(db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let collector = jarvis_growth::Collector::new(db.clone());
    let route_decisions = collector
        .count_of("route_decision")
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let promoted_artifacts = collector
        .list_artifacts(None, Some(jarvis_growth::ArtifactStatus::Promoted))
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .len();
    let payload = serde_json::json!({
        "active_sessions": active_sessions,
        "raw_event_count": raw_event_count,
        "memory_count": memory_count,
        "outbox_pending": outbox_pending,
        "route_decisions": route_decisions,
        "promoted_artifacts": promoted_artifacts,
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    Ok(json_ok(&payload))
}
