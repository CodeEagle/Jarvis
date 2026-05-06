use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Source module of a growth event. Section 16.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceModule {
    Router,
    Session,
    Agent,
    Tool,
    Memory,
    Runtime,
    Compressor,
    Ui,
}

impl SourceModule {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceModule::Router => "router",
            SourceModule::Session => "session",
            SourceModule::Agent => "agent",
            SourceModule::Tool => "tool",
            SourceModule::Memory => "memory",
            SourceModule::Runtime => "runtime",
            SourceModule::Compressor => "compressor",
            SourceModule::Ui => "ui",
        }
    }
}

/// Section 16.3 event_type set. Free-form string so callers can extend
/// it without bumping this enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthEvent {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub trace_id: Option<String>,
    pub source_module: SourceModule,
    pub event_type: String,
    pub payload_json: String,
}
