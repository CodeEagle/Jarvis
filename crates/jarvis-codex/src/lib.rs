//! Codex CLI subprocess adapter for `jarvis_router::LlmJudge`.
//!
//! Shells out to a locally-installed `codex` binary
//! (https://www.npmjs.com/package/@openai/codex) and uses
//! `codex exec --output-schema` to constrain the model to a
//! `JudgeOutcome`-shaped JSON object. Authentication is whatever the
//! codex CLI has on disk under `~/.codex/auth.json` — typically a
//! ChatGPT plan login from `codex login --device-auth`, so this
//! adapter does NOT need an `OPENAI_API_KEY`.
//!
//! Following Section 5.5 the adapter returns `None` on any failure
//! (binary missing, non-zero exit, timeout, schema parse error) so the
//! Router falls back to rule-only routing.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use jarvis_router::llm_judge::{JudgeInputs, JudgeOutcome, LlmJudge};
use tokio::process::Command;
use tracing::{debug, warn};

/// JSON Schema mirroring `JudgeOutcome`. Passed to codex via
/// `--output-schema` so the model emits a parseable object.
const JUDGE_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["primary_intent","secondary_intents","domain","topic","session_action","agent_type","confidence","clarification_needed","router_notes"],
  "properties": {
    "primary_intent":      {"type":"string"},
    "secondary_intents":   {"type":"array","items":{"type":"string"}},
    "domain":              {"type":"string"},
    "topic":               {"type":"string"},
    "session_action":      {"type":"string","enum":["continue_existing","create_new","merge","temporary"]},
    "agent_type":          {"type":"string"},
    "confidence":          {"type":"number","minimum":0,"maximum":0.99},
    "clarification_needed":{"type":"boolean"},
    "router_notes":        {"type":"string"}
  }
}"#;

#[derive(Debug, Clone)]
pub struct CodexConfig {
    /// Path or name of the codex binary; defaults to "codex" on $PATH.
    pub binary: PathBuf,
    /// Optional model override (`codex exec -m <model>`).
    pub model: Option<String>,
    /// Hard wall-clock deadline for one judge call.
    pub timeout: Duration,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("codex"),
            model: None,
            timeout: Duration::from_secs(180),
        }
    }
}

impl CodexConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(b) = std::env::var("CODEX_BINARY") {
            cfg.binary = PathBuf::from(b);
        }
        if let Ok(m) = std::env::var("CODEX_MODEL") {
            cfg.model = Some(m);
        }
        if let Ok(t) = std::env::var("CODEX_TIMEOUT_SECS") {
            if let Ok(secs) = t.parse::<u64>() {
                cfg.timeout = Duration::from_secs(secs);
            }
        }
        cfg
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodexError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("codex exec timed out after {0:?}")]
    Timeout(Duration),
    #[error("codex exec non-zero exit ({code:?}); stderr: {stderr}")]
    NonZero { code: Option<i32>, stderr: String },
    #[error("judge JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

pub struct CodexJudge {
    config: CodexConfig,
}

impl CodexJudge {
    pub fn new(config: CodexConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl LlmJudge for CodexJudge {
    async fn judge(&self, inputs: JudgeInputs<'_>) -> Option<JudgeOutcome> {
        match self.run(&inputs).await {
            Ok(o) => Some(o),
            Err(e) => {
                warn!(target: "codex", "judge failed: {e}");
                None
            }
        }
    }
}

impl CodexJudge {
    async fn run(&self, inputs: &JudgeInputs<'_>) -> Result<JudgeOutcome, CodexError> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = std::env::temp_dir().join(format!("jarvis-codex-{id}"));
        tokio::fs::create_dir_all(&dir).await?;
        let schema_path = dir.join("schema.json");
        let last_path = dir.join("last.json");
        tokio::fs::write(&schema_path, JUDGE_SCHEMA).await?;

        let prompt = build_prompt(inputs);
        let mut cmd = Command::new(&self.config.binary);
        cmd.arg("exec")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--ephemeral")
            .arg("--color")
            .arg("never")
            .arg("--output-schema")
            .arg(&schema_path)
            .arg("-o")
            .arg(&last_path)
            .arg("-C")
            .arg(&dir);
        if let Some(model) = &self.config.model {
            cmd.arg("-m").arg(model);
        }
        cmd.arg(&prompt);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Without this, a timeout-aborted future leaves the codex
            // child process running until it finishes on its own.
            .kill_on_drop(true);

        let output = match tokio::time::timeout(self.config.timeout, cmd.output()).await {
            Ok(r) => r?,
            Err(_) => {
                let _ = tokio::fs::remove_dir_all(&dir).await;
                return Err(CodexError::Timeout(self.config.timeout));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(CodexError::NonZero {
                code: output.status.code(),
                stderr,
            });
        }

        let body = tokio::fs::read_to_string(&last_path).await?;
        debug!(target: "codex", "raw judge body: {body}");
        let trimmed = body.trim();
        let outcome = serde_json::from_str::<JudgeOutcome>(trimmed)?;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        Ok(outcome)
    }
}

fn build_prompt(inputs: &JudgeInputs<'_>) -> String {
    let agents = if inputs.allowed_agents.is_empty() {
        "(any)".to_string()
    } else {
        inputs
            .allowed_agents
            .iter()
            .map(|a| format!("- {a}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let hints =
        serde_json::to_string(inputs.rule_hints).unwrap_or_else(|_| "[]".into());
    let recent_titles =
        serde_json::to_string(inputs.recent_session_titles).unwrap_or_else(|_| "[]".into());
    format!(
        r#"你是 Jarvis 主路由器（Main Router）。
唯一职责：分析用户输入，输出一个完整的 RouteDecision JSON。
不要回答问题、不要执行任务，只做调度决策。
所有字段必须存在；confidence 严格 < 1.0。

可用 agent：
{agents}

## 用户输入
{user}

## 规则层提示（仅供参考）
{hints}

## 最近活跃 session 标题
{recent_titles}

## trace 信息（原样回填）
trace_id: {trace}
task_id:  {task}
"#,
        user = inputs.user_input,
        trace = inputs.trace_id,
        task = inputs.task_id,
    )
}

#[cfg(test)]
mod tests;
