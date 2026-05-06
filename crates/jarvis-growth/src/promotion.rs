//! Promotion Gate (Section 16.6) and a mock Regression Runner (16.7).

use chrono::Utc;

use crate::artifact::{ArtifactStatus, ArtifactType, GrowthArtifact};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionDecision {
    Promote,
    NotEnoughEvidence(String),
    FailureRateTooHigh(String),
    RegressionFailed(String),
    Rejected(String),
}

#[derive(Debug, Clone)]
pub struct CandidateStats {
    pub success_count: u32,
    pub failure_count: u32,
    /// Output of the Regression Runner. `None` means the runner hasn't
    /// run yet → Skill candidates can't be promoted, but other artifact
    /// types may bypass this signal.
    pub regression_pass_rate: Option<f32>,
}

impl CandidateStats {
    pub fn failure_rate(&self) -> f32 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.0
        } else {
            self.failure_count as f32 / total as f32
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PromotionPolicy {
    pub min_success: u32,
    pub max_failure_rate: f32,
    pub min_regression_pass_rate: f32,
}

impl PromotionPolicy {
    pub const fn defaults() -> Self {
        Self {
            min_success: 3,
            max_failure_rate: 0.20,
            min_regression_pass_rate: 0.80,
        }
    }
}

pub struct PromotionGate {
    policy: PromotionPolicy,
}

impl Default for PromotionGate {
    fn default() -> Self {
        Self::new(PromotionPolicy::defaults())
    }
}

impl PromotionGate {
    pub fn new(policy: PromotionPolicy) -> Self {
        Self { policy }
    }

    /// Section 16.6 rules.
    pub fn evaluate(
        &self,
        artifact: &GrowthArtifact,
        stats: &CandidateStats,
    ) -> PromotionDecision {
        match artifact.r#type {
            // User-explicit memory candidates auto-promote when they
            // arrive (the manager already wrote them as approved); other
            // types follow the standard rules.
            ArtifactType::MemoryCandidate => self.eval_memory(stats),
            ArtifactType::SkillCandidate => self.eval_skill(stats),
            ArtifactType::RoutingExample => {
                if stats.success_count >= 1 {
                    PromotionDecision::Promote
                } else {
                    PromotionDecision::NotEnoughEvidence(
                        "routing_example needs ≥1 evidence".into(),
                    )
                }
            }
            _ => self.eval_default(stats),
        }
    }

    fn eval_skill(&self, stats: &CandidateStats) -> PromotionDecision {
        if stats.success_count < self.policy.min_success {
            return PromotionDecision::NotEnoughEvidence(format!(
                "skill needs ≥{} successes, have {}",
                self.policy.min_success, stats.success_count
            ));
        }
        if stats.failure_rate() > self.policy.max_failure_rate {
            return PromotionDecision::FailureRateTooHigh(format!(
                "skill failure rate {:.0}% > {:.0}%",
                stats.failure_rate() * 100.0,
                self.policy.max_failure_rate * 100.0
            ));
        }
        match stats.regression_pass_rate {
            None => PromotionDecision::RegressionFailed(
                "skill candidate cannot promote without regression run".into(),
            ),
            Some(p) if p < self.policy.min_regression_pass_rate => {
                PromotionDecision::RegressionFailed(format!(
                    "regression pass rate {:.0}% < {:.0}%",
                    p * 100.0,
                    self.policy.min_regression_pass_rate * 100.0
                ))
            }
            Some(_) => PromotionDecision::Promote,
        }
    }

    fn eval_memory(&self, stats: &CandidateStats) -> PromotionDecision {
        // Per Section 12.3: explicit confirmation → promote on first
        // positive signal; inferred candidates need at least 3.
        if stats.success_count >= 3 {
            return PromotionDecision::Promote;
        }
        if stats.success_count >= 1 && stats.failure_count == 0 {
            // Accept but treat as testing — caller decides what to do.
            return PromotionDecision::Promote;
        }
        PromotionDecision::NotEnoughEvidence("memory candidate needs ≥1 success".into())
    }

    fn eval_default(&self, stats: &CandidateStats) -> PromotionDecision {
        if stats.success_count < self.policy.min_success {
            PromotionDecision::NotEnoughEvidence(format!(
                "needs ≥{} successes, have {}",
                self.policy.min_success, stats.success_count
            ))
        } else if stats.failure_rate() > self.policy.max_failure_rate {
            PromotionDecision::FailureRateTooHigh(format!(
                "failure rate {:.0}% > {:.0}%",
                stats.failure_rate() * 100.0,
                self.policy.max_failure_rate * 100.0
            ))
        } else {
            PromotionDecision::Promote
        }
    }
}

/// Apply a `PromotionDecision` to an artifact in-place.
pub fn apply(decision: &PromotionDecision, artifact: &mut GrowthArtifact) {
    match decision {
        PromotionDecision::Promote => {
            artifact.status = ArtifactStatus::Promoted;
            artifact.promoted_at = Some(Utc::now());
            artifact.version += 1;
        }
        PromotionDecision::Rejected(_) => {
            artifact.status = ArtifactStatus::Rejected;
        }
        PromotionDecision::NotEnoughEvidence(_)
        | PromotionDecision::FailureRateTooHigh(_)
        | PromotionDecision::RegressionFailed(_) => {
            artifact.status = ArtifactStatus::Candidate;
        }
    }
}
