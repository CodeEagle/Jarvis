#[cfg(test)]
use crate::*;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use jarvis_core::ids::new_id_with_prefix;
#[cfg(test)]
use jarvis_core::task::{TaskNode, TaskStatus};
#[cfg(test)]
use jarvis_db::Db;

// ── TaskTree ────────────────────────────────────────────────────────────

#[cfg(test)]
fn build_node(
    id: &str,
    title: &str,
    status: TaskStatus,
    depends: &[&str],
) -> TaskNode {
    TaskNode {
        id: id.into(),
        parent_id: None,
        task_id: format!("task_{id}"),
        title: title.into(),
        intent: "coding.generate".into(),
        status,
        assigned_agent_type: "coding".into(),
        assigned_agent_id: None,
        depends_on: depends.iter().map(|s| s.to_string()).collect(),
        result_summary: None,
        result_artifact_ids: vec![],
        error_summary: None,
        created_at: Utc::now(),
        completed_at: None,
    }
}

#[test]
fn task_tree_round_trip() {
    let db = Db::in_memory().unwrap();
    let store = TaskTreeStore::new(db);
    let tree = store
        .create_tree("task_root", "sess_1", Some("trc_1"))
        .unwrap();
    store
        .add_node(&tree.id, &build_node("n1", "step 1", TaskStatus::Pending, &[]))
        .unwrap();
    store
        .add_node(
            &tree.id,
            &build_node("n2", "step 2", TaskStatus::Pending, &["n1"]),
        )
        .unwrap();
    let nodes = store.list_nodes(&tree.id).unwrap();
    assert_eq!(nodes.len(), 2);
}

#[test]
fn build_view_separates_status_buckets() {
    let db = Db::in_memory().unwrap();
    let store = TaskTreeStore::new(db);
    let tree = store
        .create_tree("task_root", "sess_1", None)
        .unwrap();
    store
        .add_node(&tree.id, &build_node("done", "x", TaskStatus::Success, &[]))
        .unwrap();
    store
        .add_node(&tree.id, &build_node("running", "y", TaskStatus::Running, &[]))
        .unwrap();
    store
        .add_node(&tree.id, &build_node("waiting", "z", TaskStatus::Pending, &[]))
        .unwrap();
    let view = store.build_view(&tree.id).unwrap();
    assert_eq!(view.completed_count, 1);
    assert_eq!(view.running_count, 1);
    assert_eq!(view.pending_count, 1);
    // active_nodes contains running+pending+(last completed for context).
    assert!(view.active_nodes.iter().any(|n| n.title == "y"));
}

#[test]
fn update_status_records_completion_time() {
    let db = Db::in_memory().unwrap();
    let store = TaskTreeStore::new(db);
    let tree = store.create_tree("task_root", "sess_1", None).unwrap();
    store
        .add_node(&tree.id, &build_node("n1", "step", TaskStatus::Running, &[]))
        .unwrap();
    store
        .update_status("n1", TaskStatus::Success, Some("ok"), None)
        .unwrap();
    let n = store.get_node("n1").unwrap().unwrap();
    assert_eq!(n.status, TaskStatus::Success);
    assert_eq!(n.result_summary.as_deref(), Some("ok"));
    assert!(n.completed_at.is_some());
}

// ── ArtifactRegistry ────────────────────────────────────────────────────

#[test]
fn artifact_register_and_index() {
    let db = Db::in_memory().unwrap();
    let reg = ArtifactRegistry::new(db);
    let a = reg
        .create(
            "sess_1",
            ArtifactKind::Note,
            "design notes",
            "summary",
            "/tmp/x.md",
            None,
        )
        .unwrap();
    let idx = reg.build_index("sess_1").unwrap();
    assert_eq!(idx.len(), 1);
    assert_eq!(idx[0].id, a.id);
    assert_eq!(idx[0].title, "design notes");
}

#[test]
fn artifact_get_returns_full_record() {
    let db = Db::in_memory().unwrap();
    let reg = ArtifactRegistry::new(db);
    let a = reg
        .create(
            "sess_1",
            ArtifactKind::CodePatch,
            "patch",
            "diff summary",
            "patch://abc",
            Some("node_1"),
        )
        .unwrap();
    let back = reg.get(&a.id).unwrap().unwrap();
    assert_eq!(back.kind, ArtifactKind::CodePatch);
    assert_eq!(back.task_node_id.as_deref(), Some("node_1"));
}

