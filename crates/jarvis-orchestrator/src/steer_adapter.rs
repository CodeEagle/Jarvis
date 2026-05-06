//! Steer adapter trait + Codex-shaped stub (Section 9.11.5).
//!
//! Concrete adapters bridge a SteerSignal into the underlying agent
//! transport. For the Codex driver that's an HTTP POST to its `/steer`
//! endpoint with `mode=append` (override is forbidden — the spec is
//! explicit). Other transports plug in by implementing the trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::steer::{InjectAt, SteerScope, SteerSignal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterMode {
    Append,
    /// Override is *never* sent over the wire by Jarvis — keeping the
    /// variant lets adapters acknowledge the mode space, but the
    /// builder helper below refuses to construct one.
    Override,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPayload {
    pub message: String,
    pub mode: AdapterMode,
    pub scope: String,
    pub inject_at: String,
    pub source_signal_id: String,
}

impl AdapterPayload {
    pub fn from_signal(signal: &SteerSignal) -> Self {
        Self {
            message: signal.content.clone(),
            mode: AdapterMode::Append,
            scope: signal.scope.as_str().to_string(),
            inject_at: signal.inject_at.as_str().to_string(),
            source_signal_id: signal.id.clone(),
        }
    }
}

#[async_trait]
pub trait SteerAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    /// Send the payload to the underlying transport. Returning Err is
    /// recoverable — SteerController logs and retries. Returning Ok
    /// means the underlying agent will inject at its next checkpoint.
    async fn send(&self, payload: AdapterPayload) -> Result<(), AdapterError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("rejected by adapter: {0}")]
    Rejected(String),
}

/// Test-only adapter that records every payload it would have sent.
/// The mock makes assertion in tests trivial without standing up an
/// HTTP listener.
pub struct RecordingAdapter {
    name: &'static str,
    inbox: tokio::sync::Mutex<Vec<AdapterPayload>>,
}

impl RecordingAdapter {
    pub fn new() -> Self {
        Self {
            name: "recording",
            inbox: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    pub async fn drain(&self) -> Vec<AdapterPayload> {
        std::mem::take(&mut *self.inbox.lock().await)
    }
}

impl Default for RecordingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SteerAdapter for RecordingAdapter {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn send(&self, payload: AdapterPayload) -> Result<(), AdapterError> {
        self.inbox.lock().await.push(payload);
        Ok(())
    }
}

/// Convenience: validate that a steer is admissible before sending.
/// Section 9.11.4 forbids steers that would override CONTEXT.md or
/// modify tool_scope. We can't see the proposed message body's intent
/// here, so we catch the few protocol-level constraints we can:
/// SteerScope::Constraint must not target tool_scope keys, content must
/// be non-empty, and the inject_at value must be one of the canonical
/// three.
pub fn admissibility_check(signal: &SteerSignal) -> Result<(), String> {
    if signal.content.trim().is_empty() {
        return Err("steer content is empty".into());
    }
    let lower = signal.content.to_lowercase();
    if lower.contains("override context.md")
        || lower.contains("覆盖 context.md")
        || lower.contains("覆盖context.md")
    {
        return Err("attempt to override CONTEXT.md is forbidden".into());
    }
    if matches!(signal.scope, SteerScope::Constraint)
        && (lower.contains("tool_scope") || lower.contains("修改 tool_scope"))
    {
        return Err("steer cannot modify tool_scope".into());
    }
    matches!(
        signal.inject_at,
        InjectAt::NextStep | InjectAt::NextToolCall | InjectAt::NextCheckpoint
    )
    .then_some(())
    .ok_or_else(|| "invalid inject_at".into())
}
