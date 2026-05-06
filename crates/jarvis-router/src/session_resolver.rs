//! Session matching. Section 6.

use chrono::{DateTime, Utc};
use jarvis_core::session::Session;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct SessionScoreInputs {
    pub semantic_similarity: f32,
    pub recency_score: f32,
    pub entity_overlap: f32,
}

/// Section 6.3: `score = sem*0.40 + rec*0.30 + ent*0.30`.
pub fn score(inputs: &SessionScoreInputs) -> f32 {
    inputs.semantic_similarity * 0.40
        + inputs.recency_score * 0.30
        + inputs.entity_overlap * 0.30
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionDecision {
    ContinueExisting {
        session_id: String,
        clarification_needed: bool,
    },
    CreateNew,
    /// Used when explicit_reference fires but no specific session can be
    /// pinpointed; upper layer should ask the user.
    NeedsClarification,
}

/// Apply Section 6.3 thresholds.
///
/// `>=0.75` → continue.
/// `[0.45, 0.75)` → continue but flag as low-confidence.
/// `<0.45` → new session.
pub fn decide(top_session_id: Option<&str>, top_score: f32) -> SessionDecision {
    match (top_session_id, top_score) {
        (Some(id), s) if s >= 0.75 => SessionDecision::ContinueExisting {
            session_id: id.to_string(),
            clarification_needed: false,
        },
        (Some(id), s) if s >= 0.45 => SessionDecision::ContinueExisting {
            session_id: id.to_string(),
            clarification_needed: true,
        },
        _ => SessionDecision::CreateNew,
    }
}

static EXPLICIT_REF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)继续|刚才那个|上面那个|这个问题|前面说的|还是那个|上次那个|continue|previous",
    )
    .expect("regex")
});

pub fn has_explicit_reference(input: &str) -> bool {
    EXPLICIT_REF_RE.is_match(input)
}

/// Recency: 1.0 if within last hour, decays linearly to 0.0 over 14 days.
pub fn recency_score(last_active_at: DateTime<Utc>, now: DateTime<Utc>) -> f32 {
    let hours = (now - last_active_at).num_minutes() as f32 / 60.0;
    if hours < 1.0 {
        1.0
    } else if hours > 14.0 * 24.0 {
        0.0
    } else {
        let max = 14.0 * 24.0;
        ((max - hours) / max).clamp(0.0, 1.0)
    }
}

/// Cheap token-overlap proxy for semantic similarity.
pub fn semantic_proxy(query: &str, summary: &str) -> f32 {
    use std::collections::HashSet;
    let tok = |s: &str| -> HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    };
    let a = tok(query);
    let b = tok(summary);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(&b).count();
    let union = a.union(&b).count();
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

pub fn entity_overlap(query: &str, entities: &[String]) -> f32 {
    if entities.is_empty() {
        return 0.0;
    }
    let lower = query.to_lowercase();
    let hits = entities
        .iter()
        .filter(|e| lower.contains(&e.to_lowercase()))
        .count();
    (hits as f32 / entities.len() as f32).min(1.0)
}

#[derive(Debug, Clone)]
pub struct ScoredSession {
    pub session: Session,
    pub score: f32,
    pub inputs: SessionScoreInputs,
}

pub fn rank_sessions(
    query: &str,
    candidates: &[Session],
    now: DateTime<Utc>,
) -> Vec<ScoredSession> {
    let mut scored: Vec<ScoredSession> = candidates
        .iter()
        .map(|s| {
            let inputs = SessionScoreInputs {
                semantic_similarity: semantic_proxy(query, &s.long_summary)
                    .max(semantic_proxy(query, &s.summary)),
                recency_score: recency_score(s.last_active_at, now),
                entity_overlap: entity_overlap(query, &s.active_entities),
            };
            let total = score(&inputs);
            ScoredSession {
                session: s.clone(),
                score: total,
                inputs,
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
}
