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
