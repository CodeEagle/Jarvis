#[cfg(test)]
use crate::*;
#[cfg(test)]
use chrono::Duration;
#[cfg(test)]
use jarvis_core::memory::*;
#[cfg(test)]
use jarvis_core::time::now;
#[cfg(test)]
use jarvis_db::Db;

fn fresh_manager() -> MemoryManager {
    MemoryManager::new(Db::in_memory().unwrap())
}

// ── write rules (Section 12.3) ──────────────────────────────────────────

#[test]
fn user_explicit_write_is_approved_with_high_confidence() {
    let mgr = fresh_manager();
    let outcome = mgr
        .write(manager::WriteRequest {
            r#type: MemoryType::PreferenceMemory,
            scope: "global",
            content: "用户不喜欢 class component",
            entities: vec!["用户".into()],
            source_type: SourceType::UserExplicit,
            source_trace_id: None,
            tier: 1,
            emotion_energy: 0.0,
            emotion_polarity: EmotionPolarity::Neutral,
            reason: None,
        })
        .unwrap();
    assert_eq!(outcome.status, MemoryStatus::Approved);
    assert!(outcome.trust_score > 0.9);
}

#[test]
fn task_result_write_is_candidate() {
    let mgr = fresh_manager();
    let outcome = mgr
        .write(manager::WriteRequest {
            r#type: MemoryType::EpisodeMemory,
            scope: "sess_1",
            content: "Bug 修复完成",
            entities: vec![],
            source_type: SourceType::TaskResult,
            source_trace_id: Some("trc_x"),
            tier: 3,
            emotion_energy: 0.0,
            emotion_polarity: EmotionPolarity::Neutral,
            reason: None,
        })
        .unwrap();
    assert_eq!(outcome.status, MemoryStatus::Candidate);
}

// ── trust score (Section 12.5 / 30.7) ───────────────────────────────────

fn build_memory(
    ty: MemoryType,
    confidence: f32,
    energy: f32,
    polarity: EmotionPolarity,
    age_days: i64,
    retrieve_count: u32,
    tier: u8,
) -> Memory {
    let n = now();
    Memory {
        id: "m_test".into(),
        r#type: ty,
        scope: "global".into(),
        content: "x".into(),
        entities: vec![],
        confidence,
        trust_score: confidence,
        half_life_days: ty.half_life_days(),
        retrieve_count,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::TaskResult,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: energy,
        emotion_polarity: polarity,
        tier,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: n - Duration::days(age_days),
        updated_at: n,
    }
}

#[test]
fn high_emotion_decays_more_slowly() {
    let n = now();
    let high = build_memory(
        MemoryType::EpisodeMemory,
        0.8,
        9.0,
        EmotionPolarity::Positive,
        14,
        0,
        3,
    );
    let low = build_memory(
        MemoryType::EpisodeMemory,
        0.8,
        0.0,
        EmotionPolarity::Neutral,
        14,
        0,
        3,
    );
    let s_high = trust::compute(&high, n);
    let s_low = trust::compute(&low, n);
    assert!(
        s_high > s_low,
        "high-emotion memory ({}) should decay slower than neutral ({})",
        s_high,
        s_low
    );
}

#[test]
fn tier1_memory_never_drops_below_floor() {
    let n = now();
    let mem = build_memory(
        MemoryType::FactMemory,
        0.5,
        0.0,
        EmotionPolarity::Neutral,
        10_000, // ancient
        0,
        1, // Tier 1
    );
    let score = trust::compute(&mem, n);
    assert!(score >= 0.30, "Tier-1 floor violated: {}", score);
}

#[test]
fn retrieve_boost_caps_at_0_20() {
    let n = now();
    let no_boost = build_memory(
        MemoryType::FactMemory,
        0.5,
        0.0,
        EmotionPolarity::Neutral,
        0,
        0,
        2,
    );
    let many_boost = build_memory(
        MemoryType::FactMemory,
        0.5,
        0.0,
        EmotionPolarity::Neutral,
        0,
        50, // would be +1.0 raw, must cap at 0.20
        2,
    );
    let n0 = trust::compute(&no_boost, n);
    let nb = trust::compute(&many_boost, n);
    let delta = nb - n0;
    assert!(delta <= 0.21, "boost capped at 0.20, got {}", delta);
}

// ── retrieval (Section 13) ──────────────────────────────────────────────

fn seed(mgr: &MemoryManager, content: &str, scope: &str) {
    mgr.write(manager::WriteRequest {
        r#type: MemoryType::PreferenceMemory,
        scope,
        content,
        entities: vec![],
        source_type: SourceType::UserExplicit,
        source_trace_id: None,
        tier: 1,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        reason: None,
    })
    .unwrap();
}

#[test]
fn retrieval_returns_top_match_for_token_overlap() {
    let mgr = fresh_manager();
    seed(&mgr, "openwrt dns resolution debug", "global");
    seed(&mgr, "react component design", "global");
    let r = retrieval::Retrieval::new(mgr.db().clone());
    let results = r
        .retrieve("global", "openwrt dns 排查", None, 5, 0.0)
        .unwrap();
    assert!(!results.is_empty());
    assert!(results[0].memory.content.contains("openwrt"));
}

#[test]
fn min_score_threshold_filters_irrelevant_memories() {
    let mgr = fresh_manager();
    seed(&mgr, "完全无关的备忘", "global");
    let r = retrieval::Retrieval::new(mgr.db().clone());
    let results = r
        .retrieve("global", "Flutter isolate", None, 5, 0.20)
        .unwrap();
    assert!(results.is_empty(), "expected empty result, got {:?}", results);
}

#[test]
fn negative_query_gets_comfort_boost_from_positive_memory() {
    use retrieval::{emotion_resonance_bonus, EmotionContext};
    let bonus = emotion_resonance_bonus(
        EmotionContext {
            energy: 8.0,
            polarity: EmotionPolarity::Negative,
        },
        7.0,
        EmotionPolarity::Positive,
    );
    assert!(bonus > 0.0);
}

#[test]
fn positive_query_gets_no_bonus_from_negative_memory() {
    use retrieval::{emotion_resonance_bonus, EmotionContext};
    let bonus = emotion_resonance_bonus(
        EmotionContext {
            energy: 8.0,
            polarity: EmotionPolarity::Positive,
        },
        7.0,
        EmotionPolarity::Negative,
    );
    assert_eq!(bonus, 0.0);
}

#[test]
fn low_emotion_query_does_not_trigger_resonance() {
    use retrieval::{emotion_resonance_bonus, EmotionContext};
    let bonus = emotion_resonance_bonus(
        EmotionContext {
            energy: 2.0,
            polarity: EmotionPolarity::Negative,
        },
        9.0,
        EmotionPolarity::Negative,
    );
    assert_eq!(bonus, 0.0);
}

#[test]
fn emotion_resonance_capped_at_0_15() {
    use retrieval::{emotion_resonance_bonus, EmotionContext};
    let bonus = emotion_resonance_bonus(
        EmotionContext {
            energy: 10.0,
            polarity: EmotionPolarity::Positive,
        },
        10.0,
        EmotionPolarity::Positive,
    );
    assert!(bonus <= 0.15 + 1e-6);
}