// ── ConversationBus ────────────────────────────────────────────────────

#[test]
fn ownership_acquire_releases_previous() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    let first = bus
        .acquire_ownership("sess_1", "agent_a", "orchestrator", None, InteractionMode::Listening)
        .unwrap();
    let second = bus
        .acquire_ownership(
            "sess_1",
            "agent_b",
            "coding",
            Some("st_1"),
            InteractionMode::WaitingUser,
        )
        .unwrap();
    let cur = bus.current_ownership("sess_1").unwrap().unwrap();
    assert_eq!(cur.id, second.id);
    assert_eq!(cur.interaction_mode, InteractionMode::WaitingUser);
    assert_ne!(first.id, second.id);
}

#[test]
fn release_ownership_clears_current() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    let r = bus
        .acquire_ownership("sess_1", "a", "coding", None, InteractionMode::Executing)
        .unwrap();
    bus.release_ownership(&r.id).unwrap();
    assert!(bus.current_ownership("sess_1").unwrap().is_none());
}

#[test]
fn classify_interrupt_message() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    assert_eq!(
        bus.classify_user_message("停一下"),
        UserMessageRoute::Interrupt
    );
    assert_eq!(
        bus.classify_user_message("cancel that"),
        UserMessageRoute::Interrupt
    );
}

#[test]
fn classify_progress_query() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    assert_eq!(
        bus.classify_user_message("做到哪了"),
        UserMessageRoute::ProgressQuery
    );
}

#[test]
fn classify_steer_message() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    assert_eq!(
        bus.classify_user_message("记得保持 stream 接口不变"),
        UserMessageRoute::Steer
    );
}

#[test]
fn classify_normal_message() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    assert_eq!(
        bus.classify_user_message("这是新的需求"),
        UserMessageRoute::Normal
    );
}

#[test]
fn open_and_list_sub_channel() {
    let db = Db::in_memory().unwrap();
    let bus = ConversationBus::new(db);
    let ch = bus
        .open_sub_channel("sess_1", "st_1", "coding", Some("/tmp/.tentacle"))
        .unwrap();
    let active = bus.list_active_sub_channels("sess_1").unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].sub_task_id, "st_1");
    bus.update_sub_channel_status(&ch.id, SubChannelStatus::Done)
        .unwrap();
    assert!(bus.list_active_sub_channels("sess_1").unwrap().is_empty());
}

// ── Checkpoints ─────────────────────────────────────────────────────────

#[test]
fn checkpoint_save_and_latest() {
    let db = Db::in_memory().unwrap();
    let store = CheckpointStore::new(db);
    let completed = vec!["step1".to_string()];
    let pinned = vec!["found bug at line 42".to_string()];
    let arts: Vec<String> = vec!["artf_1".into()];
    store
        .save(CheckpointDraft {
            sub_task_id: "st_1",
            agent_id: Some("agent_a"),
            completed_steps: &completed,
            working_set_snapshot: "current snapshot",
            pinned_findings: &pinned,
            artifact_ids_so_far: &arts,
            resume_hint: "resume from step2",
        })
        .unwrap();
    let latest = store.latest("st_1").unwrap().unwrap();
    assert_eq!(latest.resume_hint, "resume from step2");
    assert_eq!(latest.pinned_findings.len(), 1);
}

// ── Steer (Section 30.5) ────────────────────────────────────────────────

#[test]
fn steer_first_three_accepted_then_throttled() {
    let db = Db::in_memory().unwrap();
    let ctrl = SteerController::new(db);
    for i in 0..3 {
        let outcome = ctrl
            .enqueue(EnqueueRequest {
                session_id: "sess_1",
                sub_task_id: "st_1",
                trace_id: None,
                content: &format!("steer {i}"),
                scope: SteerScope::Constraint,
                inject_at: InjectAt::NextStep,
            })
            .unwrap();
        matches!(outcome, SteerEnqueueOutcome::Accepted(_))
            .then_some(())
            .expect("expected acceptance");
    }
    let fourth = ctrl
        .enqueue(EnqueueRequest {
            session_id: "sess_1",
            sub_task_id: "st_1",
            trace_id: None,
            content: "steer 4",
            scope: SteerScope::Constraint,
            inject_at: InjectAt::NextStep,
        })
        .unwrap();
    matches!(fourth, SteerEnqueueOutcome::RateLimited { .. })
        .then_some(())
        .expect("expected rate limit");
}

