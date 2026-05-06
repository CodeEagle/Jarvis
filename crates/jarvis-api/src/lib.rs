//! HTTP API surface (Section 23).
//!
//! Implements the read-mostly endpoints first — the ones that don't
//! require an LLM round-trip:
//!
//!   POST /router/input         → run Router synchronously, return RouteDecision
//!   GET  /sessions/recent      → recent active sessions
//!   GET  /sessions/{id}        → single session
//!   GET  /memory/{scope}       → list memories in scope
//!   POST /memory               → write a user-explicit memory
//!   GET  /raw-log/{session}    → raw_event_log entries for a session
//!   GET  /trace/{trace_id}     → all raw events for a trace
//!   GET  /audit/{session}      → audit_log for a session
//!   GET  /growth/events        → recent route_decision events
//!   GET  /growth/artifacts     → list growth artifacts
//!   GET  /walkthrough/{session}→ list walkthrough docs for a session
//!   GET  /healthz              → liveness probe
//!
//! All responses are JSON. Errors fall back to a `{"error": "..."}`
//! body with the appropriate status.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use jarvis_db::Db;
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{info, warn};

pub mod handlers;

#[derive(Clone)]
pub struct ApiState {
    pub db: Db,
}

pub async fn serve(state: ApiState, addr: SocketAddr) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(target: "api", "jarvis-api listening on {addr}");
    let state = Arc::new(state);
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(p) => p,
            Err(e) => {
                warn!(target: "api", "accept error: {e}");
                continue;
            }
        };
        let io = TokioIo::new(stream);
        let svc_state = state.clone();
        tokio::spawn(async move {
            let svc = service_fn(move |req| handle(svc_state.clone(), req, peer));
            if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                warn!(target: "api", "connection {peer} closed: {e}");
            }
        });
    }
}

