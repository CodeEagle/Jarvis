#[cfg(test)]
use crate::*;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use jarvis_core::ids::new_id_with_prefix;
#[cfg(test)]
use jarvis_db::Db;
#[cfg(test)]
use serde_json::json;

#[cfg(test)]
fn fresh_collector() -> Collector {
    Collector::new(Db::in_memory().unwrap())
}

#[cfg(test)]
fn candidate(t: ArtifactType) -> GrowthArtifact {
    GrowthArtifact {
        id: new_id_with_prefix("art"),
        r#type: t,
        status: ArtifactStatus::Candidate,
        version: 1,
        scope: None,
        confidence: 0.5,
        evidence_trace_ids: vec![],
        payload_json: "{}".into(),
        created_at: Utc::now(),
        promoted_at: None,
    }
}

// ── Collector ───────────────────────────────────────────────────────────

#[test]
fn emit_persists_an_event() {
    let c = fresh_collector();
    let id = c
        .emit(
            SourceModule::Router,
            "route_decision",
            Some("trc_x"),
            json!({"agent": "coding"}),
        )
        .unwrap();
    assert!(id.starts_with("ge_"));
    assert_eq!(c.count_of("route_decision").unwrap(), 1);
}

#[test]
fn put_and_get_artifact_round_trip() {
    let c = fresh_collector();
    let mut art = candidate(ArtifactType::SkillCandidate);
    art.scope = Some("devops".into());
    c.put_artifact(&art).unwrap();
    let back = c.get_artifact(&art.id).unwrap().unwrap();
    assert_eq!(back.r#type, ArtifactType::SkillCandidate);
    assert_eq!(back.scope.as_deref(), Some("devops"));
}

#[test]
fn list_artifacts_filters() {
    let c = fresh_collector();
    let s1 = candidate(ArtifactType::SkillCandidate);
    let r1 = candidate(ArtifactType::RoutingExample);
    c.put_artifact(&s1).unwrap();
    c.put_artifact(&r1).unwrap();
    let only_skills = c
        .list_artifacts(Some(ArtifactType::SkillCandidate), None)
        .unwrap();
    assert_eq!(only_skills.len(), 1);
    assert_eq!(only_skills[0].id, s1.id);
}

// ── PromotionGate (Section 30.11) ───────────────────────────────────────

#[test]
fn skill_candidate_below_three_successes_blocks() {
    let gate = PromotionGate::default();
    let art = candidate(ArtifactType::SkillCandidate);
    let stats = CandidateStats {
        success_count: 2,
        failure_count: 0,
        regression_pass_rate: Some(0.95),
    };
    let d = gate.evaluate(&art, &stats);
    matches!(d, PromotionDecision::NotEnoughEvidence(_))
        .then_some(())
        .expect("expected NotEnoughEvidence");
}

#[test]
fn skill_candidate_with_high_failure_rate_blocks() {
    let gate = PromotionGate::default();
    let art = candidate(ArtifactType::SkillCandidate);
    let stats = CandidateStats {
        success_count: 5,
        failure_count: 5, // 50%
        regression_pass_rate: Some(0.95),
    };
    matches!(
        gate.evaluate(&art, &stats),
        PromotionDecision::FailureRateTooHigh(_)
    )
    .then_some(())
    .expect("expected FailureRateTooHigh");
}

#[test]
fn skill_candidate_without_regression_blocks() {
    let gate = PromotionGate::default();
    let art = candidate(ArtifactType::SkillCandidate);
    let stats = CandidateStats {
        success_count: 5,
        failure_count: 0,
        regression_pass_rate: None,
    };
    matches!(
        gate.evaluate(&art, &stats),
        PromotionDecision::RegressionFailed(_)
    )
    .then_some(())
    .expect("expected RegressionFailed");
}

#[test]
fn skill_candidate_meeting_all_rules_promotes() {
    let gate = PromotionGate::default();
    let art = candidate(ArtifactType::SkillCandidate);
    let stats = CandidateStats {
        success_count: 4,
        failure_count: 0,
        regression_pass_rate: Some(0.85),
    };
    assert_eq!(gate.evaluate(&art, &stats), PromotionDecision::Promote);
}

#[test]
fn routing_example_promotes_on_one_success() {
    let gate = PromotionGate::default();
    let art = candidate(ArtifactType::RoutingExample);
    let stats = CandidateStats {
        success_count: 1,
        failure_count: 0,
        regression_pass_rate: None,
    };
    assert_eq!(gate.evaluate(&art, &stats), PromotionDecision::Promote);
}

#[test]
fn apply_promote_advances_status_and_version() {
    let mut art = candidate(ArtifactType::RoutingExample);
    let v0 = art.version;
    promotion::apply(&PromotionDecision::Promote, &mut art);
    assert_eq!(art.status, ArtifactStatus::Promoted);
    assert!(art.promoted_at.is_some());
    assert_eq!(art.version, v0 + 1);
}

// ── model selector + budget learner (Section 30.14) ────────────────────

#[test]
fn complex_task_triggers_upgrade_after_debounce() {
    use model_policy::*;
    // Just upgraded — must hold for 3 tasks before re-evaluating.
    assert_eq!(
        select(&ModelSelectionContext {
            current: ModelTier::Sonnet,
            task_complexity: TaskComplexity::Complex,
            recent_failure_rate: 0.5,
            pressure_ratio: 0.5,
            usage_budget_ratio: 0.5,
            tasks_since_upgrade: 1,
            tasks_since_downgrade: 99,
            auto_downgrade_enabled: true,
        }),
        ModelDecision::Hold
    );
    // After debounce window — upgrade.
    assert_eq!(
        select(&ModelSelectionContext {
            current: ModelTier::Sonnet,
            task_complexity: TaskComplexity::Complex,
            recent_failure_rate: 0.5,
            pressure_ratio: 0.5,
            usage_budget_ratio: 0.5,
            tasks_since_upgrade: 5,
            tasks_since_downgrade: 99,
            auto_downgrade_enabled: true,
        }),
        ModelDecision::Upgrade
    );
}

#[test]
fn simple_task_with_budget_pressure_downgrades() {
    use model_policy::*;
    assert_eq!(
        select(&ModelSelectionContext {
            current: ModelTier::Sonnet,
            task_complexity: TaskComplexity::Simple,
            recent_failure_rate: 0.05,
            pressure_ratio: 0.40,
            usage_budget_ratio: 0.85,
            tasks_since_upgrade: 99,
            tasks_since_downgrade: 99,
            auto_downgrade_enabled: true,
        }),
        ModelDecision::Downgrade
    );
}

#[test]
fn auto_downgrade_disabled_holds() {
    use model_policy::*;
    assert_eq!(
        select(&ModelSelectionContext {
            current: ModelTier::Sonnet,
            task_complexity: TaskComplexity::Simple,
            recent_failure_rate: 0.05,
            pressure_ratio: 0.40,
            usage_budget_ratio: 0.85,
            tasks_since_upgrade: 99,
            tasks_since_downgrade: 99,
            auto_downgrade_enabled: false,
        }),
        ModelDecision::Hold
    );
}

#[test]
fn opus_does_not_upgrade_further() {
    use model_policy::*;
    assert_eq!(
        select(&ModelSelectionContext {
            current: ModelTier::Opus,
            task_complexity: TaskComplexity::Complex,
            recent_failure_rate: 0.9,
            pressure_ratio: 0.9,
            usage_budget_ratio: 0.5,
            tasks_since_upgrade: 99,
            tasks_since_downgrade: 99,
            auto_downgrade_enabled: true,
        }),
        ModelDecision::Hold
    );
}

#[test]
fn budget_shrinks_after_three_low_usage_runs() {
    use model_policy::*;
    let new = adjust_budget(BudgetAdjustmentInputs {
        current_budget: 8000,
        recent_usage_ratios: [0.3, 0.4, 0.45],
        recent_force_compresses: 0,
    });
    assert_eq!(new, (8000.0 * 0.80) as u32);
}

#[test]
fn budget_grows_after_two_force_compresses() {
    use model_policy::*;
    let new = adjust_budget(BudgetAdjustmentInputs {
        current_budget: 8000,
        recent_usage_ratios: [0.7, 0.8, 0.9],
        recent_force_compresses: 2,
    });
    assert_eq!(new, (8000.0 * 1.15) as u32);
}

#[test]
fn budget_never_deviates_more_than_40_percent() {
    use model_policy::*;
    let new = adjust_budget(BudgetAdjustmentInputs {
        current_budget: 8000,
        recent_usage_ratios: [0.05, 0.05, 0.05],
        recent_force_compresses: 99,
    });
    assert!(new >= (8000.0 * 0.6) as u32);
    assert!(new <= (8000.0 * 1.4) as u32);
}

#[test]
fn apply_block_keeps_candidate_status() {
    let mut art = candidate(ArtifactType::SkillCandidate);
    promotion::apply(
        &PromotionDecision::NotEnoughEvidence("x".into()),
        &mut art,
    );
    assert_eq!(art.status, ArtifactStatus::Candidate);
    assert!(art.promoted_at.is_none());
}
