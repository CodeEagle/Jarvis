//! OpenAI Chat Completions adapter for `jarvis_router::LlmJudge`.
//!
//! Mirror of `jarvis-anthropic` against OpenAI's `/v1/chat/completions`
//! endpoint. Uses JSON-mode (`response_format: { "type": "json_object" }`)
//! so the model's output round-trips back into `JudgeOutcome` cleanly.
//! Returns `None` on any HTTP / parse / timeout failure (Section 5.5).

use std::time::Duration;

use async_trait::async_trait;
use jarvis_router::llm_judge::{JudgeInputs, JudgeOutcome, LlmJudge};
use serde::{Deserialize, Serialize};
use tracing::warn;

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    pub max_tokens: u32,
}

impl OpenAiConfig {
    pub fn from_env() -> Result<Self, OpenAiError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| OpenAiError::MissingApiKey)?;
        Ok(Self {
            api_key,
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
            model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            timeout: Duration::from_secs(15),
            max_tokens: 1024,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenAiError {
    #[error("missing OPENAI_API_KEY")]
    MissingApiKey,
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct OpenAiJudge {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiJudge {
    pub fn new(config: OpenAiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("reqwest client");
        Self { client, config }
    }
}

#[async_trait]
impl LlmJudge for OpenAiJudge {
    async fn judge(&self, inputs: JudgeInputs<'_>) -> Option<JudgeOutcome> {
        let request = build_request(&self.config, &inputs);
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        let response = match self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(target: "openai", "send error: {e}");
                return None;
            }
        };
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(target: "openai", "non-success {status}: {body}");
            return None;
        }
        let payload: ChatResponse = match response.json().await {
            Ok(p) => p,
            Err(e) => {
                warn!(target: "openai", "json parse error: {e}");
                return None;
            }
        };
        let text = payload
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        serde_json::from_str::<JudgeOutcome>(&text)
            .map_err(|e| {
                warn!(target: "openai", "judge JSON parse error: {e}; body: {text}");
                e
            })
            .ok()
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Msg>,
    max_tokens: u32,
    response_format: ResponseFormat,
}

#[derive(Debug, Serialize)]
struct Msg {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

fn build_request(cfg: &OpenAiConfig, inputs: &JudgeInputs<'_>) -> ChatRequest {
    let agents = inputs
        .allowed_agents
        .iter()
        .map(|a| format!("- {a}"))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!(
        "You are Jarvis Main Router. Output JSON only with keys: \
primary_intent, secondary_intents, domain, topic, session_action, \
agent_type, confidence (<1.0), clarification_needed, router_notes.\n\
Allowed agents:\n{agents}"
    );
    let hints = serde_json::to_string(inputs.rule_hints).unwrap_or_else(|_| "[]".into());
    let recent = serde_json::to_string(inputs.recent_session_titles)
        .unwrap_or_else(|_| "[]".into());
    let user = format!(
        "user_input: {}\nrule_hints: {hints}\nrecent_titles: {recent}\n\
trace_id: {}\ntask_id: {}",
        inputs.user_input, inputs.trace_id, inputs.task_id
    );
    ChatRequest {
        model: cfg.model.clone(),
        messages: vec![
            Msg {
                role: "system",
                content: system,
            },
            Msg {
                role: "user",
                content: user,
            },
        ],
        max_tokens: cfg.max_tokens,
        response_format: ResponseFormat {
            kind: "json_object",
        },
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMsg,
}

#[derive(Debug, Deserialize)]
struct ChoiceMsg {
    content: String,
}

#[cfg(test)]
mod tests;