async fn handle(
    state: Arc<ApiState>,
    req: Request<Incoming>,
    _peer: SocketAddr,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let result = match (method.clone(), path.as_str()) {
        (Method::GET, "/healthz") => Ok(json_ok(&serde_json::json!({"ok": true}))),
        (Method::POST, "/router/input") => route_input(&state, req).await,
        (Method::GET, "/sessions/recent") => handlers::recent_sessions(&state.db),
        (Method::GET, "/growth/events") => handlers::growth_events(&state.db),
        (Method::GET, "/growth/artifacts") => handlers::growth_artifacts(&state.db),
        (Method::POST, "/memory") => write_memory(&state, req).await,
        (Method::POST, "/steer") => post_steer(&state, req).await,
        (Method::POST, "/interrupt") => post_interrupt(&state, req).await,
        (Method::POST, "/maintenance/lint") => post_maintenance_lint(&state, req).await,
        (Method::GET, p) if p.starts_with("/sessions/") => {
            handlers::get_session(&state.db, &p["/sessions/".len()..])
        }
        (Method::GET, p) if p.starts_with("/memory/") => {
            handlers::list_memories(&state.db, &p["/memory/".len()..])
        }
        (Method::GET, p) if p.starts_with("/raw-log/") => {
            handlers::raw_log(&state.db, &p["/raw-log/".len()..])
        }
        (Method::GET, p) if p.starts_with("/trace/") => {
            handlers::trace(&state.db, &p["/trace/".len()..])
        }
        (Method::GET, p) if p.starts_with("/audit/") => {
            handlers::audit(&state.db, &p["/audit/".len()..])
        }
        (Method::GET, p) if p.starts_with("/walkthrough/") => {
            handlers::walkthrough(&state.db, &p["/walkthrough/".len()..])
        }
        _ => Err(ApiError::NotFound(format!("{method} {path}"))),
    };
    Ok(match result {
        Ok(r) => r,
        Err(e) => error_response(e),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal: {0}")]
    Internal(String),
}

fn error_response(err: ApiError) -> Response<Full<Bytes>> {
    let (status, body) = match &err {
        ApiError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    let body = serde_json::json!({"error": body}).to_string();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

pub(crate) fn json_ok<T: serde::Serialize>(value: &T) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

#[derive(Debug, Deserialize)]
struct RouteInputBody {
    user_input: String,
    session_id_hint: Option<String>,
    #[serde(default)]
    running_agent_types: Vec<String>,
}

async fn route_input(
    state: &Arc<ApiState>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let body = read_json::<RouteInputBody>(req).await?;
    let router = jarvis_router::Router::new(state.db.clone());
    let agents: Vec<String> = body.running_agent_types.clone();
    let (decision, diag) = router
        .route(jarvis_router::RouterInput {
            user_input: &body.user_input,
            session_id_hint: body.session_id_hint.as_deref(),
            running_agent_types: &agents,
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let payload = serde_json::json!({
        "decision": decision,
        "diagnostics": {
            "raw_event_seq": diag.raw_event_seq,
            "rule_hints": diag.rule_hints,
            "had_explicit_reference": diag.had_explicit_reference,
            "mention_warning": diag.mention_warning,
        }
    });
    Ok(json_ok(&payload))
}

#[derive(Debug, Deserialize)]
struct WriteMemoryBody {
    content: String,
    #[serde(default = "default_scope")]
    scope: String,
    #[serde(default)]
    entities: Vec<String>,
}

fn default_scope() -> String {
    "global".into()
}

async fn write_memory(
    state: &Arc<ApiState>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let body = read_json::<WriteMemoryBody>(req).await?;
    let mgr = jarvis_memory::manager::MemoryManager::new(state.db.clone());
    let outcome = mgr
        .write(jarvis_memory::manager::WriteRequest {
            r#type: jarvis_core::memory::MemoryType::PreferenceMemory,
            scope: &body.scope,
            content: &body.content,
            entities: body.entities,
            source_type: jarvis_core::memory::SourceType::UserExplicit,
            source_trace_id: None,
            tier: 1,
            emotion_energy: 0.0,
            emotion_polarity: jarvis_core::memory::EmotionPolarity::Neutral,
            reason: Some("api"),
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&serde_json::json!({
        "id": outcome.id,
        "status": format!("{:?}", outcome.status),
        "trust_score": outcome.trust_score,
    })))
}

#[derive(Debug, Deserialize)]
struct SteerBody {
    session_id: String,
    sub_task_id: String,
    content: String,
    #[serde(default = "default_steer_scope")]
    scope: String,
    #[serde(default = "default_inject_at")]
    inject_at: String,
}

fn default_steer_scope() -> String {
    "constraint".into()
}

fn default_inject_at() -> String {
    "next_step".into()
}

async fn post_steer(
    state: &Arc<ApiState>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let body = read_json::<SteerBody>(req).await?;
    let scope = match body.scope.as_str() {
        "direction" => jarvis_orchestrator::steer::SteerScope::Direction,
        "priority" => jarvis_orchestrator::steer::SteerScope::Priority,
        _ => jarvis_orchestrator::steer::SteerScope::Constraint,
    };
    let inject_at = match body.inject_at.as_str() {
        "next_tool_call" => jarvis_orchestrator::steer::InjectAt::NextToolCall,
        "next_checkpoint" => jarvis_orchestrator::steer::InjectAt::NextCheckpoint,
        _ => jarvis_orchestrator::steer::InjectAt::NextStep,
    };
    let ctrl = jarvis_orchestrator::steer::SteerController::new(state.db.clone());
    let outcome = ctrl
        .enqueue(jarvis_orchestrator::steer::EnqueueRequest {
            session_id: &body.session_id,
            sub_task_id: &body.sub_task_id,
            trace_id: None,
            content: &body.content,
            scope,
            inject_at,
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let payload = match outcome {
        jarvis_orchestrator::steer::SteerEnqueueOutcome::Accepted(s) => {
            serde_json::json!({"status":"accepted","id": s.id})
        }
        jarvis_orchestrator::steer::SteerEnqueueOutcome::RateLimited { recent_count } => {
            serde_json::json!({"status":"rate_limited","recent_count": recent_count})
        }
    };
    Ok(json_ok(&payload))
}

#[derive(Debug, Deserialize)]
struct InterruptBody {
    session_id: String,
    sub_task_id: String,
    /// "soft" / "hard" / "async"
    kind: String,
    /// Optional task node id for status update.
    node_id: Option<String>,
}

async fn post_interrupt(
    state: &Arc<ApiState>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let body = read_json::<InterruptBody>(req).await?;
    let ctrl = jarvis_orchestrator::interrupt::InterruptController::new(state.db.clone());
    let outcome = match body.kind.as_str() {
        "soft" => ctrl.soft(&body.session_id, &body.sub_task_id, body.node_id.as_deref()),
        "hard" => ctrl.hard(&body.session_id, &body.sub_task_id, body.node_id.as_deref()),
        "async" => ctrl.async_tag(&body.session_id, &body.sub_task_id),
        other => return Err(ApiError::BadRequest(format!("unknown kind: {other}"))),
    }
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    let payload = serde_json::json!({"outcome": format!("{outcome:?}")});
    Ok(json_ok(&payload))
}

#[derive(Debug, Deserialize)]
struct MaintenanceBody {
    #[serde(default = "default_scope")]
    scope: String,
}

async fn post_maintenance_lint(
    state: &Arc<ApiState>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, ApiError> {
    let body = read_json::<MaintenanceBody>(req).await?;
    let lint = jarvis_memory::lint::MemoryLint::new(state.db.clone());
    let report = lint.run(&body.scope).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(json_ok(&serde_json::json!({
        "duplicates_deprecated": report.duplicates_deprecated,
        "scratch_purged": report.scratch_purged,
        "inferences_expired": report.inferences_expired,
        "weak_lessons_demoted": report.weak_lessons_demoted,
        "conflicts_dampened": report.conflicts_dampened,
    })))
}

async fn read_json<T: for<'de> Deserialize<'de>>(req: Request<Incoming>) -> Result<T, ApiError> {
    let collected = req
        .into_body()
        .collect()
        .await
        .map_err(|e| ApiError::BadRequest(format!("body read: {e}")))?
        .to_bytes();
    serde_json::from_slice(&collected).map_err(|e| ApiError::BadRequest(format!("json: {e}")))
}

#[cfg(test)]
mod tests;
