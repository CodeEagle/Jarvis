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