#[test]
fn steer_writes_to_raw_event_log() {
    let db = Db::in_memory().unwrap();
    let ctrl = SteerController::new(db.clone());
    ctrl.enqueue(EnqueueRequest {
        session_id: "sess_1",
        sub_task_id: "st_1",
        trace_id: Some("trc_x"),
        content: "保持 stream 接口",
        scope: SteerScope::Constraint,
        inject_at: InjectAt::NextStep,
    })
    .unwrap();
    let n = jarvis_db::raw_event_log::count(&db).unwrap();
    assert!(n >= 1, "expected steer to write to raw_event_log");
}

#[test]
fn steer_status_transitions_pending_injected_acknowledged() {
    let db = Db::in_memory().unwrap();
    let ctrl = SteerController::new(db);
    let outcome = ctrl
        .enqueue(EnqueueRequest {
            session_id: "sess_1",
            sub_task_id: "st_2",
            trace_id: None,
            content: "x",
            scope: SteerScope::Direction,
            inject_at: InjectAt::NextStep,
        })
        .unwrap();
    let id = match outcome {
        SteerEnqueueOutcome::Accepted(s) => s.id,
        _ => panic!("expected accepted"),
    };
    let pending = ctrl.next_pending("st_2", InjectAt::NextStep).unwrap();
    assert_eq!(pending.unwrap().id, id);

    ctrl.mark_injected(&id).unwrap();
    let still_pending = ctrl.next_pending("st_2", InjectAt::NextStep).unwrap();
    assert!(still_pending.is_none(), "should no longer be pending");

    ctrl.mark_acknowledged(&id).unwrap();
}

// ── Tentacle (Section 10.4 / 30.12) ─────────────────────────────────────

#[test]
fn tentacle_generator_creates_files() {
    let dir = tempfile::tempdir().unwrap();
    let spec = TentacleSpec {
        task_title: "重构 sync 模块".into(),
        scope_summary: "拆分 SyncManager 职责".into(),
        key_files: vec!["lib/sync/sync_manager.dart".into()],
        constraints: vec!["保持 stream 接口".into()],
        do_not_break: vec!["sync_repository.dart 第 45 行".into()],
        todo_steps: vec![
            "分析依赖".into(),
            "创建 sync_state.dart".into(),
            "重构 sync_executor.dart".into(),
        ],
    };
    let t = TentacleGenerator::ensure(dir.path(), &spec).unwrap();
    let ctx = t.read_context().unwrap();
    assert!(ctx.contains("重构 sync 模块"));
    assert!(ctx.contains("sync_repository.dart 第 45 行"));
    let todo = t.read_todo().unwrap();
    assert!(todo.contains("- [ ] 分析依赖"));
}

#[test]
fn tentacle_tick_marks_step_done() {
    let dir = tempfile::tempdir().unwrap();
    let spec = TentacleSpec {
        task_title: "x".into(),
        scope_summary: "y".into(),
        key_files: vec![],
        constraints: vec![],
        do_not_break: vec![],
        todo_steps: vec!["分析依赖".into(), "创建文件".into()],
    };
    let t = TentacleGenerator::ensure(dir.path(), &spec).unwrap();
    t.tick("分析依赖").unwrap();
    let todo = t.read_todo().unwrap();
    assert!(todo.contains("- [x] 分析依赖"));
    assert!(todo.contains("- [ ] 创建文件"));
}

#[test]
fn tentacle_handoff_is_one_shot() {
    let dir = tempfile::tempdir().unwrap();
    let spec = TentacleSpec {
        task_title: "x".into(),
        scope_summary: "y".into(),
        key_files: vec![],
        constraints: vec![],
        do_not_break: vec![],
        todo_steps: vec!["s".into()],
    };
    let t = TentacleGenerator::ensure(dir.path(), &spec).unwrap();
    t.write_handoff_once("first").unwrap();
    let err = t.write_handoff_once("second").unwrap_err();
    assert!(format!("{err}").contains("HANDOFF.md already written"));
}

