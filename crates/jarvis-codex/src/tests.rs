//! Unit tests use a fake `codex` shell script that emits a fixed JSON
//! payload into the file given via `-o` and exits cleanly. The
//! `real_codex_smoke` test (gated `#[ignore]`) actually invokes the
//! installed codex binary and is opt-in via `cargo test --ignored`.

use super::*;
use jarvis_router::llm_judge::{JudgeInputs, LlmJudge};
use std::os::unix::fs::PermissionsExt;

fn write_fake_codex(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("codex");
    // Write `$body` into the file passed as `-o <path>`.
    // Single-quoted heredoc keeps the body verbatim — no shell expansion.
    let escaped = body.replace("EOF_FAKE", "EOF__FAKE");
    let script = format!(
        "#!/usr/bin/env bash\nset -e\nout=\nprev=\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\nif [ -z \"$out\" ]; then echo 'fake codex: missing -o' >&2; exit 2; fi\ncat > \"$out\" <<'EOF_FAKE'\n{escaped}\nEOF_FAKE\necho 'fake codex: wrote '\"$out\"\n",
    );
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn write_failing_codex(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("codex");
    let script = "#!/usr/bin/env bash\necho 'simulated failure' >&2\nexit 17\n";
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn test_inputs<'a>(input: &'a str) -> JudgeInputs<'a> {
    JudgeInputs {
        user_input: input,
        trace_id: "trc",
        task_id: "task",
        rule_hints: &[],
        recent_session_titles: &[],
        allowed_agents: &[],
    }
}

fn cfg_for(binary: std::path::PathBuf) -> CodexConfig {
    CodexConfig {
        binary,
        model: None,
        timeout: Duration::from_secs(15),
    }
}

