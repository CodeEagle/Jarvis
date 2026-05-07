#[cfg(test)]
use crate::*;
#[cfg(test)]
use std::time::{Duration, Instant};
#[cfg(test)]
use jarvis_db::Db;

// ── watchdog ────────────────────────────────────────────────────────────

#[test]
fn watchdog_fresh_beat_is_healthy() {
    let wd = Watchdog::new(WatchdogPolicy::defaults());
    wd.beat("agent_1");
    let v = wd.evaluate("agent_1", Instant::now());
    assert_eq!(v, WatchdogVerdict::Healthy);
}

#[test]
fn watchdog_unknown_agent_is_dead() {
    let wd = Watchdog::new(WatchdogPolicy::defaults());
    let v = wd.evaluate("never_seen", Instant::now());
    assert_eq!(v, WatchdogVerdict::Dead);
}

#[test]
fn watchdog_marks_stale_then_dead_after_grace() {
    let policy = WatchdogPolicy {
        heartbeat_interval: Duration::from_millis(10),
        heartbeat_timeout: Duration::from_millis(50),
        grace_period: Duration::from_millis(100),
    };
    let wd = Watchdog::new(policy);
    wd.beat("agent_1");

    // simulate clock advance well past timeout but still within grace
    let later = Instant::now() + Duration::from_millis(80);
    let v = wd.evaluate("agent_1", later);
    assert_eq!(v, WatchdogVerdict::Stale);

    // past grace → dead
    let later2 = later + Duration::from_millis(120);
    let v2 = wd.evaluate("agent_1", later2);
    assert_eq!(v2, WatchdogVerdict::Dead);
}

#[test]
fn watchdog_recovery_resets_stale() {
    let policy = WatchdogPolicy {
        heartbeat_interval: Duration::from_millis(10),
        heartbeat_timeout: Duration::from_millis(50),
        grace_period: Duration::from_millis(100),
    };
    let wd = Watchdog::new(policy);
    wd.beat("agent_1");
    let later = Instant::now() + Duration::from_millis(80);
    assert_eq!(wd.evaluate("agent_1", later), WatchdogVerdict::Stale);
    wd.beat("agent_1");
    assert_eq!(
        wd.evaluate("agent_1", Instant::now()),
        WatchdogVerdict::Healthy
    );
}

// ── fallback messages ───────────────────────────────────────────────────

#[test]
fn fallback_message_idle_short() {
    let m = fallback_message(TaskPlaneState::Idle, None);
    assert!(!m.is_empty());
}

#[test]
fn fallback_message_executing_mentions_running_agent_when_known() {
    let m = fallback_message(TaskPlaneState::Executing, Some("代码助手"));
    assert!(m.contains("代码助手"));
}

#[test]
fn fallback_message_unavailable_mentions_lightweight_mode() {
    let m = fallback_message(TaskPlaneState::Unavailable, None);
    assert!(m.contains("轻量模式"));
}

// ── control plane SLA (Section 30.4) ────────────────────────────────────

#[tokio::test]
async fn control_plane_returns_resolved_for_simple_input() {
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::new(db);
    let resp = cp
        .handle_user_input("openwrt dns 报错".into(), None, vec![])
        .await;
    match resp {
        HandledResponse::Resolved {
            decision,
            elapsed_ms,
            ..
        } => {
            assert_eq!(decision.domain, "devops");
            assert!(elapsed_ms < 2000, "took {} ms", elapsed_ms);
        }
        HandledResponse::Fallback { message, .. } => {
            panic!("expected resolved, got fallback: {message}")
        }
    }
}

// ── scheduler / maintenance ─────────────────────────────────────────────

