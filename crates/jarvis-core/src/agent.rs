use serde::{Deserialize, Serialize};

use crate::tool::ToolScope;

/// Runtime mode for an agent worker. Section 9.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    InProcess,
    WorkerProcess,
    AppServer,
}

/// Agent definition. Section 8.12.1.
///
/// Mirrors the TS `AgentDefinition` shape; `progress_templates` is kept
/// as a free-form map so we can iterate template strings without breaking
/// the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    // System identifiers
    pub r#type: String,
    pub version: String,

    // User-visible
    pub display_name: String,
    pub avatar_emoji: String,
    pub tagline: String,

    // @mention support (Section 5.3a)
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub mention_aliases: Vec<String>,

    // Routing
    #[serde(default)]
    pub trigger_domains: Vec<String>,
    #[serde(default)]
    pub trigger_intents: Vec<String>,
    #[serde(default)]
    pub priority: i32,

    // Runtime
    pub preferred_runtime_mode: RuntimeMode,
    #[serde(default)]
    pub can_be_sub_agent: bool,
    #[serde(default)]
    pub can_be_orchestrator: bool,

    // Capabilities
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,

    // Default tool scope
    pub default_tool_scope: ToolScope,

    // Context hints
    #[serde(default)]
    pub preferred_model: Option<String>,
    #[serde(default)]
    pub token_budget_hint: u32,
    #[serde(default)]
    pub system_prompt_template: String,
}

impl AgentDefinition {
    /// True when this agent is selectable by user `@mention`. Internal agents
    /// (Jarvis / Verifier / Walkthrough / Memory) return false.
    pub fn is_mentionable(&self) -> bool {
        self.mentionable
    }

    pub fn matches_mention(&self, name: &str) -> bool {
        if !self.mentionable {
            return false;
        }
        if self.display_name == name || self.r#type == name {
            return true;
        }
        self.mention_aliases.iter().any(|a| a == name)
    }
}