#[tokio::test]
async fn judge_parses_well_formed_response() {
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-codex-test-ok-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let body = r#"{"primary_intent":"coding.debug","secondary_intents":[],"domain":"coding","topic":"openwrt","session_action":"create_new","agent_type":"coding","confidence":0.9,"clarification_needed":false,"router_notes":"ok"}"#;
    let bin = write_fake_codex(&tmp, body);
    let judge = CodexJudge::new(cfg_for(bin));
    let outcome = judge.judge(test_inputs("openwrt")).await.expect("judge ok");
    assert_eq!(outcome.primary_intent, "coding.debug");
    assert_eq!(outcome.agent_type, "coding");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn judge_returns_none_on_nonzero_exit() {
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-codex-test-fail-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let bin = write_failing_codex(&tmp);
    let judge = CodexJudge::new(cfg_for(bin));
    let outcome = judge.judge(test_inputs("anything")).await;
    assert!(outcome.is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn judge_returns_none_on_garbage_output() {
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-codex-test-garbage-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let bin = write_fake_codex(&tmp, "not even json");
    let judge = CodexJudge::new(cfg_for(bin));
    let outcome = judge.judge(test_inputs("anything")).await;
    assert!(outcome.is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn judge_returns_none_on_missing_binary() {
    let bin = std::path::PathBuf::from("/nonexistent/path/to/codex");
    let judge = CodexJudge::new(cfg_for(bin));
    let outcome = judge.judge(test_inputs("anything")).await;
    assert!(outcome.is_none());
}

/// Slow fake codex: sleeps `secs` seconds, then writes `body` to `-o`.
/// PID is written to `<dir>/pid` before sleeping so callers can check
/// that the subprocess was actually reaped on cancel/timeout.
fn write_slow_codex(dir: &std::path::Path, secs: u32, body: &str) -> std::path::PathBuf {
    let path = dir.join("codex");
    let escaped = body.replace("EOF_FAKE", "EOF__FAKE");
    let script = format!(
        "#!/usr/bin/env bash\nset -e\necho $$ > {dir}/pid\nout=\nprev=\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then out=\"$a\"; fi\n  prev=\"$a\"\ndone\nsleep {secs}\ncat > \"$out\" <<'EOF_FAKE'\n{escaped}\nEOF_FAKE\n",
        dir = dir.display(),
        secs = secs,
        escaped = escaped,
    );
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn pid_alive(pid: i32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[tokio::test]
async fn judge_times_out_and_returns_none() {
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-codex-test-timeout-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let body = r#"{"primary_intent":"x","secondary_intents":[],"domain":"x","topic":"x","session_action":"create_new","agent_type":"general","confidence":0.5,"clarification_needed":false,"router_notes":"never reached"}"#;
    let bin = write_slow_codex(&tmp, 30, body);
    let mut cfg = cfg_for(bin);
    cfg.timeout = Duration::from_millis(300);
    let judge = CodexJudge::new(cfg);
    let outcome = judge.judge(test_inputs("x")).await;
    assert!(outcome.is_none(), "should time out");

    // Give the kernel a beat to reap the killed child.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pid_file = tmp.join("pid");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        !pid_alive(pid),
        "kill_on_drop should have reaped the codex child (pid {pid})"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn judge_handles_concurrent_calls() {
    let tmp = std::env::temp_dir().join(format!(
        "jarvis-codex-test-concurrent-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let body = r#"{"primary_intent":"chat","secondary_intents":[],"domain":"chat","topic":"x","session_action":"create_new","agent_type":"general","confidence":0.5,"clarification_needed":false,"router_notes":"ok"}"#;
    let bin = write_fake_codex(&tmp, body);
    let cfg = cfg_for(bin);
    let judge = std::sync::Arc::new(CodexJudge::new(cfg));

    let mut handles = vec![];
    for i in 0..6 {
        let j = judge.clone();
        handles.push(tokio::spawn(async move {
            let inputs = JudgeInputs {
                user_input: "concurrent",
                trace_id: "trc",
                task_id: "task",
                rule_hints: &[],
                recent_session_titles: &[],
                allowed_agents: &[],
            };
            (i, j.judge(inputs).await)
        }));
    }
    for h in handles {
        let (i, outcome) = h.await.unwrap();
        assert!(outcome.is_some(), "concurrent call {i} returned None");
        assert_eq!(outcome.unwrap().agent_type, "general");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Real end-to-end smoke test through the actual Router pipeline using
/// a live codex subprocess. Verifies that CodexJudge plugs into
/// `Router::route_with_judge` and produces a coherent decision.
/// Run with: `cargo test -p jarvis-codex real_codex_through_router -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn real_codex_through_router() {
    use jarvis_db::Db;
    use jarvis_router::router::{Router, RouterInput};

    let db = Db::in_memory().unwrap();
    let router = Router::new(db);
    let mut cfg = CodexConfig::default();
    cfg.timeout = Duration::from_secs(180);
    let judge = CodexJudge::new(cfg);

    let (decision, _diag) = router
        .route_with_judge(
            RouterInput {
                user_input: "openwrt 编译报错 no rule to make target",
                session_id_hint: None,
                running_agent_types: &[],
            },
            &judge,
        )
        .await
        .expect("route_with_judge");

    eprintln!("router+codex decision: {decision:#?}");
    assert!(!decision.fallback_used, "judge should have produced an outcome");
    assert!(!decision.agent_type.is_empty());
    assert!(decision.confidence < 1.0);
    assert!(decision.router_notes.contains("judge:"));
}

// ── Multi-scenario real-codex tests (all #[ignore]) ───────────────────
// These exercise the prompt design across input shapes that the rule
// layer cannot disambiguate on its own. Each call hits the live codex
// binary and uses ~13s of wall-clock time.

#[tokio::test]
#[ignore]
async fn real_codex_english_casual_input() {
    let allowed = vec!["coding".into(), "general".into(), "research".into()];
    let inputs = JudgeInputs {
        user_input: "what's a fun weekend project for a rainy afternoon?",
        trace_id: "trc-en",
        task_id: "t-en",
        rule_hints: &[],
        recent_session_titles: &[],
        allowed_agents: &allowed,
    };
    let judge = CodexJudge::new(CodexConfig::default());
    let outcome = judge.judge(inputs).await.expect("real codex None");
    eprintln!("english casual: {outcome:#?}");
    assert!(allowed.contains(&outcome.agent_type));
    assert!(outcome.confidence < 1.0);
}

#[tokio::test]
#[ignore]
async fn real_codex_continue_existing_path() {
    let allowed = vec!["coding".into(), "general".into(), "research".into()];
    let recent = vec![
        "OpenWrt build error troubleshooting".into(),
        "Research on memory CRDT designs".into(),
    ];
    let inputs = JudgeInputs {
        user_input: "刚才那个 openwrt 编译问题再帮我看看",
        trace_id: "trc-cont",
        task_id: "t-cont",
        rule_hints: &[],
        recent_session_titles: &recent,
        allowed_agents: &allowed,
    };
    let judge = CodexJudge::new(CodexConfig::default());
    let outcome = judge.judge(inputs).await.expect("real codex None");
    eprintln!("continue path: {outcome:#?}");
    assert!(allowed.contains(&outcome.agent_type));
    // The model is free to choose; we just verify the schema holds.
    assert!(outcome.confidence < 1.0);
}

#[tokio::test]
#[ignore]
async fn real_codex_respects_allowed_agents_constraint() {
    // Only `general` allowed — model must stay in the set even if
    // the input would otherwise route to coding.
    let allowed = vec!["general".into()];
    let inputs = JudgeInputs {
        user_input: "openwrt 编译出错了，no rule to make target",
        trace_id: "trc-restrict",
        task_id: "t-restrict",
        rule_hints: &[],
        recent_session_titles: &[],
        allowed_agents: &allowed,
    };
    let judge = CodexJudge::new(CodexConfig::default());
    let outcome = judge.judge(inputs).await.expect("real codex None");
    eprintln!("restricted-agent: {outcome:#?}");
    assert_eq!(
        outcome.agent_type, "general",
        "model must respect the allowed-agents constraint"
    );
}

#[tokio::test]
#[ignore]
async fn real_codex_long_input_with_rule_hints() {
    use jarvis_core::intent::IntentHint;
    let allowed = vec!["coding".into(), "general".into(), "research".into()];
    let hints = vec![
        IntentHint {
            hint: "research.web_search".into(),
            weight: 0.7,
            matched: "rule".into(),
        },
        IntentHint {
            hint: "coding.refactor".into(),
            weight: 0.3,
            matched: "rule".into(),
        },
    ];
    let long = "我在调研分布式系统的一致性算法，重点是 Raft 和 Paxos 的差异、\
                 工程实现里的性能权衡，以及 etcd / TiKV 等开源项目的取舍。\
                 想要一份覆盖论文索引、关键源码模块、和近五年改进方向的概览。"
        .repeat(3);
    let inputs = JudgeInputs {
        user_input: &long,
        trace_id: "trc-long",
        task_id: "t-long",
        rule_hints: &hints,
        recent_session_titles: &[],
        allowed_agents: &allowed,
    };
    let judge = CodexJudge::new(CodexConfig::default());
    let outcome = judge.judge(inputs).await.expect("real codex None");
    eprintln!("long input + hints: {outcome:#?}");
    assert!(allowed.contains(&outcome.agent_type));
    assert!(outcome.confidence < 1.0);
}

/// Real end-to-end smoke test against the locally-installed codex CLI.
/// Requires `codex login` to have completed and the binary on PATH.
/// Run with: `cargo test -p jarvis-codex real_codex_smoke -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn real_codex_smoke() {
    let allowed = vec!["coding".to_string(), "general".to_string(), "research".to_string()];
    let inputs = JudgeInputs {
        user_input: "openwrt 编译报错 no rule to make target",
        trace_id: "trc-real",
        task_id: "t-real",
        rule_hints: &[],
        recent_session_titles: &[],
        allowed_agents: &allowed,
    };
    let mut cfg = CodexConfig::default();
    cfg.timeout = Duration::from_secs(180);
    let judge = CodexJudge::new(cfg);
    let outcome = judge
        .judge(inputs)
        .await
        .expect("real codex returned None — check codex login / PATH");
    eprintln!("real codex outcome: {outcome:#?}");
    assert!(!outcome.primary_intent.is_empty());
    assert!(!outcome.agent_type.is_empty());
    assert!(outcome.confidence < 1.0);
    assert!(allowed.contains(&outcome.agent_type));
}
