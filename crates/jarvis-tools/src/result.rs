use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Success,
    PermissionDenied,
    BlockedByScope,
    AwaitingConfirmation,
    Timeout,
    Unavailable,
    Error,
}

impl ToolStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolStatus::Success => "success",
            ToolStatus::PermissionDenied => "permission_denied",
            ToolStatus::BlockedByScope => "blocked_by_scope",
            ToolStatus::AwaitingConfirmation => "awaiting_confirmation",
            ToolStatus::Timeout => "timeout",
            ToolStatus::Unavailable => "unavailable",
            ToolStatus::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: String,
    pub status: ToolStatus,
    /// Truncated, redaction-ready textual summary of the output.
    pub output_summary: String,
    /// Full output payload (JSON serialised).
    #[serde(default)]
    pub payload_json: Option<String>,
    /// Filled when status indicates failure.
    #[serde(default)]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(tool: &str, summary: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            tool: tool.to_string(),
            status: ToolStatus::Success,
            output_summary: summary.into(),
            payload_json: serde_json::to_string(&payload).ok(),
            error: None,
        }
    }

    pub fn deny(tool: &str, status: ToolStatus, why: impl Into<String>) -> Self {
        let why = why.into();
        Self {
            tool: tool.to_string(),
            status,
            output_summary: format!("[{}] {}", status.as_str(), why),
            payload_json: None,
            error: Some(why),
        }
    }
}
