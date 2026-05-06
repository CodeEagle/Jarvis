//! Dream system layers (Section 12.6).
//!
//! - `lint` (in this crate) is the integrity layer.
//! - `dream::cluster` is the consolidation layer: it groups
//!   tightly-related Tier-4 fragments under a single `cluster_memory`
//!   that survives at higher tier, and lowers the `trust_score` of the
//!   absorbed members so they fade out of recall but stay visible for
//!   audit.
//! - `dream::infer` is the growth layer: when the same lesson surfaces
//!   ≥3 times, an `inference_memory` is minted with an `expires_at`
//!   horizon. Counter-evidence flips the inference to `deprecated`;
//!   sufficient supporting evidence promotes it into a `fact_memory`
//!   at confidence 0.75 (the spec is explicit that we never claim
//!   certainty for inferences).

use chrono::{DateTime, Duration, Utc};
use jarvis_core::ids::new_memory_id;
use jarvis_core::memory::{
    EmotionPolarity, Memory, MemoryChangeType, MemoryStatus, MemoryType, SourceType,
};
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::memory_repo;
use jarvis_db::Db;

#[derive(Debug, Clone, Copy)]
pub struct ClusterPolicy {
    pub time_window_hours: i64,
    pub min_members: usize,
    pub similarity_threshold: f32,
    /// Trust score penalty applied to absorbed members.
    pub member_trust_drop: f32,
}

impl ClusterPolicy {
    pub const fn defaults() -> Self {
        Self {
            time_window_hours: 24,
            min_members: 2,
            similarity_threshold: 0.30,
            member_trust_drop: 0.10,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClusterReport {
    pub clusters_created: u32,
    pub members_absorbed: u32,
    pub touched_ids: Vec<String>,
}

#[derive(Clone)]
pub struct DreamCluster {
    db: Db,
    policy: ClusterPolicy,
}

impl DreamCluster {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            policy: ClusterPolicy::defaults(),
        }
    }

    pub fn with_policy(db: Db, policy: ClusterPolicy) -> Self {
        Self { db, policy }
    }

    /// Run a single clustering pass over `scope`.
    pub fn run(&self, scope: &str) -> DbResult<ClusterReport> {
        let mut report = ClusterReport::default();
        let mut all = memory_repo::list_by_scope(&self.db, scope, 4096)?;
        let now_ts = now();

        // Restrict to atomic Tier-4 fragments — those are the things
        // we collapse into clusters.
        all.retain(|m| {
            m.tier == 4
                && matches!(m.status, MemoryStatus::Approved)
                && matches!(
                    m.r#type,
                    MemoryType::EpisodeMemory | MemoryType::FactMemory
                )
        });

        let window = Duration::hours(self.policy.time_window_hours);
        let mut consumed: Vec<bool> = vec![false; all.len()];

        for i in 0..all.len() {
            if consumed[i] {
                continue;
            }
            let mut group: Vec<usize> = vec![i];
            for j in (i + 1)..all.len() {
                if consumed[j] {
                    continue;
                }
                if (all[j].created_at - all[i].created_at).abs() > window {
                    continue;
                }
                let sim = jaccard_similarity(&all[i].content, &all[j].content);
                if sim >= self.policy.similarity_threshold {
                    group.push(j);
                }
            }
            if group.len() < self.policy.min_members {
                continue;
            }
            for k in &group {
                consumed[*k] = true;
            }

            let cluster = build_cluster(&all, &group, now_ts);
            memory_repo::upsert(
                &self.db,
                &cluster,
                memory_repo::WriteOpts {
                    change_type: MemoryChangeType::DreamClustered,
                    source_module: Some("dream_cluster"),
                    reason: Some("fragments consolidated"),
                },
            )?;
            report.clusters_created += 1;
            report.touched_ids.push(cluster.id.clone());

            for k in group {
                let mut absorbed = all[k].clone();
                absorbed.trust_score = (absorbed.trust_score - self.policy.member_trust_drop)
                    .max(0.05);
                absorbed.updated_at = now_ts;
                memory_repo::upsert(
                    &self.db,
                    &absorbed,
                    memory_repo::WriteOpts {
                        change_type: MemoryChangeType::TrustScoreUpdated,
                        source_module: Some("dream_cluster"),
                        reason: Some("absorbed into cluster_memory"),
                    },
                )?;
                report.members_absorbed += 1;
                report.touched_ids.push(absorbed.id.clone());
            }
        }

        Ok(report)
    }
}

fn build_cluster(all: &[Memory], group: &[usize], now_ts: DateTime<Utc>) -> Memory {
    let members: Vec<&Memory> = group.iter().map(|i| &all[*i]).collect();
    let scope = members[0].scope.clone();
    let avg_emotion: f32 = members
        .iter()
        .map(|m| m.emotion_energy)
        .sum::<f32>()
        / members.len() as f32;
    let polarity = members
        .iter()
        .map(|m| m.emotion_polarity)
        .max_by_key(|p| match p {
            EmotionPolarity::Neutral => 0,
            EmotionPolarity::Positive => 1,
            EmotionPolarity::Negative => 2,
        })
        .unwrap_or(EmotionPolarity::Neutral);
    let summary = members
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    let entities: Vec<String> = {
        let mut set = std::collections::BTreeSet::new();
        for m in &members {
            for e in &m.entities {
                set.insert(e.clone());
            }
        }
        set.into_iter().collect()
    };
    Memory {
        id: new_memory_id(),
        r#type: MemoryType::ClusterMemory,
        scope,
        content: summary,
        entities,
        confidence: 0.80,
        trust_score: 0.80,
        half_life_days: MemoryType::ClusterMemory.half_life_days(),
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::DreamCluster,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: avg_emotion,
        emotion_polarity: polarity,
        tier: 3,
        expires_at: None,
        cluster_member_ids: members.iter().map(|m| m.id.clone()).collect(),
        created_at: now_ts,
        updated_at: now_ts,
    }
    .enforce_emotion_gate()
}

fn jaccard_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let tok = |s: &str| -> HashSet<String> {
        let mut out: HashSet<String> = HashSet::new();
        let lowered = s.to_lowercase();
        for word in lowered
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
        {
            if word.is_ascii() {
                out.insert(word.to_string());
            } else {
                for ch in word.chars() {
                    out.insert(ch.to_string());
                }
            }
        }
        out
    };
    let sa = tok(a);
    let sb = tok(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

// ── inference layer ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct InferencePolicy {
    pub min_supporting_evidence: u32,
    pub validity_days: i64,
    pub confirmed_confidence: f32,
}

impl InferencePolicy {
    pub const fn defaults() -> Self {
        Self {
            min_supporting_evidence: 3,
            validity_days: 30,
            confirmed_confidence: 0.75,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InferenceReport {
    pub inferences_created: u32,
    pub confirmed_to_facts: u32,
    pub refuted: u32,
}

pub struct InferenceCandidate<'a> {
    pub scope: &'a str,
    pub claim: &'a str,
    pub supporting_evidence: u32,
    pub counter_evidence: u32,
    pub source_trace_id: Option<&'a str>,
}

#[derive(Clone)]
pub struct DreamInference {
    db: Db,
    policy: InferencePolicy,
}

impl DreamInference {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            policy: InferencePolicy::defaults(),
        }
    }

