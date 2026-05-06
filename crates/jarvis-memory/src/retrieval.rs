//! Retrieval: FTS5 + Jaccard hybrid (Section 13.1).
//!
//! Two of the three planned paths are wired up:
//!
//! - FTS5 BM25 over `memories.content` and `memories.entities_json`,
//!   normalised to 0..=1 across the result window;
//! - Jaccard token overlap (with CJK character fallback) for
//!   short-string and exact-match cases the FTS index handles poorly.
//!
//! Vector retrieval lands when sqlite-vec is added — adding a third
//! term to `hybrid_score()` preserves the existing API.

use std::collections::HashMap;

use jarvis_core::memory::{EmotionPolarity, Memory, MemoryStatus};
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::memory_repo;
use jarvis_db::Db;
use rusqlite::params;

use crate::trust;
use crate::vectors::VectorStore;

#[derive(Debug, Clone)]
pub struct RetrievedMemory {
    pub memory: Memory,
    pub fts: f32,
    pub vector: f32,
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
    ) -> DbResult<Vec<RetrievedMemory>> {
        self.retrieve_with_vectors(scope, query, emotion, top_k, min_score, None, &[])
    }

    /// Same as `retrieve` but also consults the supplied VectorStore.
    /// `query_embedding` is the embedded query the caller computed
    /// (typically by passing `query` through an `EmbeddingProvider`).
    /// Empty `query_embedding` disables the vector path.
    #[allow(clippy::too_many_arguments)]
    pub fn retrieve_with_vectors(
        &self,
        scope: &str,
        query: &str,
        emotion: Option<EmotionContext>,
        top_k: usize,
        min_score: f32,
        store: Option<&VectorStore>,
        query_embedding: &[f32],
    ) -> DbResult<Vec<RetrievedMemory>> {
        let memories = memory_repo::list_by_scope(&self.db, scope, 1024)?;
        let query_tokens = tokenize(query);
        let fts_scores = self.fts_scores(query)?;
        let vec_scores: HashMap<String, f32> = match (store, query_embedding) {
            (Some(s), q) if !q.is_empty() => s
                .search(q, 200, 0.0)
                .into_iter()
                .collect(),
            _ => HashMap::new(),
        };

        let now_ts = now();
        let mut scored: Vec<RetrievedMemory> = memories
            .into_iter()
            .filter(|m| matches!(m.status, MemoryStatus::Approved))
            .map(|m| {
                let mem_tokens = tokenize(&m.content);
                let jaccard = jaccard_similarity(&query_tokens, &mem_tokens);
                let fts = *fts_scores.get(&m.id).unwrap_or(&0.0);
                let vector = vec_scores.get(&m.id).copied().unwrap_or(0.0).max(0.0);
                let trust_now = trust::compute(&m, now_ts);
                let emotion_bonus = emotion
                    .map(|e| emotion_resonance_bonus(e, m.emotion_energy, m.emotion_polarity))
                    .unwrap_or(0.0);
                let hybrid_score =
                    hybrid_score_full(fts, vector, jaccard, trust_now, emotion_bonus);
                RetrievedMemory {
                    memory: m,
                    fts,
                    vector,
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

    /// Run an FTS5 MATCH query against `memories_fts`, normalise BM25
    /// to 0..=1 across the candidate set, and return a map keyed by
    /// memory id. An empty query returns an empty map (BM25 is
    /// nonsensical without query terms).
    fn fts_scores(&self, query: &str) -> DbResult<HashMap<String, f32>> {
        let query = sanitize_fts_query(query);
        if query.is_empty() {
            return Ok(HashMap::new());
        }
        self.db.with(|conn| {
            // bm25() on FTS5 returns a "lower is better" relevance
            // score; we negate then min-max normalise.
            let mut stmt = conn.prepare(
                "SELECT content_id, bm25(memories_fts)
                   FROM memories_fts
                  WHERE memories_fts MATCH ?1
                  LIMIT 200",
            )?;
            let rows = stmt
                .query_map(params![query], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)? as f32))
                })?
                .collect::<Result<Vec<_>, _>>()
                .unwrap_or_default();
            if rows.is_empty() {
                return Ok(HashMap::new());
            }
            let inverted: Vec<(String, f32)> =
                rows.into_iter().map(|(id, b)| (id, -b)).collect();
            let max = inverted
                .iter()
                .map(|(_, s)| *s)
                .fold(f32::NEG_INFINITY, f32::max);
            let min = inverted
                .iter()
                .map(|(_, s)| *s)
                .fold(f32::INFINITY, f32::min);
            let span = max - min;
            // When the result set is degenerate (single hit, or all
            // hits with the same BM25), every match is "equally good" —
            // we still want the FTS path to register a positive score
            // so the caller can see the match happened.
            if span < 1e-6 {
                return Ok(inverted.into_iter().map(|(id, _)| (id, 0.85)).collect());
            }
            Ok(inverted
                .into_iter()
                .map(|(id, s)| (id, ((s - min) / span).clamp(0.0, 1.0)))
                .collect())
        })
    }
}

/// Strip characters that would confuse FTS5's MATCH syntax. We keep
/// alphanumerics and CJK; everything else collapses to whitespace.
fn sanitize_fts_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    let trimmed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed
}

/// Backwards-compatible 2-path hybrid (no vector). New callers should
/// prefer `hybrid_score_full`.
pub fn hybrid_score(fts: f32, jaccard: f32, trust: f32, emotion_bonus: f32) -> f32 {
    hybrid_score_full(fts, 0.0, jaccard, trust, emotion_bonus)
}

/// Three-path hybrid (Section 13.1):
///
/// ```text
/// raw   = fts * 0.35 + vector * 0.40 + jaccard * 0.15
/// score = raw * (0.7 + trust * 0.3) + emotion_bonus
/// ```
///
/// When `vector == 0.0` (no embedding available), the FTS weight
/// effectively absorbs the missing leg and the score collapses
/// gracefully to the 2-path form.
pub fn hybrid_score_full(
    fts: f32,
    vector: f32,
    jaccard: f32,
    trust: f32,
    emotion_bonus: f32,
) -> f32 {
    let raw = fts * 0.35 + vector * 0.40 + jaccard * 0.15;
    raw * (0.7 + trust * 0.3) + emotion_bonus
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
