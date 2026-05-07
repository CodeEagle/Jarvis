//! Subprocess wrapper around Anthropic's `claude` CLI.
//!
//! The `claude` CLI ships with its own OAuth flow (`claude login`,
//! token stored in `~/.claude/`) and supports a non-interactive
//! `--print` mode that emits a structured JSON envelope. This crate
//! implements `Completion` by shelling out to that mode, so a user
//! who's already authenticated their Claude Pro/Team subscription
//! through the official CLI can drive Jarvis without minting a
//! separate API key.
//!
//! Mirrors the pattern used by `jarvis-codex` for the routing judge.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use jarvis_llm::{
    ChatMessage, ChatRole, Completion, CompletionError, CompletionRequest,
    CompletionResponse, ModelId, Usage,
};
use serde::Deserialize;
use tokio::process::Command;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct ClaudeCliConfig {
    /// Path or name of the `claude` binary; defaults to "claude" on PATH.
    pub binary: PathBuf,
    /// Hard wall-clock deadline for one completion call.
    pub timeout: Duration,
}

impl Default for ClaudeCliConfig {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("claude"),
            timeout: Duration::from_secs(180),
        }
    }
}

impl ClaudeCliConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(b) = std::env::var("CLAUDE_BINARY") {
            cfg.binary = PathBuf::from(b);
        }
        if let Ok(t) = std::env::var("CLAUDE_TIMEOUT_SECS") {
            if let Ok(secs) = t.parse::<u64>() {
                cfg.timeout = Duration::from_secs(secs);
            }
        }
        cfg
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeCliError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("non-zero exit ({code:?}); stderr: {stderr}")]
    NonZero { code: Option<i32>, stderr: String },
    #[error("parse: {0}")]
    Parse(String),
    #[error("claude reported error: {0}")]
    ClaudeError(String),
    #[error("no user message in request")]
    NoUserMessage,
}

impl From<ClaudeCliError> for CompletionError {
    fn from(e: ClaudeCliError) -> Self {
        match e {
            ClaudeCliError::Timeout(_) => CompletionError::Timeout,
            ClaudeCliError::Io(io) => CompletionError::Backend(io.to_string()),
            ClaudeCliError::NonZero { code, stderr } => {
                let stderr = stderr.trim();
                let msg = if stderr.is_empty() {
                    format!("claude exited {code:?}")
                } else {
                    format!("claude exited {code:?}: {stderr}")
                };
                CompletionError::Upstream(msg)
            }
            ClaudeCliError::Parse(s) => CompletionError::Parse(s),
            ClaudeCliError::ClaudeError(s) => CompletionError::Upstream(s),
            ClaudeCliError::NoUserMessage => {
                CompletionError::Parse("no user message in request".into())
            }
        }
    }
}

pub struct ClaudeCliCompletion {
    config: ClaudeCliConfig,
}

impl ClaudeCliCompletion {
    pub fn new(config: ClaudeCliConfig) -> Self {
        Self { config }
    }
}

impl Default for ClaudeCliCompletion {
    fn default() -> Self {
        Self::new(ClaudeCliConfig::default())
    }
}

#[async_trait]
impl Completion for ClaudeCliCompletion {
    async fn chat(
        &self,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, CompletionError> {
        let model_id = ModelId::parse(&req.model)
            .map_err(|e| CompletionError::InvalidModelId(e.to_string()))?;
        let (system, user) = build_prompt_parts(&req.messages)?;

        let mut cmd = Command::new(&self.config.binary);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(&model_id.model)
            // Don't leave session traces on the user's machine — each
            // call is independent.
            .arg("--no-session-persistence")
            // We only want a chat completion; disable tool use so the
            // CLI doesn't try to edit files / run shells.
            .arg("--tools")
            .arg("");
        if let Some(s) = &system {
            cmd.arg("--system-prompt").arg(s);
        }
        cmd.arg(&user);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output =
            match tokio::time::timeout(self.config.timeout, cmd.output()).await {
                Ok(r) => r.map_err(ClaudeCliError::Io)?,
                Err(_) => return Err(ClaudeCliError::Timeout(self.config.timeout).into()),
            };

        // CRITICAL: parse stdout BEFORE checking the exit code. Claude
        // returns the structured error envelope on stdout (with
        // `is_error: true` + a descriptive `result` string) even when
        // the process exits non-zero — e.g. "Authentication error" on
        // a missing OAuth token. Bailing on exit-code first would
        // throw away the message the user actually needs to see.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!(target: "claude-cli", body = %stdout, "raw claude json");
        match serde_json::from_str::<ClaudeJsonOutput>(stdout.trim()) {
            Ok(parsed) if parsed.is_error => {
                warn!(target: "claude-cli", "claude is_error=true: {}", parsed.result);
                return Err(ClaudeCliError::ClaudeError(parsed.result).into());
            }
            Ok(parsed) => {
                let usage = Some(Usage {
                    input_tokens: parsed.usage.input_tokens,
                    output_tokens: parsed.usage.output_tokens,
                });
                return Ok(CompletionResponse {
                    model: req.model,
                    text: parsed.result,
                    usage,
                });
            }
            Err(e) => {
                // No structured envelope on stdout. Surface the most
                // useful diagnostic we have: non-zero exit + stderr if
                // the process failed; otherwise a parse error.
                if !output.status.success() {
                    return Err(ClaudeCliError::NonZero {
                        code: output.status.code(),
                        stderr: stderr.into_owned(),
                    }
                    .into());
                }
                return Err(ClaudeCliError::Parse(format!(
                    "json: {e}; body: {stdout}"
                ))
                .into());
            }
        }
    }
}

// ── prompt construction ────────────────────────────────────────────────

/// Reduce the message history to the (system, user-prompt) pair the
/// `claude -p` mode accepts. System messages concatenate; the user
/// prompt is the last user message — earlier turns are folded into the
/// prompt body in `Role: content` lines so single- and multi-turn
/// callers both work without `--input-format stream-json`.
fn build_prompt_parts(
    messages: &[ChatMessage],
) -> Result<(Option<String>, String), ClaudeCliError> {
    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == ChatRole::System)
        .map(|m| m.content.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    let last_user_idx = messages
        .iter()
        .rposition(|m| m.role == ChatRole::User)
        .ok_or(ClaudeCliError::NoUserMessage)?;
    let prior: Vec<String> = messages
        .iter()
        .take(last_user_idx)
        .filter(|m| !matches!(m.role, ChatRole::System))
        .map(|m| match m.role {
            ChatRole::User => format!("User: {}", m.content),
            ChatRole::Assistant => format!("Assistant: {}", m.content),
            ChatRole::System => String::new(),
        })
        .collect();
    let user_msg = messages[last_user_idx].content.clone();
    let user = if prior.is_empty() {
        user_msg
    } else {
        format!("{}\n\nUser: {}", prior.join("\n\n"), user_msg)
    };
    Ok((system, user))
}

// ── claude --output-format json envelope ───────────────────────────────

#[derive(Debug, Deserialize)]
struct ClaudeJsonOutput {
    #[serde(default)]
    is_error: bool,
    /// On success, the assistant's reply text. On failure, the error
    /// message claude wants to surface to the user.
    #[serde(default)]
    result: String,
    #[serde(default)]
    usage: ClaudeUsage,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[cfg(test)]
mod tests;
