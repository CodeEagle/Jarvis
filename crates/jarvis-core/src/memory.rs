use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Memory types. Section 12.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    PreferenceMemory,
    ProjectMemory,
    EpisodeMemory,
    FactMemory,
    EntityMemory,
    RelationMemory,
    LessonMemory,
    ScratchMemory,
    InferenceMemory,
    ClusterMemory,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::PreferenceMemory => "preference_memory",
            MemoryType::ProjectMemory => "project_memory",
            MemoryType::EpisodeMemory => "episode_memory",
            MemoryType::FactMemory => "fact_memory",
            MemoryType::EntityMemory => "entity_memory",
            MemoryType::RelationMemory => "relation_memory",
            MemoryType::LessonMemory => "lesson_memory",
            MemoryType::ScratchMemory => "scratch_memory",
            MemoryType::InferenceMemory => "inference_memory",
            MemoryType::ClusterMemory => "cluster_memory",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "preference_memory" => MemoryType::PreferenceMemory,
            "project_memory" => MemoryType::ProjectMemory,
            "episode_memory" => MemoryType::EpisodeMemory,
            "fact_memory" => MemoryType::FactMemory,
            "entity_memory" => MemoryType::EntityMemory,
            "relation_memory" => MemoryType::RelationMemory,
            "lesson_memory" => MemoryType::LessonMemory,
            "scratch_memory" => MemoryType::ScratchMemory,
            "inference_memory" => MemoryType::InferenceMemory,
            "cluster_memory" => MemoryType::ClusterMemory,
            _ => return None,
        })
    }

    /// Half-life in days. Section 12.5.
    pub fn half_life_days(&self) -> u32 {
        match self {
            MemoryType::LessonMemory => 90,
            MemoryType::PreferenceMemory => 60,
            MemoryType::ProjectMemory => 60,
            MemoryType::FactMemory => 30,
            MemoryType::EntityMemory => 30,
            MemoryType::RelationMemory => 30,
            MemoryType::EpisodeMemory => 14,
            MemoryType::InferenceMemory => 30,
            MemoryType::ClusterMemory => 30,
            // scratch never decays — it's culled directly
            MemoryType::ScratchMemory => 0,
        }
    }

    /// Whether emotion coordinates apply. Section 12.3a.
    /// fact / preference / entity / relation / scratch are forced neutral.
    pub fn supports_emotion(&self) -> bool {
        matches!(
            self,
            MemoryType::EpisodeMemory
                | MemoryType::LessonMemory
                | MemoryType::ClusterMemory
                | MemoryType::InferenceMemory
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    UserExplicit,
    Inferred,
    Correction,
    TaskResult,
    DreamCluster,
    DreamInference,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::UserExplicit => "user_explicit",
            SourceType::Inferred => "inferred",
            SourceType::Correction => "correction",
            SourceType::TaskResult => "task_result",
            SourceType::DreamCluster => "dream_cluster",
            SourceType::DreamInference => "dream_inference",
        }
    }

    /// Default initial confidence by source. Section 12.3.
    pub fn default_confidence(&self) -> f32 {
        match self {
            SourceType::UserExplicit => 0.95,
            SourceType::Correction => 0.90,
            SourceType::TaskResult => 0.70,
            SourceType::Inferred => 0.50,
            SourceType::DreamCluster => 0.65,
            // Inference is intentionally below 1.0 even when confirmed (12.6).
            SourceType::DreamInference => 0.55,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Approved,
    Candidate,
    Deprecated,
    Conflict,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Approved => "approved",
            MemoryStatus::Candidate => "candidate",
            MemoryStatus::Deprecated => "deprecated",
            MemoryStatus::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmotionPolarity {
    Positive,
    Negative,
    #[default]
    Neutral,
}

impl EmotionPolarity {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmotionPolarity::Positive => "positive",
            EmotionPolarity::Negative => "negative",
            EmotionPolarity::Neutral => "neutral",
        }
    }
}

/// Memory record. Section 12.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub r#type: MemoryType,
    pub scope: String,

    pub content: String,
    pub entities: Vec<String>,

    pub confidence: f32,
    pub trust_score: f32,
    pub half_life_days: u32,
    pub retrieve_count: u32,
    pub last_retrieved_at: Option<DateTime<Utc>>,

    pub source_trace_id: Option<String>,
    pub source_type: SourceType,

    pub conflict_ids: Vec<String>,

    pub status: MemoryStatus,

    /// Emotion energy 0..=10. Forced 0 for non-emotion-bearing types.
    pub emotion_energy: f32,
    pub emotion_polarity: EmotionPolarity,

    /// Tier 1=core / 2=task state / 3=event snapshot / 4=atomic fragment.
    pub tier: u8,

    pub expires_at: Option<DateTime<Utc>>,
    pub cluster_member_ids: Vec<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Memory {
    /// Apply the per-type emotion-coordinate gate (Section 12.3a).
    /// Mutating constructor — call after building a draft.
    pub fn enforce_emotion_gate(mut self) -> Self {
        if !self.r#type.supports_emotion() {
            self.emotion_energy = 0.0;
            self.emotion_polarity = EmotionPolarity::Neutral;
        }
        self
    }
}

/// What kind of write-time event we're recording in memory_change_log.
/// Section 15.7.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChangeType {
    Created,
    ConfidenceUpdated,
    TrustScoreUpdated,
    TierChanged,
    ConflictFlagged,
    Deprecated,
    Merged,
    DreamClustered,
    DreamInferred,
    UserRejected,
}

impl MemoryChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryChangeType::Created => "created",
            MemoryChangeType::ConfidenceUpdated => "confidence_updated",
            MemoryChangeType::TrustScoreUpdated => "trust_score_updated",
            MemoryChangeType::TierChanged => "tier_changed",
            MemoryChangeType::ConflictFlagged => "conflict_flagged",
            MemoryChangeType::Deprecated => "deprecated",
            MemoryChangeType::Merged => "merged",
            MemoryChangeType::DreamClustered => "dream_clustered",
            MemoryChangeType::DreamInferred => "dream_inferred",
            MemoryChangeType::UserRejected => "user_rejected",
        }
    }
}
