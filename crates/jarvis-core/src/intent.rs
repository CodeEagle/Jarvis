use serde::{Deserialize, Serialize};

/// L1 domains. Section 5.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Chat,
    Coding,
    Devops,
    Research,
    Creative,
    PersonalAdmin,
    SystemControl,
    Memory,
    Project,
    Internal,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Chat => "chat",
            Domain::Coding => "coding",
            Domain::Devops => "devops",
            Domain::Research => "research",
            Domain::Creative => "creative",
            Domain::PersonalAdmin => "personal_admin",
            Domain::SystemControl => "system_control",
            Domain::Memory => "memory",
            Domain::Project => "project",
            Domain::Internal => "internal",
        }
    }

    pub fn parse_l1(intent: &str) -> Option<Self> {
        let head = intent.split('.').next()?;
        Some(match head {
            "chat" => Domain::Chat,
            "coding" => Domain::Coding,
            "devops" => Domain::Devops,
            "research" => Domain::Research,
            "creative" => Domain::Creative,
            "personal_admin" => Domain::PersonalAdmin,
            "system_control" => Domain::SystemControl,
            "memory" => Domain::Memory,
            "project" => Domain::Project,
            "internal" => Domain::Internal,
            _ => return None,
        })
    }
}

/// Hint emitted by rule-layer keyword matching. The LLM judgment layer
/// treats these as advisory weights, not hard decisions. (Section 5.4)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentHint {
    pub hint: String,
    pub weight: f32,
    pub matched: String,
}
