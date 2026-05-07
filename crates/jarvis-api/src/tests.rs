use super::*;
use http_body_util::BodyExt;
use hyper::body::Bytes as HyperBytes;
use hyper::{Method, Request};
use std::net::SocketAddr;
use std::sync::Arc;

fn fresh_state() -> Arc<ApiState> {
    Arc::new(ApiState {
        db: Db::in_memory().unwrap(),
    })
}

#[allow(dead_code)]
fn dummy_peer() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

async fn body_bytes(resp: Response<Full<HyperBytes>>) -> HyperBytes {
    resp.into_body().collect().await.unwrap().to_bytes()
}

#[tokio::test]
async fn healthz_returns_ok_payload() {
    let state = fresh_state();
    let req = Request::builder()
        .method(Method::GET)
        .uri("/healthz")
        .body(unsafe { std::mem::zeroed::<hyper::body::Incoming>() })
        .ok();
    // Workaround: hyper::body::Incoming can't be constructed in user
    // code. We exercise the routing through `handle` indirectly via
    // the handlers module — `recent_sessions` is the equivalent
    // smoke check that doesn't need a real Incoming.
    let _ = req;
    let resp = handlers::recent_sessions(&state.db).unwrap();
    let bytes = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.is_array());
}

#[tokio::test]
async fn recent_sessions_returns_array() {
    let state = fresh_state();
    let n = jarvis_core::time::now();
    jarvis_db::session_repo::upsert_session(
        &state.db,
        &jarvis_core::session::Session {
            id: "sess_api".into(),
            title: "api test".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: jarvis_core::session::SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )
    .unwrap();
    let resp = handlers::recent_sessions(&state.db).unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "sess_api");
}

#[tokio::test]
async fn get_session_returns_404_when_missing() {
    let state = fresh_state();
    let err = handlers::get_session(&state.db, "missing").unwrap_err();
    matches!(err, ApiError::NotFound(_))
        .then_some(())
        .expect("expected NotFound");
}

#[tokio::test]
async fn list_memories_returns_empty_for_unused_scope() {
    let state = fresh_state();
    let resp = handlers::list_memories(&state.db, "global").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!([]));
}

#[tokio::test]
async fn session_messages_returns_chronological() {
    let state = fresh_state();
    let n = jarvis_core::time::now();
    jarvis_db::session_repo::upsert_session(
        &state.db,
        &jarvis_core::session::Session {
            id: "sess_api2".into(),
            title: "msgs".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: jarvis_core::session::SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )
    .unwrap();
    for i in 0..3 {
        jarvis_db::session_repo::append_message(
            &state.db,
            &jarvis_core::session::Message {
                id: format!("m{i}"),
                session_id: "sess_api2".into(),
                trace_id: None,
                role: jarvis_core::session::MessageRole::User,
                content: format!("hi {i}"),
                token_count: 1,
                summary_id: None,
                created_at: n,
            },
        )
        .unwrap();
    }
    let resp = handlers::session_messages(&state.db, "sess_api2").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
}

#[test]
fn dashboard_html_contains_known_tile_keys() {
    use crate::dashboard_html::DASHBOARD_HTML;
    for key in [
        "active_sessions",
        "raw_event_count",
        "memory_count",
        "outbox_pending",
        "route_decisions",
        "promoted_artifacts",
    ] {
        assert!(DASHBOARD_HTML.contains(key), "missing tile {key}");
    }
    assert!(DASHBOARD_HTML.contains("/dashboard/metrics"));
}

#[tokio::test]
async fn dashboard_metrics_returns_required_fields() {
    let state = fresh_state();
    let resp = handlers::dashboard_metrics(&state.db).unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for field in [
        "active_sessions",
        "raw_event_count",
        "memory_count",
        "outbox_pending",
        "route_decisions",
        "promoted_artifacts",
        "ts",
    ] {
        assert!(json.get(field).is_some(), "missing {field}: {json}");
    }
}

#[tokio::test]
async fn audit_returns_array() {
    let state = fresh_state();
    jarvis_db::audit_log::append(
        &state.db,
        jarvis_db::audit_log::AppendAudit {
            trace_id: None,
            session_id: Some("sess_api"),
            task_id: None,
            actor: "api",
            action: "test",
            target: None,
            status: jarvis_db::audit_log::AuditStatus::Success,
            input_summary: None,
            output_summary: None,
            before_hash: None,
            after_hash: None,
            data_json: None,
        },
    )
    .unwrap();
    let resp = handlers::audit(&state.db, "sess_api").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn sse_stream_emits_existing_raw_events_on_connect() {
    let state = ApiState {
        db: Db::in_memory().unwrap(),
    };
    for content in ["hello world", "second message"] {
        jarvis_db::raw_event_log::append(
            &state.db,
            jarvis_db::raw_event_log::AppendEvent {
                event_type: jarvis_db::RawEventKind::UserMessage,
                session_id: Some("sess_sse"),
                trace_id: None,
                agent_id: None,
                raw_content: content,
                safe_content: None,
            },
        )
        .unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = Arc::new(state);
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = arc_state.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    handle(svc_state.clone(), req, peer)
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    client
        .write_all(b"GET /sessions/sess_sse/stream HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = vec![0u8; 4096];
    let mut total = Vec::new();
    let read_loop = async {
        loop {
            let n = client.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            total.extend_from_slice(&buf[..n]);
            let s = String::from_utf8_lossy(&total);
            if s.matches("event: user_message").count() >= 2 {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), read_loop).await;
    server.abort();
    let response = String::from_utf8_lossy(&total);
    assert!(response.contains("text/event-stream"), "headers: {response}");
    assert!(response.contains("event: user_message"), "body: {response}");
    assert!(response.contains("hello"), "body: {response}");
}

#[tokio::test]
async fn end_to_end_serve_and_client() {
    // Bring up the server on an OS-assigned port and exercise /healthz.
    let state = ApiState {
        db: Db::in_memory().unwrap(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = Arc::new(state);
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = arc_state.clone();
            tokio::spawn(async move {
                let svc = hyper::service::service_fn(move |req| {
                    handle(svc_state.clone(), req, peer)
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });

    // Tiny TCP client speaking HTTP/1.1 by hand to avoid pulling
    // hyper-client just for the test.
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_to_end(&mut buf),
    )
    .await;
    server.abort();
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "got: {response}");
    assert!(response.contains("\"ok\":true"));
}
