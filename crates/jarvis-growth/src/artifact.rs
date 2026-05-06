use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    RoutingExample,
    IntentCandidate,
    SkillCandidate,
    ToolPolicy,
    AgentPolicy,
    SummaryTemplate,
    MemoryCandidate,
    SessionResolverRule,
    ModelPolicy,
}

impl ArtifactType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactType::RoutingExample => "routing_example",
            ArtifactType::IntentCandidate => "intent_candidate",
            ArtifactType::SkillCandidate => "skill_candidate",
            ArtifactType::ToolPolicy => "tool_policy",
            ArtifactType::AgentPolicy => "agent_policy",
            ArtifactType::SummaryTemplate => "summary_template",
            ArtifactType::MemoryCandidate => "memory_candidate",
            ArtifactType::SessionResolverRule => "session_resolver_rule",
            ArtifactType::ModelPolicy => "model_policy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "routing_example" => ArtifactType::RoutingExample,
            "intent_candidate" => ArtifactType::IntentCandidate,
            "skill_candidate" => ArtifactType::SkillCandidate,
            "tool_policy" => ArtifactType::ToolPolicy,
            "agent_policy" => ArtifactType::AgentPolicy,
            "summary_template" => ArtifactType::SummaryTemplate,
            "memory_candidate" => ArtifactType::MemoryCandidate,
            "session_resolver_rule" => ArtifactType::SessionResolverRule,
            "model_policy" => ArtifactType::ModelPolicy,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Candidate,
    Testing,
    Promoted,
    Rejected,
    Deprecated,
}

impl ArtifactStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactStatus::Candidate => "candidate",
            ArtifactStatus::Testing => "testing",
            ArtifactStatus::Promoted => "promoted",
            ArtifactStatus::Rejected => "rejected",
            ArtifactStatus::Deprecated => "deprecated",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthArtifact {
    pub id: String,
    pub r#type: ArtifactType,
    pub status: ArtifactStatus,
    pub version: u32,
    pub scope: Option<String>,
    pub confidence: f32,
    pub evidence_trace_ids: Vec<String>,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
    pub promoted_at: Option<DateTime<Utc>>,
}