#[test]
fn tentacle_context_is_write_protected_for_subagents() {
    let dir = tempfile::tempdir().unwrap();
    let spec = TentacleSpec {
        task_title: "x".into(),
        scope_summary: "y".into(),
        key_files: vec![],
        constraints: vec![],
        do_not_break: vec![],
        todo_steps: vec!["s".into()],
    };
    let t = TentacleGenerator::ensure(dir.path(), &spec).unwrap();
    let err = t
        .try_overwrite_context("malicious replacement")
        .unwrap_err();
    assert!(format!("{err}").contains("write-protected"));
}

#[test]
fn tentacle_notes_appends() {
    let dir = tempfile::tempdir().unwrap();
    let spec = TentacleSpec {
        task_title: "x".into(),
        scope_summary: "y".into(),
        key_files: vec![],
        constraints: vec![],
        do_not_break: vec![],
        todo_steps: vec!["s".into()],
    };
    let t = TentacleGenerator::ensure(dir.path(), &spec).unwrap();
    t.append_note("first finding").unwrap();
    t.append_note("second finding").unwrap();
    let notes = t.read_notes().unwrap();
    assert!(notes.contains("first finding"));
    assert!(notes.contains("second finding"));
}

// ── workspace lock (Section 30.12) ──────────────────────────────────────

#[test]
fn workspace_writer_lock_excludes_other_writers() {
    use std::path::Path;
    let path = Path::new("/tmp/jarvis_ws_test_a");
    let _writer =
        WorkspaceLocker::acquire(path, "task_a", workspace::WorkspaceMode::Writer)
            .expect("acquire writer");
    let err = WorkspaceLocker::acquire(path, "task_b", workspace::WorkspaceMode::Writer);
    assert!(err.is_err(), "second writer should not acquire");
}

#[test]
fn workspace_writer_blocks_readers() {
    use std::path::Path;
    let path = Path::new("/tmp/jarvis_ws_test_b");
    let _writer =
        WorkspaceLocker::acquire(path, "task_a", workspace::WorkspaceMode::Writer)
            .expect("acquire writer");
    let r = WorkspaceLocker::acquire(path, "task_r", workspace::WorkspaceMode::Reader);
    assert!(r.is_err());
}

#[test]
fn workspace_readers_can_share() {
    use std::path::Path;
    let path = Path::new("/tmp/jarvis_ws_test_c");
    let _r1 = WorkspaceLocker::acquire(path, "r1", workspace::WorkspaceMode::Reader)
        .expect("first reader");
    let _r2 = WorkspaceLocker::acquire(path, "r2", workspace::WorkspaceMode::Reader)
        .expect("second reader");
}

#[test]
fn workspace_lock_releases_on_drop() {
    use std::path::Path;
    let path = Path::new("/tmp/jarvis_ws_test_d");
    {
        let _w = WorkspaceLocker::acquire(path, "task_a", workspace::WorkspaceMode::Writer)
            .expect("acquire");
    }
    let _w2 = WorkspaceLocker::acquire(path, "task_b", workspace::WorkspaceMode::Writer)
        .expect("re-acquire after drop");
}

// ── worktree manager ────────────────────────────────────────────────────

#[test]
fn worktree_create_and_discard() {
    let dir = tempfile::tempdir().unwrap();
    let mgr = workspace::WorktreeManager::new(dir.path());
    let wt = mgr.create("task_xyz").unwrap();
    assert!(wt.root.exists());
    assert!(mgr.exists("task_xyz"));
    mgr.discard(&wt).unwrap();
    assert!(!mgr.exists("task_xyz"));
}

// ── walkthrough auto-approval (Section 30.3.1) ──────────────────────────