    pub fn with_policy(db: Db, policy: InferencePolicy) -> Self {
        Self { db, policy }
    }

    /// Decide what to do with a candidate inference. Returns the
    /// resulting Memory id when one is written.
    pub fn evaluate(&self, candidate: InferenceCandidate<'_>) -> DbResult<EvaluationOutcome> {
        if candidate.counter_evidence > 0 {
            return Ok(EvaluationOutcome::Refuted);
        }
        let n = now();
        if candidate.supporting_evidence >= self.policy.min_supporting_evidence {
            // Promote to fact_memory at fixed sub-1.0 confidence.
            let mem = Memory {
                id: new_memory_id(),
                r#type: MemoryType::FactMemory,
                scope: candidate.scope.to_string(),
                content: candidate.claim.to_string(),
                entities: vec![],
                confidence: self.policy.confirmed_confidence,
                trust_score: self.policy.confirmed_confidence,
                half_life_days: MemoryType::FactMemory.half_life_days(),
                retrieve_count: 0,
                last_retrieved_at: None,
                source_trace_id: candidate.source_trace_id.map(str::to_string),
                source_type: SourceType::DreamInference,
                conflict_ids: vec![],
                status: MemoryStatus::Approved,
                emotion_energy: 0.0,
                emotion_polarity: EmotionPolarity::Neutral,
                tier: 2,
                expires_at: None,
                cluster_member_ids: vec![],
                created_at: n,
                updated_at: n,
            }
            .enforce_emotion_gate();
            memory_repo::upsert(
                &self.db,
                &mem,
                memory_repo::WriteOpts {
                    change_type: MemoryChangeType::DreamInferred,
                    source_module: Some("dream_inference"),
                    reason: Some("inference confirmed → fact"),
                },
            )?;
            return Ok(EvaluationOutcome::ConfirmedAsFact { memory_id: mem.id });
        }
        // Otherwise mint an inference_memory with an expiry horizon.
        let mem = Memory {
            id: new_memory_id(),
            r#type: MemoryType::InferenceMemory,
            scope: candidate.scope.to_string(),
            content: candidate.claim.to_string(),
            entities: vec![],
            confidence: 0.55,
            trust_score: 0.55,
            half_life_days: MemoryType::InferenceMemory.half_life_days(),
            retrieve_count: 0,
            last_retrieved_at: None,
            source_trace_id: candidate.source_trace_id.map(str::to_string),
            source_type: SourceType::DreamInference,
            conflict_ids: vec![],
            status: MemoryStatus::Approved,
            emotion_energy: 0.0,
            emotion_polarity: EmotionPolarity::Neutral,
            tier: 3,
            expires_at: Some(n + Duration::days(self.policy.validity_days)),
            cluster_member_ids: vec![],
            created_at: n,
            updated_at: n,
        }
        .enforce_emotion_gate();
        memory_repo::upsert(
            &self.db,
            &mem,
            memory_repo::WriteOpts {
                change_type: MemoryChangeType::DreamInferred,
                source_module: Some("dream_inference"),
                reason: Some("inference candidate stored with expiry"),
            },
        )?;
        Ok(EvaluationOutcome::TentativeInference { memory_id: mem.id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationOutcome {
    /// Counter-evidence ruled it out. Nothing was written.
    Refuted,
    /// Supporting evidence below threshold. We persisted an
    /// `inference_memory` with an expiry horizon.
    TentativeInference { memory_id: String },
    /// Promoted directly to a `fact_memory` at sub-1.0 confidence.
    ConfirmedAsFact { memory_id: String },
}
