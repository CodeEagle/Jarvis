//! ResponseSynthesizer (PRD §27.6).
//!
//! When multiple sub-Agents have produced their own SubTaskResult for
//! the same parent task — e.g. the parallel-explore command in §8.17.4
//! or the review-after-implementation pattern in §8.6 mode B/C — the
//! Orchestrator needs a deterministic rule to pick a single answer to
//! return to the user.
//!
//! PRD §27.6: "ResponseSynthesizer 冲突 → 以 ReviewerAgent 结论为准".
//! That's the canonical strategy; this module implements it plus a
//! couple of obvious fallbacks for non-reviewer multi-Agent shapes.

use crate::sub_task::{SubTaskResult, SubTaskStatus};

/// How to pick a winner when multiple SubTaskResults disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArbitrationStrategy {
    /// PRD §27.6 default: if any candidate's `agent_type` is the
    /// reviewer (`verifier`), its result wins regardless of others.
    /// Otherwise falls back to `HighestConfidenceSuccess`.
    ReviewerTakesPrecedence,
    /// Pick the highest-token-using *successful* result. Handy when
    /// the orchestrator wants the most thorough answer.
    HighestTokenSuccess,
    /// Pick the result with the most artifacts. Surrogate for "did
    /// the most work".
    MostArtifacts,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub agent_type: String,
    pub result: SubTaskResult,
}

#[derive(Debug, Clone)]
pub struct ArbitrationOutcome {
    pub winner: SubTaskResult,
    pub winner_agent_type: String,
    pub strategy: ArbitrationStrategy,
    pub reason: String,
}

/// Pick a winner among `candidates` per `strategy`.
///
/// Returns `None` only when `candidates` is empty. If all candidates
/// failed, the strategy still picks one (the highest-token failure or
/// the most-artifacts failure) so the caller has something to surface
/// rather than silently dropping the work.
pub fn arbitrate(
    candidates: &[Candidate],
    strategy: ArbitrationStrategy,
) -> Option<ArbitrationOutcome> {
    if candidates.is_empty() {
        return None;
    }

    if strategy == ArbitrationStrategy::ReviewerTakesPrecedence {
        if let Some(c) = candidates.iter().find(|c| c.agent_type == "verifier") {
            return Some(ArbitrationOutcome {
                winner: c.result.clone(),
                winner_agent_type: c.agent_type.clone(),
                strategy,
                reason: "verifier (reviewer) takes precedence per §27.6".into(),
            });
        }
        // No reviewer → fall through to highest-token-success.
        return arbitrate(candidates, ArbitrationStrategy::HighestTokenSuccess);
    }

    let success: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.result.status == SubTaskStatus::Success)
        .collect();

    let pool: &[&Candidate] = if !success.is_empty() {
        &success
    } else {
        // Use a heap-allocated borrow chain so types line up.
        return arbitrate_failures(candidates, strategy);
    };

    let pick = match strategy {
        ArbitrationStrategy::HighestTokenSuccess => pool
            .iter()
            .max_by_key(|c| c.result.token_used)
            .copied(),
        ArbitrationStrategy::MostArtifacts => pool
            .iter()
            .max_by_key(|c| c.result.artifact_ids.len())
            .copied(),
        ArbitrationStrategy::ReviewerTakesPrecedence => unreachable!("handled above"),
    };

    pick.map(|c| ArbitrationOutcome {
        winner: c.result.clone(),
        winner_agent_type: c.agent_type.clone(),
        strategy,
        reason: format!("{strategy:?} over {} successful candidate(s)", pool.len()),
    })
}

fn arbitrate_failures(
    candidates: &[Candidate],
    strategy: ArbitrationStrategy,
) -> Option<ArbitrationOutcome> {
    let pick = match strategy {
        ArbitrationStrategy::HighestTokenSuccess => candidates
            .iter()
            .max_by_key(|c| c.result.token_used),
        ArbitrationStrategy::MostArtifacts => candidates
            .iter()
            .max_by_key(|c| c.result.artifact_ids.len()),
        ArbitrationStrategy::ReviewerTakesPrecedence => candidates.first(),
    };
    pick.map(|c| ArbitrationOutcome {
        winner: c.result.clone(),
        winner_agent_type: c.agent_type.clone(),
        strategy,
        reason: format!(
            "{strategy:?}: all {} candidate(s) failed; picking least-bad",
            candidates.len()
        ),
    })
}