#[test]
fn walkthrough_auto_approves_low_risk_verified() {
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "重构 sync");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    let mut summary = walkthrough::new_section(
        walkthrough::SectionType::Summary,
        "summary",
        "做了什么",
    );
    summary.risk_level = Some(walkthrough::RiskLevel::None);
    let mut change = walkthrough::new_section(
        walkthrough::SectionType::Change,
        "change",
        "改动",
    );
    change.risk_level = Some(walkthrough::RiskLevel::Low);
    change.files_changed = 3;
    let mut tests = walkthrough::new_section(
        walkthrough::SectionType::TestResult,
        "tests",
        "12/12 通过",
    );
    tests.risk_level = Some(walkthrough::RiskLevel::Low);
    tests.has_test_failure = false;
    doc.sections = vec![summary, change, tests];
    assert_eq!(doc.auto_decide(), walkthrough::AutoApprovalDecision::AutoApprove);
}

#[test]
fn walkthrough_high_risk_requires_human() {
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    let mut sec = walkthrough::new_section(
        walkthrough::SectionType::Risk,
        "risk",
        "high risk",
    );
    sec.risk_level = Some(walkthrough::RiskLevel::High);
    doc.sections = vec![sec];
    matches!(
        doc.auto_decide(),
        walkthrough::AutoApprovalDecision::RequireHuman(_)
    )
    .then_some(())
    .expect("expected RequireHuman");
}

#[test]
fn walkthrough_disputed_auto_rejects() {
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    doc.verification_status = walkthrough::VerificationStatus::Disputed;
    matches!(
        doc.auto_decide(),
        walkthrough::AutoApprovalDecision::AutoReject(_)
    )
    .then_some(())
    .expect("expected AutoReject");
}

#[test]
fn walkthrough_test_failure_blocks_auto_approve() {
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    let mut tests = walkthrough::new_section(
        walkthrough::SectionType::TestResult,
        "tests",
        "11/12 passed",
    );
    tests.risk_level = Some(walkthrough::RiskLevel::Low);
    tests.has_test_failure = true;
    doc.sections = vec![tests];
    matches!(
        doc.auto_decide(),
        walkthrough::AutoApprovalDecision::RequireHuman(_)
    )
    .then_some(())
    .expect("expected human review on test failure");
}

#[test]
fn walkthrough_too_many_files_blocks_auto_approve() {
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    let mut change = walkthrough::new_section(
        walkthrough::SectionType::Change,
        "change",
        "huge",
    );
    change.risk_level = Some(walkthrough::RiskLevel::Low);
    change.files_changed = 6;
    doc.sections = vec![change];
    matches!(
        doc.auto_decide(),
        walkthrough::AutoApprovalDecision::RequireHuman(_)
    )
    .then_some(())
    .expect("expected human review on size cap");
}

#[test]
fn walkthrough_store_round_trip_and_auto_review() {
    let db = Db::in_memory().unwrap();
    let store = walkthrough::WalkthroughStore::new(db);
    let mut doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    doc.sections = vec![walkthrough::new_section(
        walkthrough::SectionType::Summary,
        "summary",
        "ok",
    )];
    store.save(&doc).unwrap();
    let decision = store.auto_review(&doc.id).unwrap();
    assert_eq!(decision, walkthrough::AutoApprovalDecision::AutoApprove);
    let back = store.get(&doc.id).unwrap().unwrap();
    assert_eq!(back.approval_status, walkthrough::ApprovalStatus::Approved);
    assert_eq!(back.approved_by.as_deref(), Some("auto"));
}

// ── verifier ────────────────────────────────────────────────────────────

#[test]
fn verifier_file_exists_passes_when_present() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.txt"), "hi").unwrap();
    let mut check = verifier::VerifierCheck::new(
        "doc_1",
        "sec_1",
        verifier::CheckType::FileExists,
        "expected hello.txt",
    );
    let ok = verifier::execute_check(
        &mut check,
        dir.path(),
        std::path::Path::new("hello.txt"),
    );
    assert!(ok);
    assert_eq!(check.r#match, Some(true));
}

#[test]
fn verifier_file_exists_fails_with_discrepancy() {
    let dir = tempfile::tempdir().unwrap();
    let mut check = verifier::VerifierCheck::new(
        "doc_1",
        "sec_1",
        verifier::CheckType::FileExists,
        "expected nope.txt",
    );
    let ok = verifier::execute_check(
        &mut check,
        dir.path(),
        std::path::Path::new("nope.txt"),
    );
    assert!(!ok);
    assert!(check.discrepancy.is_some());
}

