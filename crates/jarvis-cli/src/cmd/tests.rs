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

#[tokio::test]
async fn cmd_route_with_judge_uses_judge_outcome() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-cli-judge-{}",
        jarvis_core::ids::new_id_with_prefix("t")
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let bin = tmp.join("codex");
    let body = r#"{"primary_intent":"research.web_search","secondary_intents":[],"domain":"research","topic":"x","session_action":"create_new","agent_type":"research","confidence":0.95,"clarification_needed":false,"router_notes":"cli judge"}"#;
    let script = format!(
        "#!/usr/bin/env bash\nset -e\nout=\nprev=\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\ncat > \"$out\" <<'EOF_FAKE'\n{body}\nEOF_FAKE\n"
    );
    std::fs::write(&bin, script).unwrap();
    let mut perms = std::fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&bin, perms).unwrap();

    let cfg = jarvis_codex::CodexConfig {
        binary: bin.clone(),
        model: None,
        timeout: std::time::Duration::from_secs(15),
    };
    let judge = jarvis_codex::CodexJudge::new(cfg);
    let db = fresh_db();
    let pretty = cmd_route_with_judge(&db, "research the openwrt build system", &judge)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    assert_eq!(v["agent_type"], "research");
    assert!(v["router_notes"].as_str().unwrap().contains("cli judge"));
    let _ = std::fs::remove_dir_all(&tmp);
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
    // id surfaces in list output so it can be piped to forget /
    // memory-history without a separate search step.
    assert!(
        lines[0].contains("mem_"),
        "list line must include memory id: {}",
        lines[0]
    );
}

#[test]
fn cmd_memory_search_ranks_query_relevant_first() {
    let db = fresh_db();
    cmd_memory_write(&db, "用户偏好函数式编程", "global").unwrap();
    cmd_memory_write(&db, "周末喜欢做菜", "global").unwrap();
    let lines = cmd_memory_search(&db, "global", "函数式", 5).unwrap();
    assert!(!lines.is_empty(), "expected hits for '函数式'");
    assert!(lines[0].contains("函数式"), "top hit should match query: {lines:?}");
}

