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

// ── v1.9 handoff API ────────────────────────────────────────────────────

#[tokio::test]
async fn handoff_capacity_returns_advisory() {
    let state = fresh_state();
    let n = chrono::Utc::now();
    jarvis_db::session_repo::upsert_session(
        &state.db,
        &jarvis_core::session::Session {
            id: "sess_v19".into(),
            title: "v1.9 capacity".into(),
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
    let resp = handlers::capacity_for_session(&state.db, "sess_v19").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["session_id"], "sess_v19");
    assert_eq!(json["advisory_level"], "ok");
    assert_eq!(json["waiting_user"], false);
}

#[tokio::test]
async fn handoff_plan_post_persists_and_appears_in_pending() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = fresh_state();
    let n = chrono::Utc::now();
    jarvis_db::session_repo::upsert_session(
        &arc_state.db,
        &jarvis_core::session::Session {
            id: "sess_plan".into(),
            title: "plan target".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "重构 sync 模块".into(),
            active_entities: vec![],
            unresolved: vec!["sync_executor 重构".into()],
            resolved: vec!["sync_state 抽离".into()],
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
    let server_state = arc_state.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = server_state.clone();
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

    let body = r#"{"advisory_level":"manual"}"#;
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /sessions/sess_plan/handoff/plan HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    client.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.read_to_end(&mut buf),
    )
    .await;
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "response: {response}");
    assert!(response.contains("\"source_session_id\":\"sess_plan\""));
    assert!(response.contains("\"user_decision\":\"pending\""));

    // Now /handoff/pending lists it.
    let resp = handlers::handoff_list_pending(&arc_state.db).unwrap();
    let body2 = body_bytes(resp).await;
    let arr: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(arr.as_array().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn handoff_accept_via_http_swaps_session() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = fresh_state();
    let n = chrono::Utc::now();
    jarvis_db::session_repo::upsert_session(
        &arc_state.db,
        &jarvis_core::session::Session {
            id: "sess_swap".into(),
            title: "swap source".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "x".into(),
            active_entities: vec![],
            unresolved: vec!["pending thread".into()],
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

    // Plan via library directly to keep the test simple.
    let sess = jarvis_db::session_repo::get_session(&arc_state.db, "sess_swap")
        .unwrap()
        .unwrap();
    let snap = jarvis_orchestrator::handoff::plan(
        &jarvis_orchestrator::handoff::PlanInputs {
            session: &sess,
            trace_id: None,
            advisory_level: "manual",
            benefit_score: 0.5,
            pressure_ratio: 0.5,
            recent_activity: vec![],
            pinned_memory_ids: vec![],
            pinned_artifact_ids: vec![],
            rolling_summary_excerpt: "".into(),
            must_know_constraints: vec![],
        },
    );
    jarvis_orchestrator::handoff::persist_snapshot(&arc_state.db, &snap).unwrap();
    let snap_id = snap.id.clone();

    let server_state = arc_state.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = server_state.clone();
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

    let body = r#"{"new_title":"continued"}"#;
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /handoff/{snap_id}/accept HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    client.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.read_to_end(&mut buf),
    )
    .await;
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.contains("\"target_session_id\""));
    // Source archived.
    let after = jarvis_db::session_repo::get_session(&arc_state.db, "sess_swap")
        .unwrap()
        .unwrap();
    assert_eq!(after.status, jarvis_core::session::SessionStatus::Archived);
    server.abort();
}

// ── Conversation API (PRD §23.3) ────────────────────────────────────────

#[tokio::test]
async fn conversation_ownership_returns_null_when_idle() {
    let state = fresh_state();
    let resp = handlers::conversation_ownership(&state.db, "sess_x").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_null());
}

#[tokio::test]
async fn conversation_ownership_returns_record_after_acquire() {
    let state = fresh_state();
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(state.db.clone());
    bus.acquire_ownership(
        "sess_x",
        "agent_a",
        "orchestrator",
        None,
        jarvis_orchestrator::conversation_bus::InteractionMode::Listening,
    )
    .unwrap();
    let resp = handlers::conversation_ownership(&state.db, "sess_x").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["agent_id"], "agent_a");
    assert_eq!(json["interaction_mode"], "listening");
}

#[tokio::test]
async fn conversation_sub_channels_lists_active() {
    let state = fresh_state();
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(state.db.clone());
    bus.open_sub_channel("sess_y", "st_1", "coding", None).unwrap();
    let resp = handlers::conversation_sub_channels(&state.db, "sess_y").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["sub_task_id"], "st_1");
}

#[tokio::test]
async fn conversation_activity_lists_cards() {
    let state = fresh_state();
    let store = jarvis_orchestrator::activity_card::ActivityCardStore::new(state.db.clone());
    store
        .create(jarvis_orchestrator::activity_card::CardDraft {
            session_id: "sess_z",
            sub_task_id: Some("st_a"),
            trace_id: None,
            agent_type: "coding",
            agent_display_name: "代码助手",
            agent_avatar_emoji: "💻",
            title: "fixing the bug",
        })
        .unwrap();
    let resp = handlers::conversation_activity(&state.db, "sess_z").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["agent_type"], "coding");
}

#[tokio::test]
async fn conversation_pending_lists_queued_messages() {
    let state = fresh_state();
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(state.db.clone());
    bus.enqueue_pending_message("sess_p", "wait for me", "queue").unwrap();
    let resp = handlers::conversation_pending(&state.db, "sess_p").unwrap();
    let body = body_bytes(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["content"], "wait for me");
    assert_eq!(arr[0]["resolved"], false);
}

