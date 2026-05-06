//! SubTaskEnvelope and SubTaskResult shapes (Section 8.7).

use jarvis_core::tool::ToolScope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskEnvelope {
    pub sub_task_id: String,
    pub parent_task_id: String,
    pub trace_id: String,

    pub title: String,
    pub instruction: String,

    #[serde(default)]
    pub depends_on_results: Vec<SubTaskResultSummary>,
    #[serde(default)]
    pub input_artifact_refs: Vec<String>,

    pub tool_scope: ToolScope,
    pub output_spec: OutputSpec,

    pub constraints: SubTaskConstraints,

    /// Optional pointer to a `.tentacle/` directory that the sub-agent
    /// should read at startup. Section 8.4a.
    #[serde(default)]
    pub tentacle_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpec {
    pub format: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskConstraints {
    pub max_tool_calls: u32,
    pub max_file_reads: u32,
    pub token_budget: u32,
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskResultSummary {
    pub sub_task_id: String,
    pub summary: String,
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubTaskStatus {
    Success,
    Partial,
    Failed,
    NeedClarification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskResult {
    pub sub_task_id: String,
    pub status: SubTaskStatus,
    pub summary: String,
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub escalation: Option<Escalation>,
    pub token_used: u32,
    pub tool_calls_count: u32,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    pub reason: String,
    #[serde(default)]
    pub options: Vec<String>,
}
