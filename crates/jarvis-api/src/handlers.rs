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
