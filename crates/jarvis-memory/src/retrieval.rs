//! v0.1 retrieval: token-overlap (Jaccard) over `memories.content`.
//!
//! Section 13.1 specifies a three-way hybrid (FTS5 + vector + Jaccard).
//! Until vector and FTS5 are wired in, Jaccard alone gives us a fully
//! deterministic retrieval path that's easy to test. The hybrid score
//! shape is preserved so adding new components doesn't change the API.

use jarvis_core::memory::{EmotionPolarity, Memory, MemoryStatus};
use jarvis_core::time::now;
use jarvis_db::memory_repo;
use jarvis_db::Db;

use crate::trust;

#[derive(Debug, Clone)]
pub struct RetrievedMemory {
    pub memory: Memory,
    pub jaccard: f32,
    pub trust_now: f32,
    pub emotion_bonus: f32,
    pub hybrid_score: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EmotionContext {
    pub energy: f32,
    pub polarity: EmotionPolarity,
}

#[derive(Clone)]
pub struct Retrieval {
    db: Db,
}

impl Retrieval {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Retrieve top-K memories for `scope` ranked by hybrid score.
    /// `min_score` filters everything below the threshold (Section 13 门槛机制).
    pub fn retrieve(
        &self,
        scope: &str,
        query: &str,
        emotion: Option<EmotionContext>,
        top_k: usize,
        min_score: f32,
    ) -> jarvis_db::error::DbResult<Vec<RetrievedMemory>> {
        let memories = memory_repo::list_by_scope(&self.db, scope, 1024)?;
        let query_tokens = tokenize(query);

        let now_ts = now();
        let mut scored: Vec<RetrievedMemory> = memories
            .into_iter()
            .filter(|m| matches!(m.status, MemoryStatus::Approved))
            .map(|m| {
                let mem_tokens = tokenize(&m.content);
                let jaccard = jaccard_similarity(&query_tokens, &mem_tokens);
                let trust_now = trust::compute(&m, now_ts);
                let emotion_bonus = emotion
                    .map(|e| emotion_resonance_bonus(e, m.emotion_energy, m.emotion_polarity))
                    .unwrap_or(0.0);
                let hybrid_score = hybrid_score(jaccard, trust_now, emotion_bonus);
                RetrievedMemory {
                    memory: m,
                    jaccard,
                    trust_now,
                    emotion_bonus,
                    hybrid_score,
                }
            })
            .filter(|r| r.hybrid_score >= min_score)
            .collect();

        scored.sort_by(|a, b| {
            b.hybrid_score
                .partial_cmp(&a.hybrid_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        Ok(scored)
    }
}

/// Hybrid score (Section 13.1 simplified to one path + trust + emotion).
///
/// ```text
/// score = (jaccard * w_jaccard) * (0.7 + trust * 0.3) + emotion_bonus
/// ```
///
/// When FTS5 / vector retrievers are added, additional weighted terms
/// fold into the parenthesised expression.
pub fn hybrid_score(jaccard: f32, trust: f32, emotion_bonus: f32) -> f32 {
    (jaccard * 0.50) * (0.7 + trust * 0.3) + emotion_bonus
}

/// Section 13.2. Bonus capped at 0.15. Triggered only when the query has
/// non-trivial emotional energy. Negative-query / positive-memory gives
/// a comfort boost; the converse does not.
pub fn emotion_resonance_bonus(
    query: EmotionContext,
    memory_energy: f32,
    memory_polarity: EmotionPolarity,
) -> f32 {
    if query.energy < 3.0 {
        return 0.0;
    }
    let aligned = match (query.polarity, memory_polarity) {
        // Same-polarity match → resonance.
        (EmotionPolarity::Positive, EmotionPolarity::Positive) => true,
        (EmotionPolarity::Negative, EmotionPolarity::Negative) => true,
        // Negative query is comforted by positive memory.
        (EmotionPolarity::Negative, EmotionPolarity::Positive) => true,
        _ => false,
    };
    if !aligned {
        return 0.0;
    }
    let strength =
        ((query.energy / 10.0) * (memory_energy / 10.0)).clamp(0.0, 1.0);
    (strength * 0.15).min(0.15)
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    use std::collections::HashSet;
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    let inter = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}
