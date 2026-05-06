use serde::{Deserialize, Serialize};

/// Mention parse mode. Section 5.3a.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MentionMode {
    None,
    Single,
    Multi,
    Steer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMention {
    pub raw_text: String,
    pub resolved_type: Option<String>,
    pub unresolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionParseResult {
    pub raw_input: String,
    pub clean_input: String,
    pub mentions: Vec<AgentMention>,
    pub mention_mode: MentionMode,
}