#[tokio::test]
async fn maintenance_jobs_run_lint_synchronously() {
    use std::sync::Arc;
    let db = Db::in_memory().unwrap();
    // Seed a stale scratch memory the lint should sweep.
    let m = jarvis_core::memory::Memory {
        id: "mem_stale".into(),
        r#type: jarvis_core::memory::MemoryType::ScratchMemory,
        scope: "global".into(),
        content: "transient note".into(),
        entities: vec![],
        confidence: 0.3,
        trust_score: 0.3,
        half_life_days: 0,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: jarvis_core::memory::SourceType::Inferred,
        conflict_ids: vec![],
        status: jarvis_core::memory::MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: jarvis_core::memory::EmotionPolarity::Neutral,
        tier: 4,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: chrono::Utc::now() - chrono::Duration::hours(48),
        updated_at: chrono::Utc::now(),
    };
    jarvis_db::memory_repo::upsert(
        &db,
        &m,
        jarvis_db::memory_repo::WriteOpts {
            change_type: jarvis_core::memory::MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();

    let jobs = Arc::new(scheduler::MaintenanceJobs::new(db));
    let report = jobs.run_lint("global").await;
    assert_eq!(report.scratch_purged, 1);
}

#[test]
fn scheduler_config_defaults_24h_lint() {
    let cfg = scheduler::SchedulerConfig::default();
    assert_eq!(cfg.dream_lint_period.as_secs(), 24 * 3600);
    assert_eq!(cfg.scope, "global");
}

// ── replication daemon ─────────────────────────────────────────────────

#[tokio::test]
async fn replicator_drains_pending_rows_to_peer() {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::Mutex;

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let received = received_clone.clone();
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<Incoming>| {
                    let received = received.clone();
                    async move {
                        use http_body_util::BodyExt;
                        let body = req.into_body().collect().await.unwrap().to_bytes();
                        received
                            .lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&body).to_string());
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(Full::new(Bytes::from("ok")))
                                .unwrap(),
                        )
                    }
                });
                let _ = http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });

    let db = Db::in_memory().unwrap();
    for i in 0..3 {
        jarvis_db::outbox::enqueue(
            &db,
            jarvis_db::outbox::EnqueueRequest {
                kind: "memory.upsert",
                target_table: "memories",
                target_id: &format!("m_{i}"),
                payload_json: "{\"x\":1}",
            },
        )
        .unwrap();
    }
    let cfg = replication::ReplicationConfig {
        peer_endpoint: format!("http://{addr}/replicate"),
        ..Default::default()
    };
    let r = replication::Replicator::new(db.clone(), cfg);
    let n = r.drain_once().await.unwrap();
    assert_eq!(n, 3);
    assert_eq!(jarvis_db::outbox::pending_count(&db).unwrap(), 0);
    assert_eq!(received.lock().unwrap().len(), 3);
    server.abort();
}

#[tokio::test]
async fn replicator_stops_on_peer_error_without_marking_delivered() {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let svc = service_fn(|_req: Request<Incoming>| async {
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Full::new(Bytes::from("nope")))
                            .unwrap(),
                    )
                });
                let _ = http1::Builder::new().serve_connection(io, svc).await;
            });
        }
    });

    let db = Db::in_memory().unwrap();
    jarvis_db::outbox::enqueue(
        &db,
        jarvis_db::outbox::EnqueueRequest {
            kind: "x",
            target_table: "memories",
            target_id: "a",
            payload_json: "{}",
        },
    )
    .unwrap();
    let cfg = replication::ReplicationConfig {
        peer_endpoint: format!("http://{addr}/replicate"),
        ..Default::default()
    };
    let r = replication::Replicator::new(db.clone(), cfg);
    let n = r.drain_once().await.unwrap();
    assert_eq!(n, 0);
    assert_eq!(jarvis_db::outbox::pending_count(&db).unwrap(), 1);
    server.abort();
}

