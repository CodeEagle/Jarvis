//! Dynamic model up/downgrade selector + token-budget learner (Section
//! 9.5.2b / 19.3 / 19.4).
//!
//! Both are pure-function pieces — callers feed in stats they've
//! already collected; we return a recommendation. Persistence belongs
//! in the upstream call site, which keeps this module easy to test.

use serde::{Deserialize, Serialize};

/// The three Anthropic tiers we model. Other models can be wired in by
/// extending these enums and the `upgrade()` / `downgrade()` ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Haiku,
    Sonnet,
    Opus,
}

impl ModelTier {
    pub fn upgrade(self) -> Self {
        match self {
            ModelTier::Haiku => ModelTier::Sonnet,
            ModelTier::Sonnet => ModelTier::Opus,
            ModelTier::Opus => ModelTier::Opus,
        }
    }

    pub fn downgrade(self) -> Self {
        match self {
            ModelTier::Opus => ModelTier::Sonnet,
            ModelTier::Sonnet => ModelTier::Haiku,
            ModelTier::Haiku => ModelTier::Haiku,
        }
    }
}

/// Inputs for the next-step model decision.
#[derive(Debug, Clone, Copy)]
pub struct ModelSelectionContext {
    pub current: ModelTier,
    pub task_complexity: TaskComplexity,
    pub recent_failure_rate: f32,
    pub pressure_ratio: f32,
    pub usage_budget_ratio: f32,
    /// Number of tasks since the last upgrade (for debounce).
    pub tasks_since_upgrade: u32,
    /// Number of tasks since the last downgrade.
    pub tasks_since_downgrade: u32,
    /// Whether automatic downgrades are currently allowed (Section
    /// 9.5.2b: pause auto-downgrade if recent success rate is too low).
    pub auto_downgrade_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskComplexity {
    Simple,
    Medium,
    Complex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelDecision {
    Hold,
    Upgrade,
    Downgrade,
}

/// Apply Section 9.5.2b decision matrix.
pub fn select(ctx: &ModelSelectionContext) -> ModelDecision {
    let should_upgrade = matches!(ctx.task_complexity, TaskComplexity::Complex)
        || ctx.recent_failure_rate > 0.4
        || ctx.pressure_ratio > 0.72;
    let can_downgrade = matches!(ctx.task_complexity, TaskComplexity::Simple)
        && ctx.recent_failure_rate < 0.1
        && ctx.pressure_ratio < 0.45
        && ctx.usage_budget_ratio > 0.75
        && ctx.auto_downgrade_enabled;

    // Debounce — Section 9.5.2b: keep current model for at least 3
    // tasks after upgrade, 5 tasks after downgrade.
    if should_upgrade && ctx.current != ModelTier::Opus {
        if ctx.tasks_since_upgrade < 3 {
            return ModelDecision::Hold;
        }
        return ModelDecision::Upgrade;
    }
    if can_downgrade && ctx.current != ModelTier::Haiku {
        if ctx.tasks_since_downgrade < 5 {
            return ModelDecision::Hold;
        }
        return ModelDecision::Downgrade;
    }
    ModelDecision::Hold
}

// ── token budget self-learning (Section 19.4) ───────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct BudgetAdjustmentInputs {
    pub current_budget: u32,
    /// Most recent `token_used / current_budget` ratios.
    pub recent_usage_ratios: [f32; 3],
    /// Number of force-compress events in the most recent window.
    pub recent_force_compresses: u32,
}

/// Returns a new budget. Section 19.4: shrink 20% after three
/// consecutive low-usage runs (<0.5), grow 15% after two
/// force-compress events, never deviate more than ±40% from the input
/// budget on a single adjustment.
pub fn adjust_budget(inp: BudgetAdjustmentInputs) -> u32 {
    let initial = inp.current_budget as f32;
    let low_usage_count = inp
        .recent_usage_ratios
        .iter()
        .filter(|r| **r < 0.5)
        .count();

    let mut new_budget = initial;
    if low_usage_count >= 3 {
        new_budget *= 0.80;
    }
    if inp.recent_force_compresses >= 2 {
        new_budget *= 1.15;
    }

    // ±40% guardrail.
    let lower = initial * 0.6;
    let upper = initial * 1.4;
    new_budget = new_budget.clamp(lower, upper);
    new_budget.round() as u32
}
