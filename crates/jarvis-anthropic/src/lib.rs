//! Anthropic Messages API adapter for `jarvis_router::LlmJudge`.
//!
//! Hits Anthropic's `/v1/messages` endpoint and asks Claude to emit a
//! `JudgeOutcome` JSON object. Implements **prompt caching** for the
//! stable system block per Section 13.5 — the system prompt is split
//! into a long stable prefix (`cache_control: ephemeral`) plus the
//! per-turn user content, so subsequent calls inside the cache TTL
//! pay only for the dynamic part.
//!
//! The adapter is intentionally network-only — it returns `None` from
//! `judge` whenever anything goes wrong (timeout / non-2xx / parse
//! error). Section 5.5: Router treats `None` as the fallback signal.

use std::time::Duration;

use async_trait::async_trait;
use jarvis_router::llm_judge::{JudgeInputs, JudgeOutcome, LlmJudge};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    pub max_tokens: u32,
    /// When true, marks the system block as `cache_control: ephemeral`.
    pub prompt_caching: bool,
}

impl AnthropicConfig {
    pub fn from_env() -> Result<Self, AnthropicError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| AnthropicError::MissingApiKey)?;
        Ok(Self {
            api_key,
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
            model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            timeout: Duration::from_secs(15),
            max_tokens: 1024,
            prompt_caching: true,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnthropicError {
    #[error("missing ANTHROPIC_API_KEY")]
    MissingApiKey,
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct AnthropicJudge {
    client: reqwest::Client,
    config: AnthropicConfig,
}

impl AnthropicJudge {
    pub fn new(config: AnthropicConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("reqwest client");
        Self { client, config }
    }
}

#[async_trait]
impl LlmJudge for AnthropicJudge {
    async fn judge(&self, inputs: JudgeInputs<'_>) -> Option<JudgeOutcome> {
        let request = build_request(&self.config, &inputs);
        let url = format!("{}/v1/messages", self.config.base_url);
        let response = match self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(target: "anthropic", "send error: {e}");
                return None;
            }
        };
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(target: "anthropic", "non-success {status}: {body}");
            return None;
        }
        let payload: MessagesResponse = match response.json().await {
            Ok(p) => p,
            Err(e) => {
                warn!(target: "anthropic", "json parse error: {e}");
                return None;
            }
        };
        let text = payload
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        debug!(target: "anthropic", "raw judge text: {text}");
        parse_judge_outcome(&text)
    }
}

// ── request shape ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    system: Vec<SystemBlock>,
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct SystemBlock {
    #[serde(rename = "type")]
    kind: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Debug, Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Serialize)]
struct Message {
    role: String,
    content: String,
}

fn build_request(cfg: &AnthropicConfig, inputs: &JudgeInputs<'_>) -> MessagesRequest {
    let stable = stable_system_text(inputs);
    let system_block = SystemBlock {
        kind: "text".into(),
        text: stable,
        cache_control: cfg
            .prompt_caching
            .then(|| CacheControl {
                kind: "ephemeral".into(),
            }),
    };
    let user_text = user_turn_text(inputs);
    MessagesRequest {
        model: cfg.model.clone(),
        max_tokens: cfg.max_tokens,
        system: vec![system_block],
        messages: vec![Message {
            role: "user".into(),
            content: user_text,
        }],
    }
}

fn stable_system_text(inputs: &JudgeInputs<'_>) -> String {
    // Section 5.4 Router system prompt structure.
    let agents = inputs
        .allowed_agents
        .iter()
        .map(|a| format!("- {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"你是 Jarvis 的主路由器（Main Router）。

## 你的唯一职责
分析用户输入，结合上下文信息，输出一个完整的 RouteDecision JSON。
你不回答用户的问题，不执行任何任务，只做调度决策。

## 输出格式
只输出 JSON，不输出任何其他内容，不加 markdown 代码块标记。
所有字段必须存在，不能省略。confidence 不能填 1.0。

## 输出 schema
{{
  "primary_intent": "<L1.L2 格式，如 coding.debug>",
  "secondary_intents": ["..."],
  "domain": "<L1>",
  "topic": "<≤20 字主题>",
  "session_action": "continue_existing | create_new | merge | temporary",
  "agent_type": "<one of allowed agents>",
  "confidence": <0.0~0.99>,
  "clarification_needed": <true|false>,
  "router_notes": "<1-3 句路由理由>"
}}

## 可用 Agent
{agents}
"#,
    )
}

fn user_turn_text(inputs: &JudgeInputs<'_>) -> String {
    let hints = if inputs.rule_hints.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(inputs.rule_hints).unwrap_or_else(|_| "[]".into())
    };
    let recent_titles = serde_json::to_string(inputs.recent_session_titles)
        .unwrap_or_else(|_| "[]".into());
    format!(
        r#"## 用户输入
{}

## 规则层提示（仅供参考）
{hints}

## 最近活跃 Session 标题
{recent_titles}

## trace 信息（必须原样回填）
trace_id: {}
task_id:  {}
"#,
        inputs.user_input, inputs.trace_id, inputs.task_id
    )
}

// ── response shape ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

fn parse_judge_outcome(text: &str) -> Option<JudgeOutcome> {
    // Trim any code-fence the model may have wrapped around the JSON.
    let trimmed = text.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim())
        .map(|s| s.strip_suffix("```").unwrap_or(s).trim())
        .unwrap_or(trimmed);
    serde_json::from_str::<JudgeOutcome>(body)
        .map_err(|e| {
            warn!(target: "anthropic", "judge JSON parse error: {e}; body: {body}");
            e
        })
        .ok()
}

#[cfg(test)]
mod tests;
