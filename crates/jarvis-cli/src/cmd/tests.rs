use super::*;

fn fresh_db() -> Db {
    Db::in_memory().expect("in-memory db")
}

fn seed_session(db: &Db, id: &str) {
    let n = jarvis_core::time::now();
    jarvis_db::session_repo::upsert_session(
        db,
        &jarvis_core::session::Session {
            id: id.into(),
            title: "test".into(),
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
}

#[test]
fn cmd_route_returns_pretty_json() {
    let db = fresh_db();
    let out = cmd_route(&db, "openwrt 报错").unwrap();
    assert!(out.starts_with('{'));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["domain"], "devops");
    assert!(v["confidence"].as_f64().unwrap() < 1.0);
}

#[test]
fn cmd_route_rejects_empty_input() {
    let db = fresh_db();
    let err = cmd_route(&db, "   ").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_memory_write_and_list_round_trip() {
    let db = fresh_db();
    let written = cmd_memory_write(&db, "用户偏好函数式风格", "global").unwrap();
    assert!(written.starts_with("wrote mem_"));
    let lines = cmd_memory_list(&db, "global").unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("preference_memory"));
    assert!(lines[0].contains("函数式"));
}

#[test]
fn cmd_memory_write_rejects_empty_content() {
    let db = fresh_db();
    let err = cmd_memory_write(&db, "   ", "global").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_raw_log_returns_session_events() {
    let db = fresh_db();
    seed_session(&db, "sess_cli");
    jarvis_db::raw_event_log::append(
        &db,
        jarvis_db::raw_event_log::AppendEvent {
            event_type: jarvis_db::RawEventKind::UserMessage,
            session_id: Some("sess_cli"),
            trace_id: None,
            agent_id: None,
            raw_content: "hello",
            safe_content: None,
        },
    )
    .unwrap();
    let lines = cmd_raw_log(&db, "sess_cli", 10).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("user_message"));
    assert!(lines[0].contains("hello"));
}

#[test]
fn cmd_raw_log_rejects_missing_session() {
    let db = fresh_db();
    let err = cmd_raw_log(&db, "", 10).unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_audit_lists_session_audit_entries() {
    let db = fresh_db();
    jarvis_db::audit_log::append(
        &db,
        jarvis_db::audit_log::AppendAudit {
            trace_id: None,
            session_id: Some("sess_cli"),
            task_id: None,
            actor: "tool_runtime",
            action: "tool.call",
            target: Some("read_file"),
            status: jarvis_db::audit_log::AuditStatus::Success,
            input_summary: None,
            output_summary: Some("ok"),
            before_hash: None,
            after_hash: None,
            data_json: None,
        },
    )
    .unwrap();
    let lines = cmd_audit(&db, "sess_cli", 10).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("tool_runtime"));
    assert!(lines[0].contains("read_file"));
    assert!(lines[0].contains("success"));
}

#[test]
fn cmd_trace_view_pretty_prints_events_for_a_trace() {
    let db = fresh_db();
    jarvis_db::raw_event_log::append(
        &db,
        jarvis_db::raw_event_log::AppendEvent {
            event_type: jarvis_db::RawEventKind::UserMessage,
            session_id: Some("sess_cli"),
            trace_id: Some("trc_view"),
            agent_id: None,
            raw_content: "hi",
            safe_content: None,
        },
    )
    .unwrap();
    let lines = cmd_trace_view(&db, "trc_view").unwrap();
    assert!(lines[0].contains("trc_view"));
    assert!(lines.iter().any(|l| l.contains("user_message")));
}

#[test]
fn cmd_memory_history_returns_full_change_log() {
    let db = fresh_db();
    let mgr = MemoryManager::new(db.clone());
    let outcome = mgr
        .write(WriteRequest {
            r#type: MemoryType::PreferenceMemory,
            scope: "global",
            content: "x",
            entities: vec![],
            source_type: SourceType::UserExplicit,
            source_trace_id: None,
            tier: 1,
            emotion_energy: 0.0,
            emotion_polarity: EmotionPolarity::Neutral,
            reason: None,
        })
        .unwrap();
    let lines = cmd_memory_history(&db, &outcome.id).unwrap();
    assert!(lines.iter().any(|l| l.contains("created")));
}

#[test]
fn cmd_sessions_list_returns_active_sessions() {
    let db = fresh_db();
    seed_session(&db, "sess_a");
    seed_session(&db, "sess_b");
    let lines = cmd_sessions_list(&db).unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|l| l.contains("sess_a")));
}

#[test]
fn cmd_session_messages_returns_chronological() {
    let db = fresh_db();
    seed_session(&db, "sess_x");
    let n = jarvis_core::time::now();
    for i in 0..3 {
        jarvis_db::session_repo::append_message(
            &db,
            &jarvis_core::session::Message {
                id: format!("m{i}"),
                session_id: "sess_x".into(),
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
    let lines = cmd_session_messages(&db, "sess_x", 100).unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("[user"));
}

#[test]
fn cmd_skills_list_returns_registered_skills() {
    let db = fresh_db();
    let reg = jarvis_growth::SkillRegistry::new(db.clone());
    let mut skill = jarvis_growth::skill::new_skill("diagnose", "x");
    skill.success_count = 4;
    skill.failure_count = 0;
    skill.status = jarvis_growth::SkillStatus::Promoted;
    reg.upsert(&skill).unwrap();
    let lines = cmd_skills_list(&db).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("diagnose"));
    assert!(lines[0].contains("promoted"));
}

#[test]
fn cmd_walkthrough_list_and_approve_round_trip() {
    let db = fresh_db();
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = jarvis_orchestrator::walkthrough::new_draft(
        "st_x", "sess_w", "coding", "demo",
    );
    store.save(&doc).unwrap();
    let lines = cmd_walkthrough_list(&db, "sess_w").unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("approval=pending"));

    let approved = cmd_walkthrough_approve(&db, &doc.id, "tester").unwrap();
    assert!(approved.contains("approved"));
    assert!(approved.contains("tester"));
    let after = cmd_walkthrough_list(&db, "sess_w").unwrap();
    assert!(after[0].contains("approval=approved"));
}

#[test]
fn cmd_walkthrough_reject_records_actor() {
    let db = fresh_db();
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = jarvis_orchestrator::walkthrough::new_draft(
        "st_y", "sess_w2", "coding", "x",
    );
    store.save(&doc).unwrap();
    let out = cmd_walkthrough_reject(&db, &doc.id, "tester", Some("not ready")).unwrap();
    assert!(out.contains("rejected"));
    assert!(out.contains("tester"));
}

#[test]
fn cmd_outbox_pending_reports_zero_for_empty() {
    let db = fresh_db();
    let line = cmd_outbox_pending(&db).unwrap();
    assert!(line.contains("pending outbox rows: 0"));
}

#[test]
fn cmd_dashboard_summary_json_emits_valid_payload() {
    let db = fresh_db();
    seed_session(&db, "sess_j");
    cmd_memory_write(&db, "x", "global").unwrap();
    let raw = cmd_dashboard_summary_json(&db).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(v["active_sessions"].as_u64().unwrap() >= 1);
    assert_eq!(v["pending_outbox"].as_u64().unwrap(), 0);
}

#[test]
fn cmd_dashboard_summary_reports_seeded_state() {
    let db = fresh_db();
    seed_session(&db, "sess_cli");
    cmd_memory_write(&db, "test mem", "global").unwrap();
    let summary = cmd_dashboard_summary(&db).unwrap();
    assert!(summary.contains("active_sessions=1"));
    assert!(summary.contains("memories=1"));
    assert!(summary.contains("pending_outbox=0"));
}
