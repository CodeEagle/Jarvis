use serde::{Deserialize, Serialize};

use crate::agent::RuntimeMode;
use crate::tool::ToolScope;

/// What the Router decided to do with this turn. Section 5.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub trace_id: String,
    pub task_id: String,

    pub primary_intent: String,
    #[serde(default)]
    pub secondary_intents: Vec<String>,
    pub domain: String,
    pub topic: String,

    pub session_action: SessionAction,
    pub target_session_id: Option<String>,

    pub agent_type: String,
    pub preferred_runtime_mode: RuntimeMode,

    pub confidence: f32,
    pub clarification_needed: bool,

    pub memory_read: bool,
    pub memory_write: bool,

    #[serde(default)]
    pub skill_candidates: Vec<String>,

    pub tool_scope: ToolScope,

    pub router_notes: String,

    // @mention overrides (Section 5.3a)
    #[serde(default)]
    pub mention_override: bool,
    #[serde(default)]
    pub forced_sub_agents: Vec<String>,
    #[serde(default)]
    pub override_action: Option<String>,
    #[serde(default)]
    pub steer_target_sub_task_id: Option<String>,
    #[serde(default)]
    pub steer_content: Option<String>,

    // Degradation flags (Section 5.5)
    #[serde(default)]
    pub fallback_used: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    ContinueExisting,
    CreateNew,
    Merge,
    Temporary,
}

impl SessionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionAction::ContinueExisting => "continue_existing",
            SessionAction::CreateNew => "create_new",
            SessionAction::Merge => "merge",
            SessionAction::Temporary => "temporary",
        }
    }
}
