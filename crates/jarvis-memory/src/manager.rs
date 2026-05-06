//! High-level memory write API. Always pairs writes with `memory_change_log`
//! through `jarvis-db`. Section 12.3.

use jarvis_core::ids::new_memory_id;
use jarvis_core::memory::{
    EmotionPolarity, Memory, MemoryChangeType, MemoryStatus, MemoryType, SourceType,
};
use jarvis_core::time::now;
use jarvis_db::memory_repo;
use jarvis_db::Db;

use crate::trust;

/// Result of a write call.
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub id: String,
    pub status: MemoryStatus,
    pub trust_score: f32,
}

#[derive(Clone)]
pub struct MemoryManager {
    db: Db,
}

pub struct WriteRequest<'a> {
    pub r#type: MemoryType,
    pub scope: &'a str,
    pub content: &'a str,
    pub entities: Vec<String>,
    pub source_type: SourceType,
    pub source_trace_id: Option<&'a str>,
    pub tier: u8,
    pub emotion_energy: f32,
    pub emotion_polarity: EmotionPolarity,
    pub reason: Option<&'a str>,
}

impl MemoryManager {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Apply Section 12.3 write rules and persist.
    ///
    /// - `user_explicit` → `Approved`, default confidence 0.95.
    /// - `correction` → `Approved`, default confidence 0.90.
    /// - `task_result` → `Candidate`, awaits Growth Engine review.
    /// - `inferred` (low confidence) is forced into a `ScratchMemory`.
    pub fn write(&self, req: WriteRequest<'_>) -> jarvis_db::error::DbResult<WriteOutcome> {
        let confidence = req.source_type.default_confidence();
        let (mem_type, status) = match req.source_type {
            SourceType::UserExplicit | SourceType::Correction => {
                (req.r#type, MemoryStatus::Approved)
            }
            SourceType::TaskResult => (req.r#type, MemoryStatus::Candidate),
            SourceType::DreamCluster | SourceType::DreamInference => {
                (req.r#type, MemoryStatus::Candidate)
            }
            SourceType::Inferred if confidence < 0.5 => {
                (MemoryType::ScratchMemory, MemoryStatus::Candidate)
            }
            SourceType::Inferred => (req.r#type, MemoryStatus::Candidate),
        };

        let half_life = mem_type.half_life_days();
        let now_ts = now();
        let trust_score = trust::initial(mem_type, confidence);

        let mem = Memory {
            id: new_memory_id(),
            r#type: mem_type,
            scope: req.scope.to_string(),
            content: req.content.to_string(),
            entities: req.entities,
            confidence,
            trust_score,
            half_life_days: half_life,
            retrieve_count: 0,
            last_retrieved_at: None,
            source_trace_id: req.source_trace_id.map(str::to_string),
            source_type: req.source_type,
            conflict_ids: vec![],
            status,
            emotion_energy: req.emotion_energy,
            emotion_polarity: req.emotion_polarity,
            tier: req.tier,
            expires_at: None,
            cluster_member_ids: vec![],
            created_at: now_ts,
            updated_at: now_ts,
        }
        .enforce_emotion_gate();

        memory_repo::upsert(
            &self.db,
            &mem,
            memory_repo::WriteOpts {
                change_type: MemoryChangeType::Created,
                source_module: Some("memory_manager"),
                reason: req.reason,
            },
        )?;

        Ok(WriteOutcome {
            id: mem.id,
            status: mem.status,
            trust_score: mem.trust_score,
        })
    }

    pub fn db(&self) -> &Db {
        &self.db
    }
}