#[test]
fn verifier_run_marks_disputed_when_files_missing() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    let verifier_store = verifier::VerifierStore::new(db);
    let doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    walkthrough_store.save(&doc).unwrap();

    let check = verifier::VerifierCheck::new(
        &doc.id,
        "sec_1",
        verifier::CheckType::FileExists,
        "missing",
    );
    let report = verifier::run(
        &walkthrough_store,
        &verifier_store,
        &doc.id,
        dir.path(),
        vec![(check, std::path::PathBuf::from("missing.txt"))],
    )
    .unwrap();
    assert!(!report.all_match);
    let back = walkthrough_store.get(&doc.id).unwrap().unwrap();
    assert_eq!(back.verification_status, walkthrough::VerificationStatus::Disputed);
    assert!(back.verification_notes.is_some());
}

#[test]
fn verifier_run_marks_verified_when_all_match() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "ok").unwrap();
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    let verifier_store = verifier::VerifierStore::new(db);
    let doc = walkthrough::new_draft("st_1", "sess_1", "coding", "x");
    walkthrough_store.save(&doc).unwrap();

    let check = verifier::VerifierCheck::new(
        &doc.id,
        "sec_1",
        verifier::CheckType::FileExists,
        "expect a.txt",
    );
    let report = verifier::run(
        &walkthrough_store,
        &verifier_store,
        &doc.id,
        dir.path(),
        vec![(check, std::path::PathBuf::from("a.txt"))],
    )
    .unwrap();
    assert!(report.all_match);
    let back = walkthrough_store.get(&doc.id).unwrap().unwrap();
    assert_eq!(back.verification_status, walkthrough::VerificationStatus::Verified);
}

// ── activity card ───────────────────────────────────────────────────────

#[test]
fn activity_card_lifecycle() {
    let db = Db::in_memory().unwrap();
    let store = activity_card::ActivityCardStore::new(db);
    let card = store
        .create(activity_card::CardDraft {
            session_id: "sess_1",
            sub_task_id: Some("st_1"),
            trace_id: None,
            agent_type: "coding",
            agent_display_name: "代码助手",
            agent_avatar_emoji: "💻",
            title: "重构 sync",
        })
        .unwrap();
    store
        .set_progress(&card.id, "读取 sync.dart", "已读 1/4")
        .unwrap();
    let mid = store.get(&card.id).unwrap().unwrap();
    assert_eq!(mid.status, activity_card::CardStatus::Running);
    assert_eq!(mid.current_action.as_deref(), Some("读取 sync.dart"));

    store
        .update_status(&card.id, activity_card::CardStatus::Success, Some("done"))
        .unwrap();
    let done = store.get(&card.id).unwrap().unwrap();
    assert_eq!(done.status, activity_card::CardStatus::Success);
    assert!(done.completed_at.is_some());
}

// ── regression orchestrator (Section 30.3.3) ────────────────────────────

#[cfg(test)]
fn make_approved_doc(
    walkthrough_store: &walkthrough::WalkthroughStore,
    title: &str,
) -> walkthrough::WalkthroughDoc {
    let mut doc = walkthrough::new_draft("st_x", "sess_1", "coding", title);
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    doc.approval_status = walkthrough::ApprovalStatus::Approved;
    walkthrough_store.save(&doc).unwrap();
    doc
}

#[test]
fn regression_pass_when_no_changes() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "ok").unwrap();
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    let doc = make_approved_doc(&walkthrough_store, "feature A");
    let orch = regression::RegressionOrchestrator::new(db);

    let plan = regression::RegressionPlan {
        doc_id: doc.id.clone(),
        feature_title: doc.title.clone(),
        checks: vec![regression::RegressionCheckPlan {
            check: verifier::VerifierCheck::new(
                &doc.id,
                "sec_1",
                verifier::CheckType::FileExists,
                "expect a.txt",
            ),
            target: std::path::PathBuf::from("a.txt"),
            touched_in_changeset: false,
        }],
    };
    let report = orch.run(dir.path(), Some("sess_1"), vec![plan]).unwrap();
    assert_eq!(report.total_checks, 1);
    assert_eq!(report.passed, 1);
    assert_eq!(report.potential_bugs, 0);
    assert_eq!(report.items[0].status, regression::ItemStatus::Pass);
}

