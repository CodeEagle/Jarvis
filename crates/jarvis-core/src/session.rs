use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Section 6.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub domain: String,
    pub topic: String,

    pub summary: String,
    pub long_summary: String,

    #[serde(default)]
    pub active_entities: Vec<String>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub resolved: Vec<String>,

    #[serde(default)]
    pub recent_message_ids: Vec<String>,
    #[serde(default)]
    pub memory_refs: Vec<String>,
    #[serde(default)]
    pub skill_refs: Vec<String>,

    pub status: SessionStatus,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Paused,
    Archived,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Paused => "paused",
            SessionStatus::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub role: MessageRole,
    pub content: String,
    pub token_count: u32,
    pub summary_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Per-turn compressed summary. Section 14.2 / 9.5.4 Layer 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSummary {
    pub id: String,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub task_id: Option<String>,

    pub user_goal: String,
    pub actions_taken: Vec<String>,
    pub result: String,
    pub entities: Vec<String>,
    pub unresolved: Vec<String>,

    pub source_message_ids: Vec<String>,
    pub token_count: u32,

    pub created_at: DateTime<Utc>,
}
