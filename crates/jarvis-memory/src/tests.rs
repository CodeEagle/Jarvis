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
fn fts5_retrieval_finds_keyword_match() {
    let mgr = fresh_manager();
    seed(&mgr, "openwrt dns hosts diagnostic", "global");
    seed(&mgr, "react ssr hydration", "global");
    seed(&mgr, "flutter isolate streaming", "global");
    let r = retrieval::Retrieval::new(mgr.db().clone());
    let results = r
        .retrieve("global", "openwrt", None, 5, 0.0)
        .unwrap();
    assert!(!results.is_empty());
    assert!(results[0].memory.content.contains("openwrt"));
    // FTS path should have produced a non-zero score.
    assert!(results[0].fts > 0.0);
}

#[test]
fn fts5_query_ignores_punctuation() {
    let mgr = fresh_manager();
    seed(&mgr, "用户偏好 Riverpod 状态管理", "global");
    let r = retrieval::Retrieval::new(mgr.db().clone());
    let results = r
        .retrieve("global", "Riverpod 状态管理 ?!", None, 5, 0.0)
        .unwrap();
    assert!(!results.is_empty());
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

// ── cold-start snapshot ─────────────────────────────────────────────────

#[test]
fn cold_start_snapshot_capture_and_active() {
    let db = Db::in_memory().unwrap();
    let store = cold_start::ColdStartStore::new(db);
    let snap = store
        .capture(cold_start::SnapshotInputs {
            session_id: "sess_1",
            rolling_summary: "v1",
            recent_messages: &["hi".into()],
            memory_hits: &["mem_1".into()],
            cross_session: &[],
            valid_for_turns: 5,
        })
        .unwrap();
    assert_eq!(snap.used_turns, 0);
    let active = store.active_for("sess_1").unwrap().unwrap();
    assert_eq!(active.id, snap.id);
}

#[test]
fn cold_start_snapshot_retires_after_used_turns() {
    let db = Db::in_memory().unwrap();
    let store = cold_start::ColdStartStore::new(db);
    let snap = store
        .capture(cold_start::SnapshotInputs {
            session_id: "sess_1",
            rolling_summary: "v1",
            recent_messages: &[],
            memory_hits: &[],
            cross_session: &[],
            valid_for_turns: 2,
        })
        .unwrap();
    store.touch(&snap.id).unwrap();
    store.touch(&snap.id).unwrap();
    let active = store.active_for("sess_1").unwrap();
    assert!(active.is_none(), "snapshot should retire after used_turns >= valid_for_turns");
}

// ── dream cluster + inference (Section 30.8) ────────────────────────────

#[cfg(test)]
fn write_episode(db: &Db, id: &str, content: &str, age_minutes: i64) -> Memory {
    let n = now() - chrono::Duration::minutes(age_minutes);
    let m = Memory {
        id: id.into(),
        r#type: MemoryType::EpisodeMemory,
        scope: "global".into(),
        content: content.into(),
        entities: vec![],
        confidence: 0.7,
        trust_score: 0.7,
        half_life_days: MemoryType::EpisodeMemory.half_life_days(),
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::TaskResult,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        tier: 4,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: n,
        updated_at: n,
    };
    jarvis_db::memory_repo::upsert(
        db,
        &m,
        jarvis_db::memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();
    m
}

#[test]
fn dream_cluster_builds_cluster_memory_from_related_fragments() {
    let db = Db::in_memory().unwrap();
    write_episode(&db, "f1", "调试 sync_manager 找到 race condition", 0);
    write_episode(&db, "f2", "调试 sync_manager 修复 race condition 完成", 60);
    write_episode(&db, "f3", "调试 sync_manager 添加测试用例", 120);

    let dream = dream::DreamCluster::with_policy(
        db.clone(),
        dream::ClusterPolicy {
            time_window_hours: 24,
            min_members: 2,
            similarity_threshold: 0.30,
            member_trust_drop: 0.10,
        },
    );
    let report = dream.run("global").unwrap();
    assert!(report.clusters_created >= 1);
    assert!(report.members_absorbed >= 2);
}

#[test]
fn dream_cluster_skips_when_below_min_members() {
    let db = Db::in_memory().unwrap();
    write_episode(&db, "alone", "孤立片段", 0);
    let dream = dream::DreamCluster::new(db);
    let report = dream.run("global").unwrap();
    assert_eq!(report.clusters_created, 0);
}

#[test]
fn dream_inference_refuted_when_counter_evidence() {
    let db = Db::in_memory().unwrap();
    let inf = dream::DreamInference::new(db);
    let outcome = inf
        .evaluate(dream::InferenceCandidate {
            scope: "global",
            claim: "用户从不在周末工作",
            supporting_evidence: 5,
            counter_evidence: 1,
            source_trace_id: None,
        })
        .unwrap();
    assert_eq!(outcome, dream::EvaluationOutcome::Refuted);
}

#[test]
fn dream_inference_promoted_to_fact_at_threshold() {
    let db = Db::in_memory().unwrap();
    let inf = dream::DreamInference::new(db.clone());
    let outcome = inf
        .evaluate(dream::InferenceCandidate {
            scope: "global",
            claim: "用户偏好凌晨写代码",
            supporting_evidence: 3,
            counter_evidence: 0,
            source_trace_id: None,
        })
        .unwrap();
    let mem_id = match outcome {
        dream::EvaluationOutcome::ConfirmedAsFact { memory_id } => memory_id,
        other => panic!("expected confirmation, got {other:?}"),
    };
    let stored = jarvis_db::memory_repo::get(&db, &mem_id).unwrap().unwrap();
    assert_eq!(stored.r#type, MemoryType::FactMemory);
    // Section 12.6 says confirmed inferences cap at 0.75.
    assert!((stored.confidence - 0.75).abs() < 1e-3);
}

#[test]
fn dream_inference_below_threshold_creates_tentative_with_expiry() {
    let db = Db::in_memory().unwrap();
    let inf = dream::DreamInference::new(db.clone());
    let outcome = inf
        .evaluate(dream::InferenceCandidate {
            scope: "global",
            claim: "可能偏好 React 而非 Vue",
            supporting_evidence: 2,
            counter_evidence: 0,
            source_trace_id: None,
        })
        .unwrap();
    let mem_id = match outcome {
        dream::EvaluationOutcome::TentativeInference { memory_id } => memory_id,
        other => panic!("expected tentative, got {other:?}"),
    };
    let stored = jarvis_db::memory_repo::get(&db, &mem_id).unwrap().unwrap();
    assert_eq!(stored.r#type, MemoryType::InferenceMemory);
    assert!(stored.expires_at.is_some());
}

// ── persona layer ───────────────────────────────────────────────────────

#[test]
fn persona_layer_writes_and_reads() {
    let dir = tempfile::tempdir().unwrap();
    let layer = persona::PersonaLayer::new(dir.path());
    layer.write_persona("## 语气\n简洁直接").unwrap();
    let p = layer.load().unwrap();
    assert!(p.persona_md.contains("简洁直接"));
}

#[test]
fn persona_user_md_synced_from_preference_memory() {
    let mgr = fresh_manager();
    mgr.write(manager::WriteRequest {
        r#type: MemoryType::PreferenceMemory,
        scope: "global",
        content: "代码风格偏好函数式",
        entities: vec![],
        source_type: SourceType::UserExplicit,
        source_trace_id: None,
        tier: 1,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        reason: None,
    })
    .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let layer = persona::PersonaLayer::new(dir.path());
    let body = layer.sync_user_from_memory(mgr.db()).unwrap();
    assert!(body.contains("代码风格偏好函数式"));
}

#[test]
fn persona_render_stable_block_combines_blocks() {
    let layer = persona::PersonaLayer::new(std::env::temp_dir().join("persona_test"));
    let p = persona::Persona {
        persona_md: "P".into(),
        user_md: "U".into(),
    };
    let block = layer.render_stable_block(&p);
    assert!(block.contains("P"));
    assert!(block.contains("U"));
}

// ── memory lint (Section 30.8) ──────────────────────────────────────────

#[cfg(test)]
fn write_memory(mgr: &MemoryManager, content: &str, ty: MemoryType) -> String {
    let outcome = mgr
        .write(manager::WriteRequest {
            r#type: ty,
            scope: "global",
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
    outcome.id
}

#[test]
fn lint_deprecates_duplicate_keeping_higher_trust() {
    let mgr = fresh_manager();
    let id1 = write_memory(&mgr, "用户喜欢简洁代码风格", MemoryType::PreferenceMemory);
    let _id2 = write_memory(&mgr, "用户偏好简洁代码风格", MemoryType::PreferenceMemory);

    // Manually nudge id2's trust score lower to ensure it loses.
    let mut victim = jarvis_db::memory_repo::get(mgr.db(), &_id2).unwrap().unwrap();
    victim.trust_score = 0.3;
    jarvis_db::memory_repo::upsert(
        mgr.db(),
        &victim,
        jarvis_db::memory_repo::WriteOpts {
            change_type: MemoryChangeType::TrustScoreUpdated,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();

    let policy = lint::LintPolicy {
        similarity_threshold: 0.55,
        ..lint::LintPolicy::defaults()
    };
    let lint = lint::MemoryLint::with_policy(mgr.db().clone(), policy);
    let report = lint.run("global").unwrap();
    assert!(report.duplicates_deprecated >= 1);

    let kept = jarvis_db::memory_repo::get(mgr.db(), &id1).unwrap().unwrap();
    assert_eq!(kept.status, MemoryStatus::Approved);
}

#[test]
fn lint_purges_aged_scratch() {
    let db = Db::in_memory().unwrap();
    // write scratch directly with backdated created_at.
    let m = jarvis_core::memory::Memory {
        id: "scratch_x".into(),
        r#type: MemoryType::ScratchMemory,
        scope: "global".into(),
        content: "transient note".into(),
        entities: vec![],
        confidence: 0.3,
        trust_score: 0.3,
        half_life_days: 0,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::Inferred,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        tier: 4,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: now() - chrono::Duration::hours(48),
        updated_at: now(),
    };
    jarvis_db::memory_repo::upsert(
        &db,
        &m,
        jarvis_db::memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();
    let lint = lint::MemoryLint::new(db.clone());
    let report = lint.run("global").unwrap();
    assert_eq!(report.scratch_purged, 1);
    let after = jarvis_db::memory_repo::get(&db, "scratch_x").unwrap().unwrap();
    assert_eq!(after.status, MemoryStatus::Deprecated);
}

#[test]
fn lint_expires_inference_memory_past_expiry() {
    let db = Db::in_memory().unwrap();
    let m = jarvis_core::memory::Memory {
        id: "inf_x".into(),
        r#type: MemoryType::InferenceMemory,
        scope: "global".into(),
        content: "用户每天上午开始工作".into(),
        entities: vec![],
        confidence: 0.5,
        trust_score: 0.5,
        half_life_days: 30,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::DreamInference,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        tier: 3,
        expires_at: Some(now() - chrono::Duration::days(1)),
        cluster_member_ids: vec![],
        created_at: now() - chrono::Duration::days(60),
        updated_at: now(),
    };
    jarvis_db::memory_repo::upsert(
        &db,
        &m,
        jarvis_db::memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();
    let lint = lint::MemoryLint::new(db.clone());
    let report = lint.run("global").unwrap();
    assert_eq!(report.inferences_expired, 1);
    let after = jarvis_db::memory_repo::get(&db, "inf_x").unwrap().unwrap();
    assert_eq!(after.status, MemoryStatus::Deprecated);
}

#[test]
fn lint_demotes_weak_unread_lesson() {
    let db = Db::in_memory().unwrap();
    let m = jarvis_core::memory::Memory {
        id: "lesson_x".into(),
        r#type: MemoryType::LessonMemory,
        scope: "global".into(),
        content: "never modify generated files".into(),
        entities: vec![],
        confidence: 0.05, // very low → trust below threshold
        trust_score: 0.05,
        half_life_days: 90,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::Correction,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        tier: 3,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: now(),
        updated_at: now(),
    };
    jarvis_db::memory_repo::upsert(
        &db,
        &m,
        jarvis_db::memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();
    let lint = lint::MemoryLint::new(db.clone());
    let report = lint.run("global").unwrap();
    assert!(report.weak_lessons_demoted >= 1);
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