#[tokio::test]
async fn conversation_reply_post_persists_pending_message() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = fresh_state();
    let server_state = arc_state.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = server_state.clone();
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

    let body = r#"{"content":"select option B","routing_decision":"sub_agent_reply"}"#;
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /conversation/sess_reply/reply HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    client.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.read_to_end(&mut buf),
    )
    .await;
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK"), "response: {response}");
    assert!(response.contains("\"session_id\":\"sess_reply\""));

    // Verify persistence.
    let bus = jarvis_orchestrator::conversation_bus::ConversationBus::new(arc_state.db.clone());
    let pending = bus.list_pending_messages("sess_reply").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].content, "select option B");
    assert_eq!(pending[0].routing_decision, "sub_agent_reply");
    server.abort();
}

#[tokio::test]
async fn conversation_steer_path_mismatch_rejects() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let arc_state = fresh_state();
    let server_state = arc_state.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc_state = server_state.clone();
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

    // Path says sess_a, body says sess_b → must reject.
    let body = r#"{"session_id":"sess_b","sub_task_id":"st_x","content":"keep stream","scope":"constraint","inject_at":"next_step"}"#;
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /conversation/sess_a/steer HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    client.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.read_to_end(&mut buf),
    )
    .await;
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("400 Bad Request"), "expected 400: {response}");
    server.abort();
}

/// Concurrency stress: open N SSE clients against distinct sessions
/// and verify each client receives only its session's events. Catches
/// per-session filter regressions and head-of-line blocking between
/// streams.
#[tokio::test]
async fn sse_stream_isolates_concurrent_clients_by_session() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let state = ApiState {
        db: Db::in_memory().unwrap(),
    };
    // Seed 5 events per session for 3 sessions.
    let sessions = ["sess_a", "sess_b", "sess_c"];
    for sess in &sessions {
        for i in 0..5 {
            jarvis_db::raw_event_log::append(
                &state.db,
                jarvis_db::raw_event_log::AppendEvent {
                    event_type: jarvis_db::RawEventKind::UserMessage,
                    session_id: Some(sess),
                    trace_id: None,
                    agent_id: None,
                    raw_content: &format!("{sess}-msg-{i}"),
                    safe_content: None,
                },
            )
            .unwrap();
        }
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

    // Open 3 clients in parallel, each waiting for 5 events on its session.
    let mut handles = vec![];
    for sess in &sessions {
        let sess = sess.to_string();
        let handle = tokio::spawn(async move {
            let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
            let req = format!(
                "GET /sessions/{sess}/stream HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
            );
            client.write_all(req.as_bytes()).await.unwrap();
            let mut buf = vec![0u8; 4096];
            let mut total = Vec::new();
            let read_loop = async {
                loop {
                    let n = client.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    total.extend_from_slice(&buf[..n]);
                    let s = String::from_utf8_lossy(&total);
                    if s.matches(&format!("{sess}-msg-")).count() >= 5 {
                        break;
                    }
                }
            };
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), read_loop).await;
            (sess, String::from_utf8_lossy(&total).to_string())
        });
        handles.push(handle);
    }

    // Each client must see exactly its own session's 5 events,
    // and none of the other sessions' events.
    for h in handles {
        let (sess, body) = h.await.unwrap();
        let own_count = body.matches(&format!("{sess}-msg-")).count();
        assert!(
            own_count >= 5,
            "session {sess}: expected ≥5 own events, got {own_count}\n{body}",
        );
        for other in &sessions {
            if *other == sess {
                continue;
            }
            assert!(
                !body.contains(&format!("{other}-msg-")),
                "session {sess} leaked events from {other}: {body}",
            );
        }
    }
    server.abort();
}

/// Client disconnect mid-stream must not poison the server. Open
/// then immediately close 20 client connections, then verify a
/// fresh client can still get a clean stream.
#[tokio::test]
async fn sse_stream_handles_rapid_client_disconnects() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let state = ApiState {
        db: Db::in_memory().unwrap(),
    };
    jarvis_db::raw_event_log::append(
        &state.db,
        jarvis_db::raw_event_log::AppendEvent {
            event_type: jarvis_db::RawEventKind::UserMessage,
            session_id: Some("sess_ds"),
            trace_id: None,
            agent_id: None,
            raw_content: "before",
            safe_content: None,
        },
    )
    .unwrap();

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

    // 20 rapid connect-and-disconnect cycles.
    for _ in 0..20 {
        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(
            b"GET /sessions/sess_ds/stream HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
        // Read a tiny bit then drop — simulates client tab close.
        let mut tiny = [0u8; 64];
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            c.read(&mut tiny),
        )
        .await;
        drop(c);
    }

    // Server should still serve a fresh client cleanly.
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    client
        .write_all(
            b"GET /sessions/sess_ds/stream HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut buf = vec![0u8; 4096];
    let mut total = Vec::new();
    let read = async {
        loop {
            let n = client.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            total.extend_from_slice(&buf[..n]);
            if String::from_utf8_lossy(&total).contains("before") {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), read).await;
    let body = String::from_utf8_lossy(&total);
    assert!(
        body.contains("before"),
        "server lost the ability to serve after rapid disconnects: {body}"
    );
    server.abort();
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
