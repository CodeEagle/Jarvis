use serde::{Deserialize, Serialize};

/// Tool authorization scope dispensed by Router for a single TaskEnvelope.
/// Section 11.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolScope {
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub blocked_tools: Vec<String>,
    #[serde(default)]
    pub requires_confirmation: Vec<String>,
    pub max_tool_calls: u32,
    pub max_parallel_tools: u32,
}

impl ToolScope {
    pub fn empty() -> Self {
        Self {
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            requires_confirmation: Vec::new(),
            max_tool_calls: 0,
            max_parallel_tools: 0,
        }
    }

    pub fn is_allowed(&self, tool: &str) -> bool {
        if self.blocked_tools.iter().any(|t| t == tool) {
            return false;
        }
        self.allowed_tools.iter().any(|t| t == tool)
    }

    pub fn requires_confirmation(&self, tool: &str) -> bool {
        self.requires_confirmation.iter().any(|t| t == tool)
    }
}