#[test]
fn regression_classifies_touched_failure_as_expected_change() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    // file is missing — verification fails
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    let doc = make_approved_doc(&walkthrough_store, "feature A");
    let orch = regression::RegressionOrchestrator::new(db);

    let plan = regression::RegressionPlan {
        doc_id: doc.id.clone(),
        feature_title: doc.title.clone(),
        checks: vec![regression::RegressionCheckPlan {
            check: verifier::VerifierCheck::new(
                &doc.id,
                "sec_1",
                verifier::CheckType::FileExists,
                "expect missing.txt",
            ),
            target: std::path::PathBuf::from("missing.txt"),
            touched_in_changeset: true,
        }],
    };
    let report = orch.run(dir.path(), Some("sess_1"), vec![plan]).unwrap();
    assert_eq!(report.expected_changes, 1);
    assert_eq!(report.potential_bugs, 0);
    assert_eq!(
        report.items[0].status,
        regression::ItemStatus::ExpectedChange
    );
}

#[test]
fn regression_classifies_untouched_failure_as_potential_bug() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    let doc = make_approved_doc(&walkthrough_store, "feature A");
    let orch = regression::RegressionOrchestrator::new(db);

    let plan = regression::RegressionPlan {
        doc_id: doc.id.clone(),
        feature_title: doc.title.clone(),
        checks: vec![regression::RegressionCheckPlan {
            check: verifier::VerifierCheck::new(
                &doc.id,
                "sec_1",
                verifier::CheckType::FileExists,
                "expect missing.txt",
            ),
            target: std::path::PathBuf::from("missing.txt"),
            touched_in_changeset: false,
        }],
    };
    let report = orch.run(dir.path(), Some("sess_1"), vec![plan]).unwrap();
    assert_eq!(report.potential_bugs, 1);
    assert_eq!(
        report.items[0].status,
        regression::ItemStatus::PotentialBug
    );
}

#[test]
fn regression_skips_unapproved_walkthrough() {
    let db = Db::in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let walkthrough_store = walkthrough::WalkthroughStore::new(db.clone());
    // Doc not approved
    let mut doc = walkthrough::new_draft("st_y", "sess_1", "coding", "feature B");
    doc.verification_status = walkthrough::VerificationStatus::Verified;
    walkthrough_store.save(&doc).unwrap();
    let orch = regression::RegressionOrchestrator::new(db);

    let plan = regression::RegressionPlan {
        doc_id: doc.id.clone(),
        feature_title: doc.title.clone(),
        checks: vec![regression::RegressionCheckPlan {
            check: verifier::VerifierCheck::new(
                &doc.id,
                "sec_1",
                verifier::CheckType::FileExists,
                "expect anything.txt",
            ),
            target: std::path::PathBuf::from("anything.txt"),
            touched_in_changeset: false,
        }],
    };
    let report = orch.run(dir.path(), None, vec![plan]).unwrap();
    assert_eq!(report.total_checks, 0); // skipped
    assert_eq!(report.items.len(), 0);
}

// suppress unused import warning when sub_task module is not used in tests
#[test]
fn sub_task_envelope_serializes() {
    let env = SubTaskEnvelope {
        sub_task_id: new_id_with_prefix("st"),
        parent_task_id: "task_root".into(),
        trace_id: "trc_1".into(),
        title: "do something".into(),
        instruction: "instructions".into(),
        depends_on_results: vec![],
        input_artifact_refs: vec![],
        tool_scope: jarvis_core::tool::ToolScope::empty(),
        output_spec: OutputSpec {
            format: "text".into(),
            max_tokens: 500,
        },
        constraints: SubTaskConstraints {
            max_tool_calls: 10,
            max_file_reads: 10,
            token_budget: 4000,
            timeout_ms: 60_000,
        },
        tentacle_path: None,
    };
    let s = serde_json::to_string(&env).unwrap();
    let _back: SubTaskEnvelope = serde_json::from_str(&s).unwrap();
}