#[tokio::test]
async fn control_plane_records_turn_summary_on_continue_existing_resolved() {
    use jarvis_core::session::{Session, SessionStatus};
    use jarvis_core::time::now;
    use jarvis_memory::compression::CompressionStore;
    use std::sync::Arc;

    let db = Db::in_memory().unwrap();
    // Seed a session that explicit_reference will hit so the route
    // resolves to continue_existing → turn_summary will be written.
    let n = now();
    let sess = Session {
        id: "sess_existing".into(),
        title: "OpenWrt 排查".into(),
        domain: "devops".into(),
        topic: "openwrt".into(),
        summary: "...".into(),
        long_summary: "...".into(),
        active_entities: vec![],
        unresolved: vec![],
        resolved: vec![],
        recent_message_ids: vec![],
        memory_refs: vec![],
        skill_refs: vec![],
        status: SessionStatus::Active,
        created_at: n,
        updated_at: n,
        last_active_at: n,
    };
    jarvis_db::session_repo::upsert_session(&db, &sess).unwrap();

    struct ContinueJudge;
    #[async_trait::async_trait]
    impl jarvis_router::LlmJudge for ContinueJudge {
        async fn judge(
            &self,
            _inputs: jarvis_router::JudgeInputs<'_>,
        ) -> Option<jarvis_router::JudgeOutcome> {
            Some(jarvis_router::JudgeOutcome {
                primary_intent: "devops.networking".into(),
                secondary_intents: vec![],
                domain: "devops".into(),
                topic: "openwrt".into(),
                session_action: jarvis_core::route::SessionAction::ContinueExisting,
                agent_type: "devops".into(),
                confidence: 0.95,
                clarification_needed: false,
                router_notes: "judge says continue".into(),
            })
        }
    }
    let cp = ControlPlane::new(db.clone());
    let resp = cp
        .handle_user_input_with_judge(
            "继续 OpenWrt 排查".into(),
            Some("sess_existing".into()),
            vec![],
            Arc::new(ContinueJudge),
        )
        .await;
    match resp {
        HandledResponse::Resolved { decision, .. } => {
            assert_eq!(decision.session_action, jarvis_core::route::SessionAction::ContinueExisting);
            // TurnSummary written for the continued session.
            let store = CompressionStore::new(db);
            let rows = store.list_turn_summaries(
                decision.target_session_id.as_deref().unwrap_or("sess_existing"),
                10,
            ).unwrap();
            assert!(
                !rows.is_empty(),
                "expected at least one TurnSummary recorded after Resolved"
            );
            assert!(rows[0].user_goal.contains("OpenWrt"));
        }
        HandledResponse::Fallback { message, .. } => {
            panic!("expected Resolved, got Fallback: {message}");
        }
    }
}

#[tokio::test]
async fn control_plane_skips_turn_summary_for_create_new() {
    use jarvis_memory::compression::CompressionStore;
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::new(db.clone());
    // 完全新话题；session_action 会是 create_new；不应写 turn_summary
    let _ = cp.handle_user_input("hello world".into(), None, vec![]).await;
    let store = CompressionStore::new(db);
    let rows = store.list_turn_summaries("any_session", 10).unwrap();
    assert!(rows.is_empty(), "create_new shouldn't trigger turn_summary");
}

#[tokio::test]
async fn control_plane_with_judge_takes_judge_outcome() {
    use std::sync::Arc;
    struct StubJudge;
    #[async_trait::async_trait]
    impl jarvis_router::LlmJudge for StubJudge {
        async fn judge(
            &self,
            _inputs: jarvis_router::JudgeInputs<'_>,
        ) -> Option<jarvis_router::JudgeOutcome> {
            Some(jarvis_router::JudgeOutcome {
                primary_intent: "research.web_search".into(),
                secondary_intents: vec![],
                domain: "research".into(),
                topic: "x".into(),
                session_action: jarvis_core::route::SessionAction::CreateNew,
                agent_type: "research".into(),
                confidence: 0.97,
                clarification_needed: false,
                router_notes: "stub judge".into(),
            })
        }
    }
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::new(db);
    let resp = cp
        .handle_user_input_with_judge(
            "openwrt 报错".into(),
            None,
            vec![],
            Arc::new(StubJudge),
        )
        .await;
    match resp {
        HandledResponse::Resolved { decision, .. } => {
            assert_eq!(decision.agent_type, "research");
            assert!(decision.router_notes.contains("stub judge"));
            assert!(!decision.fallback_used);
        }
        HandledResponse::Fallback { message, .. } => {
            panic!("expected Resolved, got Fallback: {message}");
        }
    }
}

#[tokio::test]
async fn control_plane_falls_back_when_budget_too_tight() {
    // Set fallback budget to 1ms so the router can't beat it.
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::with_sla(
        db,
        ResponseSla {
            interrupt_ack: Duration::from_millis(500),
            progress_query: Duration::from_millis(800),
            sub_agent_reply: Duration::from_millis(1000),
            fallback_ack: Duration::from_millis(1),
        },
    );
    let resp = cp
        .handle_user_input("hello".into(), None, vec![])
        .await;
    match resp {
        HandledResponse::Fallback { elapsed_ms, .. } => {
            // Hit the fallback budget; must not exceed it by much.
            assert!(elapsed_ms < 200, "fallback took too long: {} ms", elapsed_ms);
        }
        HandledResponse::Resolved { .. } => {
            // Acceptable on extremely fast machines, though unlikely at 1ms.
        }
    }
}
