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

/// Real end-to-end smoke test against the locally-installed codex CLI.
/// Requires `codex login` to have completed and the binary on PATH.
/// Run with: `cargo test -p jarvis-codex --release real_codex_smoke -- --ignored --nocapture`.
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
