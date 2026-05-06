use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::Message;
use crate::tool::ToolScope;

/// What Router hands to the Agent. Section 7.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEnvelope {
    pub trace_id: String,
    pub task_id: String,
    pub user_input: String,

    pub intent: String,
    pub domain: String,
    pub topic: String,

    pub session: TaskEnvelopeSession,

    #[serde(default)]
    pub memories: Vec<crate::memory::Memory>,
    #[serde(default)]
    pub skills: Vec<String>,

    pub tool_scope: ToolScope,

    pub constraints: TaskConstraints,

    pub router_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEnvelopeSession {
    pub id: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub recent_messages: Vec<Message>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub active_entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConstraints {
    pub language: String,
    pub verbosity: Verbosity,
    pub safety_level: SafetyLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Short,
    Normal,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyLevel {
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

/// Section 8.4 TaskNode — Orchestrator's task tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub task_id: String,

    pub title: String,
    pub intent: String,
    pub status: TaskStatus,

    pub assigned_agent_type: String,
    pub assigned_agent_id: Option<String>,

    pub depends_on: Vec<String>,

    pub result_summary: Option<String>,
    pub result_artifact_ids: Vec<String>,
    pub error_summary: Option<String>,

    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
