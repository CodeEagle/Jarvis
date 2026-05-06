//! LLM judgment layer (Section 5.4).
//!
//! Two responsibilities:
//!
//! 1. Define an `LlmJudge` trait so the Router doesn't depend on any
//!    specific LLM client. Concrete adapters (Anthropic, OpenAI, local
//!    models) live in dedicated crates.
//! 2. Ship a `RuleBasedJudge` adapter that returns a `JudgeOutcome`
//!    derived purely from the rule layer — useful as a fallback when
//!    a real adapter times out (Section 5.5) and as a default for
//!    tests where determinism matters more than nuance.

use async_trait::async_trait;
use jarvis_core::intent::IntentHint;
use jarvis_core::route::SessionAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct JudgeInputs<'a> {
    pub user_input: &'a str,
    pub trace_id: &'a str,
    pub task_id: &'a str,
    pub rule_hints: &'a [IntentHint],
    pub recent_session_titles: &'a [String],
    pub allowed_agents: &'a [String],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeOutcome {
    pub primary_intent: String,
    #[serde(default)]
    pub secondary_intents: Vec<String>,
    pub domain: String,
    pub topic: String,
    pub session_action: SessionAction,
    pub agent_type: String,
    pub confidence: f32,
    pub clarification_needed: bool,
    pub router_notes: String,
}

#[async_trait]
pub trait LlmJudge: Send + Sync {
    /// Returns `None` when the adapter is unavailable (network error,
    /// timeout, …). Router treats `None` as the fallback signal and
    /// continues with rule-only routing.
    async fn judge(&self, inputs: JudgeInputs<'_>) -> Option<JudgeOutcome>;
}

/// Fallback judge that synthesises a `JudgeOutcome` from rule hints.
/// Always available, never errors, and never hits the network.
#[derive(Debug, Clone, Default)]
pub struct RuleBasedJudge;

#[async_trait]
impl LlmJudge for RuleBasedJudge {
    async fn judge(&self, inputs: JudgeInputs<'_>) -> Option<JudgeOutcome> {
        let dominant = inputs
            .rule_hints
            .iter()
            .max_by(|a, b| {
                a.weight
                    .partial_cmp(&b.weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned();
        let (primary_intent, weight) = match dominant {
            Some(h) => (h.hint, h.weight),
            None => ("chat".to_string(), 0.0),
        };
        let domain = primary_intent
            .split('.')
            .next()
            .unwrap_or("chat")
            .to_string();
        let topic: String = inputs.user_input.chars().take(20).collect();
        let session_action = if inputs.recent_session_titles.is_empty() {
            SessionAction::CreateNew
        } else {
            SessionAction::ContinueExisting
        };
        let agent_type = pick_agent(&domain, inputs.allowed_agents);
        let confidence = (0.45 + weight * 0.4).min(0.85);
        Some(JudgeOutcome {
            primary_intent,
            secondary_intents: vec![],
            domain,
            topic,
            session_action,
            agent_type,
            confidence,
            clarification_needed: weight < 0.20,
            router_notes: format!(
                "rule-based judge with dominant weight {:.2}",
                weight
            ),
        })
    }
}

fn pick_agent(domain: &str, allowed: &[String]) -> String {
    let preferred = match domain {
        "coding" => "coding",
        "devops" => "devops",
        "research" => "research",
        "creative" => "creative",
        "memory" => "memory",
        "project" => "project_manager",
        _ => "general",
    };
    if allowed.iter().any(|a| a == preferred) {
        preferred.to_string()
    } else if allowed.contains(&"general".to_string()) {
        "general".to_string()
    } else {
        allowed
            .first()
            .cloned()
            .unwrap_or_else(|| "general".to_string())
    }
}
