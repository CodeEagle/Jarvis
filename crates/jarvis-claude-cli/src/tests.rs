//! Unit tests use a fake `claude` shell script that emits a fixed
//! JSON envelope to stdout. Real-binary tests are intentionally not
//! included — gating on `claude login` having been run is too flaky
//! for CI; manual verification covers the integration path.

use super::*;
use std::os::unix::fs::PermissionsExt;

fn write_fake_claude(dir: &std::path::Path, stdout: &str, exit_code: i32) -> std::path::PathBuf {
    let path = dir.join("claude");
    // Echo `$stdout` and exit with `$exit_code`. Heredoc with single
    // quotes keeps the body verbatim.
    let escaped = stdout.replace("EOF_FAKE", "EOF__FAKE");
    let script = format!(
        "#!/usr/bin/env bash\ncat <<'EOF_FAKE'\n{escaped}\nEOF_FAKE\nexit {exit_code}\n",
    );
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn cfg_for(binary: std::path::PathBuf) -> ClaudeCliConfig {
    ClaudeCliConfig {
        binary,
        timeout: Duration::from_secs(15),
    }
}

fn req(model: &str, system: Option<&str>, user: &str) -> CompletionRequest {
    let mut messages = Vec::new();
    if let Some(s) = system {
        messages.push(ChatMessage::system(s));
    }
    messages.push(ChatMessage::user(user));
    CompletionRequest::new(model, messages)
}

#[tokio::test]
async fn parses_successful_response() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"{"type":"result","subtype":"success","is_error":false,"result":"hello-from-claude-cli","duration_ms":42,"session_id":"x","total_cost_usd":0.0001,"usage":{"input_tokens":10,"output_tokens":4}}"#;
    let bin = write_fake_claude(dir.path(), body, 0);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let resp = backend
        .chat(req("claude-cli/sonnet", Some("be terse"), "say it"))
        .await
        .expect("ok");
    assert_eq!(resp.text, "hello-from-claude-cli");
    assert_eq!(resp.model, "claude-cli/sonnet");
    let usage = resp.usage.unwrap();
    assert_eq!(usage.input_tokens, 10);
    assert_eq!(usage.output_tokens, 4);
}

#[tokio::test]
async fn surfaces_is_error_as_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"{"type":"result","subtype":"success","is_error":true,"result":"Authentication error","duration_ms":1,"session_id":"x","usage":{"input_tokens":0,"output_tokens":0}}"#;
    let bin = write_fake_claude(dir.path(), body, 0);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let err = backend
        .chat(req("claude-cli/sonnet", None, "x"))
        .await
        .expect_err("should fail");
    let msg = format!("{err}");
    assert!(msg.contains("Authentication error"), "got: {msg}");
    assert!(matches!(err, CompletionError::Upstream(_)));
}

#[tokio::test]
async fn nonzero_exit_with_unparseable_stdout_becomes_upstream_error() {
    let dir = tempfile::tempdir().unwrap();
    let bin = write_fake_claude(dir.path(), "boom", 17);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let err = backend
        .chat(req("claude-cli/sonnet", None, "x"))
        .await
        .expect_err("should fail");
    assert!(matches!(err, CompletionError::Upstream(_)), "got: {err:?}");
}

#[tokio::test]
async fn auth_error_envelope_with_nonzero_exit_keeps_the_message() {
    // Real-world claude shape: stdout has the structured envelope
    // with is_error=true *and* the process exits non-zero (when
    // stdout is piped). The wrapper must surface the JSON's `result`
    // text rather than a bare "exited 1" message.
    let dir = tempfile::tempdir().unwrap();
    let body = r#"{"type":"result","subtype":"success","is_error":true,"result":"Authentication error · This may be a temporary network issue, please try again","duration_ms":1,"session_id":"x","usage":{"input_tokens":0,"output_tokens":0}}"#;
    let bin = write_fake_claude(dir.path(), body, 1);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let err = backend
        .chat(req("claude-cli/sonnet", None, "x"))
        .await
        .expect_err("should fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("Authentication error"),
        "expected JSON `result` text in error; got: {msg}"
    );
    assert!(matches!(err, CompletionError::Upstream(_)));
}

#[tokio::test]
async fn missing_binary_becomes_backend_error() {
    let backend = ClaudeCliCompletion::new(cfg_for(
        std::path::PathBuf::from("/nonexistent/claude"),
    ));
    let err = backend
        .chat(req("claude-cli/sonnet", None, "x"))
        .await
        .expect_err("should fail");
    assert!(
        matches!(err, CompletionError::Backend(_)),
        "got: {err:?}"
    );
}

#[tokio::test]
async fn garbage_stdout_becomes_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let bin = write_fake_claude(dir.path(), "this is not json", 0);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let err = backend
        .chat(req("claude-cli/sonnet", None, "x"))
        .await
        .expect_err("should fail");
    assert!(matches!(err, CompletionError::Parse(_)), "got: {err:?}");
}

#[tokio::test]
async fn rejects_request_with_no_user_message() {
    let dir = tempfile::tempdir().unwrap();
    let bin = write_fake_claude(dir.path(), "{}", 0);
    let backend = ClaudeCliCompletion::new(cfg_for(bin));
    let req = CompletionRequest::new(
        "claude-cli/sonnet",
        vec![ChatMessage::system("only system, no user")],
    );
    let err = backend.chat(req).await.expect_err("should fail");
    assert!(matches!(err, CompletionError::Parse(_)), "got: {err:?}");
}

#[test]
fn build_prompt_concatenates_systems() {
    let msgs = vec![
        ChatMessage::system("be brief"),
        ChatMessage::system("answer in english"),
        ChatMessage::user("hi"),
    ];
    let (system, user) = build_prompt_parts(&msgs).unwrap();
    assert_eq!(system.unwrap(), "be brief\n\nanswer in english");
    assert_eq!(user, "hi");
}

#[test]
fn build_prompt_folds_prior_turns_into_user_text() {
    let msgs = vec![
        ChatMessage::system("be brief"),
        ChatMessage::user("hello"),
        ChatMessage::assistant("hi there"),
        ChatMessage::user("how are you"),
    ];
    let (_system, user) = build_prompt_parts(&msgs).unwrap();
    assert!(
        user.contains("User: hello"),
        "missing earlier user turn: {user}"
    );
    assert!(
        user.contains("Assistant: hi there"),
        "missing assistant turn: {user}"
    );
    assert!(user.ends_with("User: how are you"), "got: {user}");
}