#[test]
fn cmd_memory_search_rejects_empty_query() {
    let db = fresh_db();
    let err = cmd_memory_search(&db, "global", "   ", 5).unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_memory_forget_marks_deprecated_and_logs_change() {
    let db = fresh_db();
    let written = cmd_memory_write(&db, "to be forgotten", "global").unwrap();
    let id = written
        .split_whitespace()
        .nth(1)
        .expect("wrote <id> ...")
        .to_string();
    let out = cmd_memory_forget(&db, &id, Some("user requested")).unwrap();
    assert!(out.contains("deprecated"));
    // List excludes deprecated memories.
    let listed = cmd_memory_list(&db, "global").unwrap();
    assert!(
        !listed.iter().any(|l| l.contains("forgotten")),
        "deprecated memory should not be listed: {listed:?}"
    );
    // History records the deprecation.
    let history = cmd_memory_history(&db, &id).unwrap();
    assert!(
        history.iter().any(|l| l.contains("deprecated")),
        "history should record the deprecate event: {history:?}"
    );
}

#[test]
fn cmd_memory_forget_returns_error_for_unknown_id() {
    let db = fresh_db();
    let err = cmd_memory_forget(&db, "mem_does_not_exist", None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not found"), "got: {msg}");
}

#[test]
fn cmd_verifier_list_returns_saved_checks() {
    let db = fresh_db();
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = jarvis_orchestrator::walkthrough::new_draft("st_v", "sess_v", "coding", "demo");
    store.save(&doc).unwrap();

    let vstore = jarvis_orchestrator::verifier::VerifierStore::new(db.clone());
    let mut check = jarvis_orchestrator::verifier::VerifierCheck::new(
        &doc.id,
        "sec_1",
        jarvis_orchestrator::verifier::CheckType::FileExists,
        "lib/sync.dart",
    );
    check.r#match = Some(true);
    vstore.save(&check).unwrap();

    let lines = cmd_verifier_list(&db, &doc.id).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("file_exists"));
    assert!(lines[0].contains("lib/sync.dart"));
}

#[test]
fn cmd_verifier_status_shows_doc_state() {
    let db = fresh_db();
    let store = jarvis_orchestrator::walkthrough::WalkthroughStore::new(db.clone());
    let doc = jarvis_orchestrator::walkthrough::new_draft("st_w", "sess_w", "coding", "demo");
    store.save(&doc).unwrap();

    let line = cmd_verifier_status(&db, &doc.id).unwrap();
    assert!(line.contains("verification=pending"));
    assert!(line.contains("approval=pending"));
}

#[test]
fn cmd_verifier_status_unknown_doc_errors() {
    let db = fresh_db();
    let err = cmd_verifier_status(&db, "doc_unknown").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn cmd_regression_latest_when_empty() {
    let db = fresh_db();
    let line = cmd_regression_latest(&db).unwrap();
    assert!(line.contains("(no regression report yet)"));
}

#[test]
fn cmd_regression_list_when_empty() {
    let db = fresh_db();
    let lines = cmd_regression_list(&db, 10).unwrap();
    assert!(lines.is_empty());
}

#[test]
fn cmd_commands_list_returns_default_catalogue() {
    let lines = cmd_commands_list().unwrap();
    assert!(!lines.is_empty(), "default catalogue must have rows");
    let ids: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    // PRD §8.17.2 default ids
    assert!(ids.iter().any(|i| *i == "sync_and_resolve"));
    assert!(ids.iter().any(|i| *i == "regression_check"));
    assert!(ids.iter().any(|i| *i == "parallel_explore"));
}

#[test]
fn cmd_commands_list_json_round_trip() {
    let json = cmd_commands_list_json().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let arr = v.as_array().unwrap();
    assert!(arr.iter().any(|c| c["id"] == "regression_check"));
}

#[test]
fn cmd_commands_run_creates_execution_row() {
    let db = fresh_db();
    seed_session(&db, "sess_cmd");
    let line = cmd_commands_run(&db, "sync_and_resolve", "sess_cmd", "user_button").unwrap();
    assert!(line.starts_with("cmd_"));
    assert!(line.contains("status=running"));
    assert!(line.contains("steps_pending=3")); // 3 steps in default catalogue
}

#[test]
fn cmd_commands_run_unknown_command_id_errors() {
    let db = fresh_db();
    seed_session(&db, "sess_cmd");
    let err = cmd_commands_run(&db, "no_such_command", "sess_cmd", "user").unwrap_err();
    let msg = err.to_string();
    assert!(msg.to_lowercase().contains("command") || msg.contains("not"));
}

#[test]
fn cmd_commands_run_rejects_empty_args() {
    let db = fresh_db();
    let err1 = cmd_commands_run(&db, "  ", "sess_x", "user").unwrap_err();
    matches!(err1, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg for command_id");
    let err2 = cmd_commands_run(&db, "sync_and_resolve", "  ", "user").unwrap_err();
    matches!(err2, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg for session_id");
}

#[test]
fn cmd_commands_status_round_trip() {
    let db = fresh_db();
    seed_session(&db, "sess_cmd");
    let line = cmd_commands_run(&db, "sync_and_resolve", "sess_cmd", "user").unwrap();
    let id = line.split_whitespace().next().unwrap();
    let status_out = cmd_commands_status(&db, id).unwrap();
    assert!(status_out.contains("status=running"));
    assert!(status_out.contains("step 0"));
    assert!(status_out.contains("agent=devops"));
}

#[test]
fn cmd_commands_status_unknown_errors() {
    let db = fresh_db();
    let err = cmd_commands_status(&db, "cmd_nope").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn cmd_commands_recent_lists_executions_newest_first() {
    let db = fresh_db();
    seed_session(&db, "sess_a");
    seed_session(&db, "sess_b");
    cmd_commands_run(&db, "sync_and_resolve", "sess_a", "user").unwrap();
    cmd_commands_run(&db, "regression_check", "sess_b", "user").unwrap();
    cmd_commands_run(&db, "unblock_agent", "sess_a", "user").unwrap();

    // No filter — all sessions.
    let all = cmd_commands_recent(&db, None, 10).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all[0].contains("unblock_agent")); // newest first

    // Filter to one session.
    let only_a = cmd_commands_recent(&db, Some("sess_a"), 10).unwrap();
    assert_eq!(only_a.len(), 2);
    for line in &only_a {
        assert!(line.contains("session=sess_a"));
    }
}

#[tokio::test]
async fn cmd_judge_probe_reports_outcome_for_stub_judge() {
    struct OkJudge;
    #[async_trait::async_trait]
    impl jarvis_router::LlmJudge for OkJudge {
        async fn judge(
            &self,
            _inputs: jarvis_router::JudgeInputs<'_>,
        ) -> Option<jarvis_router::JudgeOutcome> {
            Some(jarvis_router::JudgeOutcome {
                primary_intent: "chat".into(),
                secondary_intents: vec![],
                domain: "chat".into(),
                topic: "x".into(),
                session_action: jarvis_core::route::SessionAction::CreateNew,
                agent_type: "general".into(),
                confidence: 0.7,
                clarification_needed: false,
                router_notes: "probe ok".into(),
            })
        }
    }
    let line = cmd_judge_probe(&OkJudge).await.unwrap();
    assert!(line.contains("judge probe ok"));
    assert!(line.contains("agent=general"));
}

#[tokio::test]
async fn cmd_judge_probe_returns_err_when_adapter_unavailable() {
    struct DownJudge;
    #[async_trait::async_trait]
    impl jarvis_router::LlmJudge for DownJudge {
        async fn judge(
            &self,
            _inputs: jarvis_router::JudgeInputs<'_>,
        ) -> Option<jarvis_router::JudgeOutcome> {
            None
        }
    }
    let err = cmd_judge_probe(&DownJudge).await.unwrap_err();
    assert!(err.to_string().contains("FAILED"));
}

#[test]
fn cmd_sessions_new_creates_active_session() {
    let db = fresh_db();
    let out = cmd_sessions_new(&db, "Test Session", "coding").unwrap();
    assert!(out.starts_with("created sess_"));
    let listed = cmd_sessions_list(&db).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].contains("Test Session"));
    assert!(listed[0].contains("domain=coding"));
}

#[test]
fn cmd_sessions_new_rejects_empty_title() {
    let db = fresh_db();
    let err = cmd_sessions_new(&db, "  ", "coding").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_sessions_archive_hides_from_list() {
    let db = fresh_db();
    seed_session(&db, "sess_to_archive");
    let lines_before = cmd_sessions_list(&db).unwrap();
    assert_eq!(lines_before.len(), 1);
    let out = cmd_sessions_archive(&db, "sess_to_archive").unwrap();
    assert!(out.contains("archived"));
    let lines_after = cmd_sessions_list(&db).unwrap();
    assert!(
        lines_after.is_empty(),
        "archived session should not appear: {lines_after:?}"
    );
    // Idempotent.
    let again = cmd_sessions_archive(&db, "sess_to_archive").unwrap();
    assert!(again.contains("already archived"));
}

#[test]
fn cmd_sessions_archive_unknown_id_errors() {
    let db = fresh_db();
    let err = cmd_sessions_archive(&db, "sess_nope").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn cmd_persona_get_returns_empty_marker_when_absent() {
    let db = fresh_db();
    let line = cmd_persona_get(&db, "global").unwrap();
    assert!(line.contains("(no persona"));
}

#[test]
fn cmd_persona_set_get_round_trip_json() {
    let db = fresh_db();
    cmd_persona_set(&db, "global", r#"{"name":"jarvis","style":"terse"}"#).unwrap();
    let got = cmd_persona_get(&db, "global").unwrap();
    assert!(got.contains("scope=global"));
    assert!(got.contains("\"name\":\"jarvis\""));
}

#[test]
fn cmd_persona_set_wraps_plain_text_as_json_string() {
    let db = fresh_db();
    cmd_persona_set(&db, "global", "easygoing assistant").unwrap();
    let got = cmd_persona_get(&db, "global").unwrap();
    // Plain text should round-trip as a JSON string literal.
    assert!(got.contains("\"easygoing assistant\""));
}

#[test]
fn cmd_persona_set_rejects_empty_content() {
    let db = fresh_db();
    let err = cmd_persona_set(&db, "global", "  ").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_activity_cards_lists_session_cards() {
    let db = fresh_db();
    seed_session(&db, "sess_cards");
    let store = jarvis_orchestrator::activity_card::ActivityCardStore::new(db.clone());
    store
        .create(jarvis_orchestrator::activity_card::CardDraft {
            session_id: "sess_cards",
            sub_task_id: None,
            trace_id: None,
            agent_type: "coding",
            agent_display_name: "Coder",
            agent_avatar_emoji: "🛠",
            title: "fixing the bug",
        })
        .unwrap();
    let lines = cmd_activity_cards(&db, "sess_cards").unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("agent=coding"));
    assert!(lines[0].contains("fixing the bug"));
}

#[test]
fn cmd_activity_cards_rejects_empty_session() {
    let db = fresh_db();
    let err = cmd_activity_cards(&db, "  ").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

#[test]
fn cmd_memory_write_rejects_empty_content() {
    let db = fresh_db();
    let err = cmd_memory_write(&db, "   ", "global").unwrap_err();
    matches!(err, CmdError::MissingArg(_))
        .then_some(())
        .expect("expected MissingArg");
}

// ── v1.9 handoff CLI ────────────────────────────────────────────────────

#[test]
fn cmd_sessions_capacity_returns_advisory_for_known_session() {
    let db = fresh_db();
    seed_session(&db, "sess_cap");
    let line = cmd_sessions_capacity(&db, "sess_cap").unwrap();
    assert!(line.contains("session=sess_cap"));
    assert!(line.contains("advisory="));
    assert!(line.contains("pressure="));
    assert!(line.contains("waiting_user=false"));
}

#[test]
fn cmd_sessions_capacity_unknown_errors() {
    let db = fresh_db();
    let err = cmd_sessions_capacity(&db, "nope").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn cmd_sessions_handoff_persists_snapshot() {
    let db = fresh_db();
    seed_session(&db, "sess_h");
    let line = cmd_sessions_handoff(&db, "sess_h").unwrap();
    assert!(line.starts_with("hand_"));
    assert!(line.contains("source=sess_h"));
    // Pending list now has it.
    let pending = cmd_handoff_list(&db, 10).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].contains("decision=pending"));
}

#[test]
fn cmd_handoff_accept_round_trip() {
    let db = fresh_db();
    seed_session(&db, "sess_a");
    let summary = cmd_sessions_handoff(&db, "sess_a").unwrap();
    let id = summary.split_whitespace().next().unwrap().to_string();
    let out = cmd_handoff_accept(&db, &id, Some("继续会话")).unwrap();
    assert!(out.contains("accepted"));
    assert!(out.contains("new_session=sess_"));
    // Pending list emptied.
    let pending = cmd_handoff_list(&db, 10).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn cmd_handoff_decline_round_trip() {
    let db = fresh_db();
    seed_session(&db, "sess_d");
    let summary = cmd_sessions_handoff(&db, "sess_d").unwrap();
    let id = summary.split_whitespace().next().unwrap().to_string();
    let out = cmd_handoff_decline(&db, &id).unwrap();
    assert!(out.contains("declined"));
    let pending = cmd_handoff_list(&db, 10).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn cmd_handoff_show_returns_pretty_json() {
    let db = fresh_db();
    seed_session(&db, "sess_s");
    let summary = cmd_sessions_handoff(&db, "sess_s").unwrap();
    let id = summary.split_whitespace().next().unwrap().to_string();
    let json = cmd_handoff_show(&db, &id).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["id"], id);
    assert_eq!(v["source_session_id"], "sess_s");
    assert_eq!(v["user_decision"], "pending");
    assert!(v["sections"]["suggested_first_message"].as_str().is_some());
}

#[test]
fn cmd_memory_list_json_emits_valid_array() {
    let db = fresh_db();
    cmd_memory_write(&db, "用户偏好 vim", "global").unwrap();
    cmd_memory_write(&db, "Mirage 项目用 Riverpod", "global").unwrap();
    let out = cmd_memory_list_json(&db, "global").unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    for item in arr {
        assert!(item["id"].as_str().unwrap().starts_with("mem_"));
        assert!(item["type"].as_str().is_some());
        assert!(item["trust_now"].as_f64().is_some());
        assert!(item["tier"].as_i64().is_some());
    }
}

#[test]
fn cmd_memory_search_json_includes_score_breakdown() {
    let db = fresh_db();
    cmd_memory_write(&db, "用户偏好 vim", "global").unwrap();
    let out = cmd_memory_search_json(&db, "global", "vim", 5).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["hybrid_score"].as_f64().is_some());
    assert!(arr[0]["fts"].as_f64().is_some());
    assert!(arr[0]["jaccard"].as_f64().is_some());
    assert!(arr[0]["vector"].as_f64().is_some());
}

#[test]
fn cmd_sessions_list_json_emits_required_fields() {
    let db = fresh_db();
    seed_session(&db, "sess_a");
    let out = cmd_sessions_list_json(&db).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let s = &arr[0];
    assert_eq!(s["id"], "sess_a");
    assert!(s["title"].as_str().is_some());
    assert!(s["domain"].as_str().is_some());
    assert!(s["status"].as_str().is_some());
    assert!(s["last_active_at"].as_str().is_some());
}

#[test]
fn cmd_raw_log_json_serialises_seq_and_timestamps() {
    let db = fresh_db();
    seed_session(&db, "sess_log");
    jarvis_db::raw_event_log::append(
        &db,
        jarvis_db::raw_event_log::AppendEvent {
            event_type: jarvis_db::RawEventKind::UserMessage,
            session_id: Some("sess_log"),
            trace_id: Some("trc"),
            agent_id: None,
            raw_content: "hello",
            safe_content: None,
        },
    )
    .unwrap();
    let out = cmd_raw_log_json(&db, "sess_log", 10).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["raw_content"], "hello");
    assert!(arr[0]["seq"].as_i64().is_some());
    assert!(arr[0]["ts"].as_str().is_some());
}

#[test]
fn cmd_audit_json_serialises_actor_and_status() {
    let db = fresh_db();
    jarvis_db::audit_log::append(
        &db,
        jarvis_db::audit_log::AppendAudit {
            trace_id: None,
            session_id: Some("sess_a"),
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
    let out = cmd_audit_json(&db, "sess_a", 10).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["actor"], "tool_runtime");
    assert_eq!(arr[0]["status"], "success");
}

#[test]
fn cmd_skills_list_json_emits_status_and_counts() {
    let db = fresh_db();
    let reg = jarvis_growth::SkillRegistry::new(db.clone());
    let mut skill = jarvis_growth::skill::new_skill("diagnose", "x");
    skill.success_count = 5;
    skill.failure_count = 0;
    skill.status = jarvis_growth::SkillStatus::Promoted;
    reg.upsert(&skill).unwrap();
    let out = cmd_skills_list_json(&db).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["status"], "promoted");
    assert_eq!(arr[0]["success_count"], 5);
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

// ── jarvis model login / logout ─────────────────────────────────────────

mod oauth_login {
    use super::*;
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;

    type SimpleHandler = Arc<
        dyn Fn(
                String,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Response<Full<Bytes>>> + Send>>
            + Send
            + Sync,
    >;

    async fn spawn_idp(handler: SimpleHandler) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let io = TokioIo::new(stream);
                let h = handler.clone();
                tokio::spawn(async move {
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let h = h.clone();
                        async move {
                            Ok::<_, Infallible>(h(req.uri().path().to_string()).await)
                        }
                    });
                    let _ = http1::Builder::new().serve_connection(io, svc).await;
                });
            }
        });
        addr
    }

    fn json_resp(status: u16, body: &str) -> Response<Full<Bytes>> {
        Response::builder()
            .status(StatusCode::from_u16(status).unwrap())
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body.to_string())))
            .unwrap()
    }

    #[tokio::test]
    async fn login_completes_device_flow_and_writes_token_file() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_h = counter.clone();
        let handler: SimpleHandler = Arc::new(move |path| {
            let counter = counter_h.clone();
            Box::pin(async move {
                if path == "/device/code" {
                    json_resp(
                        200,
                        r#"{"device_code":"DC","user_code":"USER-123","verification_uri":"https://idp.example/device","verification_uri_complete":"https://idp.example/device?code=USER-123","expires_in":600,"interval":1}"#,
                    )
                } else if path == "/token" {
                    let n = counter.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        json_resp(400, r#"{"error":"authorization_pending"}"#)
                    } else {
                        json_resp(
                            200,
                            r#"{"access_token":"AT","refresh_token":"RT","token_type":"Bearer","expires_in":3600}"#,
                        )
                    }
                } else {
                    json_resp(404, r#"{"error":"not_found"}"#)
                }
            })
        });
        let addr = spawn_idp(handler).await;

        let mut cfg = jarvis_llm::LlmConfig::default();
        cfg.providers.insert(
            "demo".into(),
            jarvis_llm::ProviderConfig {
                oauth: Some(jarvis_llm::OAuthProviderConfig {
                    device_authorization_endpoint: format!(
                        "http://{addr}/device/code"
                    ),
                    token_endpoint: format!("http://{addr}/token"),
                    client_id: "test-client".into(),
                    client_secret: None,
                    scope: Some("read".into()),
                    audience: None,
                    user_agent: Some("jarvis-cli-test".into()),
                }),
                ..Default::default()
            },
        );

        let dir = tempfile::tempdir().unwrap();
        let store = jarvis_auth::TokenStore::new(dir.path());

        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let lines_w = lines.clone();
        cmd_model_login_with("demo", &cfg, &store, move |s: &str| {
            lines_w.lock().unwrap().push(s.to_string());
        })
        .await
        .expect("login ok");

        let printed = lines.lock().unwrap().join("\n");
        assert!(
            printed.contains("https://idp.example/device?code=USER-123"),
            "verification URL missing: {printed}"
        );
        assert!(printed.contains("USER-123"), "user code missing: {printed}");
        assert!(printed.contains("Approved"), "no approval line: {printed}");

        let saved = store.load("demo").unwrap().expect("token stored");
        assert_eq!(saved.access_token, "AT");
        assert_eq!(saved.refresh_token.as_deref(), Some("RT"));

        // Logout removes it.
        let out = cmd_model_logout_with("demo", &store).unwrap();
        assert!(out.contains("logged out"), "got: {out}");
        assert!(store.load("demo").unwrap().is_none());
        // Idempotent on second call.
        let out2 = cmd_model_logout_with("demo", &store).unwrap();
        assert!(out2.contains("nothing to do"), "got: {out2}");
    }

    #[tokio::test]
    async fn login_surfaces_access_denied() {
        let handler: SimpleHandler = Arc::new(|path| {
            Box::pin(async move {
                if path == "/device/code" {
                    json_resp(
                        200,
                        r#"{"device_code":"DC","user_code":"X","verification_uri":"https://idp.example/device","expires_in":600,"interval":1}"#,
                    )
                } else {
                    json_resp(400, r#"{"error":"access_denied"}"#)
                }
            })
        });
        let addr = spawn_idp(handler).await;

        let mut cfg = jarvis_llm::LlmConfig::default();
        cfg.providers.insert(
            "demo".into(),
            jarvis_llm::ProviderConfig {
                oauth: Some(jarvis_llm::OAuthProviderConfig {
                    device_authorization_endpoint: format!(
                        "http://{addr}/device/code"
                    ),
                    token_endpoint: format!("http://{addr}/token"),
                    client_id: "c".into(),
                    client_secret: None,
                    scope: None,
                    audience: None,
                    user_agent: None,
                }),
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let store = jarvis_auth::TokenStore::new(dir.path());
        let err = cmd_model_login_with("demo", &cfg, &store, |_| {})
            .await
            .expect_err("should fail");
        let msg = format!("{err}");
        assert!(msg.contains("access_denied") || msg.contains("denied"));
    }

    #[tokio::test]
    async fn login_rejects_provider_without_oauth_block() {
        let cfg = jarvis_llm::LlmConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let store = jarvis_auth::TokenStore::new(dir.path());
        let err = cmd_model_login_with("missing", &cfg, &store, |_| {})
            .await
            .expect_err("should fail");
        let msg = format!("{err}");
        assert!(msg.contains("no [providers.missing.oauth] block"), "got: {msg}");
    }

    #[tokio::test]
    async fn login_rejects_empty_provider_name() {
        let cfg = jarvis_llm::LlmConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let store = jarvis_auth::TokenStore::new(dir.path());
        let err = cmd_model_login_with("", &cfg, &store, |_| {})
            .await
            .expect_err("should fail");
        assert!(matches!(err, CmdError::MissingArg(_)));
    }
}
