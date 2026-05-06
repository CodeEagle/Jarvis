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

// ── compression policy (Section 30.10) ──────────────────────────────────

#[test]
fn simple_complexity_threshold_is_lower_than_default() {
    let p = compression::CompressionPolicy {
        pressure_ratio: 0.0,
        task_complexity: compression::TaskComplexity::Simple,
        usage_budget_ratio: 0.5,
    };
    let t = compression::compression_threshold(&p);
    assert!((t - 0.45).abs() < 1e-6);
}

#[test]
fn complex_task_threshold_is_higher() {
    let p = compression::CompressionPolicy {
        pressure_ratio: 0.0,
        task_complexity: compression::TaskComplexity::Complex,
        usage_budget_ratio: 0.5,
    };
    let t = compression::compression_threshold(&p);
    assert!((t - 0.65).abs() < 1e-6);
}

#[test]
fn budget_pressure_lowers_threshold() {
    let normal = compression::compression_threshold(&compression::CompressionPolicy {
        pressure_ratio: 0.0,
        task_complexity: compression::TaskComplexity::Medium,
        usage_budget_ratio: 0.5,
    });
    let tight = compression::compression_threshold(&compression::CompressionPolicy {
        pressure_ratio: 0.0,
        task_complexity: compression::TaskComplexity::Medium,
        usage_budget_ratio: 0.85,
    });
    assert!((tight - (normal - 0.10)).abs() < 1e-6);
}

#[test]
fn threshold_floor_is_0_35() {
    let t = compression::compression_threshold(&compression::CompressionPolicy {
        pressure_ratio: 0.0,
        task_complexity: compression::TaskComplexity::Simple,
        usage_budget_ratio: 0.99,
    });
    assert!(t >= 0.35 - 1e-6);
}

#[test]
fn aggressive_when_pressure_high() {
    let a = compression::aggressiveness(&compression::CompressionPolicy {
        pressure_ratio: 0.90,
        task_complexity: compression::TaskComplexity::Medium,
        usage_budget_ratio: 0.5,
    });
    assert_eq!(a, compression::Aggressiveness::Aggressive);
}

#[test]
fn light_when_complex_and_low_pressure() {
    let a = compression::aggressiveness(&compression::CompressionPolicy {
        pressure_ratio: 0.4,
        task_complexity: compression::TaskComplexity::Complex,
        usage_budget_ratio: 0.4,
    });
    assert_eq!(a, compression::Aggressiveness::Light);
}

#[test]
fn should_trigger_when_pressure_above_threshold() {
    let p = compression::CompressionPolicy {
        pressure_ratio: 0.60,
        task_complexity: compression::TaskComplexity::Simple,
        usage_budget_ratio: 0.5,
    };
    assert!(compression::should_trigger(&p));
}

// ── compression persistence ─────────────────────────────────────────────

#[test]
fn turn_summary_writes_and_lists() {
    let db = Db::in_memory().unwrap();
    let store = compression::CompressionStore::new(db);
    let id = store
        .write_turn_summary(&compression::TurnSummaryDraft {
            session_id: "sess_1".into(),
            trace_id: None,
            task_id: None,
            user_goal: "fix DNS".into(),
            actions_taken: vec!["read /etc/hosts".into()],
            result: "fixed".into(),
            entities: vec!["DNS".into()],
            unresolved: vec![],
            source_message_ids: vec!["m1".into(), "m2".into()],
            token_count: 80,
        })
        .unwrap();
    assert!(id.starts_with("turn_"));
    let listed = store.list_turn_summaries("sess_1", 10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].user_goal, "fix DNS");
}

#[test]
fn rolling_summary_versions_increment_per_session() {
    let db = Db::in_memory().unwrap();
    // session must exist for rolling-summary write to update sessions row.
    let n = now();
    jarvis_db::session_repo::upsert_session(
        &db,
        &jarvis_core::session::Session {
            id: "sess_1".into(),
            title: "x".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: jarvis_core::session::SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )
    .unwrap();

    let store = compression::CompressionStore::new(db.clone());
    let v1 = store
        .append_rolling_version(
            "sess_1",
            "v1 content",
            10,
            compression::SummaryTrigger::Manual,
            &[],
        )
        .unwrap();
    let v2 = store
        .append_rolling_version(
            "sess_1",
            "v2 content",
            12,
            compression::SummaryTrigger::TaskComplete,
            &[],
        )
        .unwrap();
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);

    let history = store.rolling_version_history("sess_1").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].0, 1);
    assert_eq!(history[1].1, "v2 content");

    let session = jarvis_db::session_repo::get_session(&db, "sess_1")
        .unwrap()
        .unwrap();
    assert_eq!(session.long_summary, "v2 content");
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
