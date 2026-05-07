//! CapacityAdvisor (PRD v1.9 §1.1–§1.3, §1.6).
//!
//! Decides when a Session has spent its useful context budget and the
//! AI should offer to hand off to a fresh session. The advisor itself
//! is pure: it consumes a `ContextHealth` snapshot (cheap to compute
//! from `ContextSnapshot` rows + ConversationBus state) and returns
//! one of four advisory levels.
//!
//! The four levels (PRD §1.3):
//!   - ok                       : pressure low and trend stable
//!   - warn                     : single-turn benefit dip
//!   - handoff_recommended      : sustained drop OR high pressure +
//!                                large rolling summary
//!   - handoff_required         : pressure ≥ 0.95 and force_compress
//!                                has fired ≥ 3 times
//!
//! Reverse rules from §1.6 are enforced here so callers can blindly
//! act on the level without re-checking context.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvisoryLevel {
    Ok,
    Warn,
    HandoffRecommended,
    HandoffRequired,
}

impl AdvisoryLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdvisoryLevel::Ok => "ok",
            AdvisoryLevel::Warn => "warn",
            AdvisoryLevel::HandoffRecommended => "handoff_recommended",
            AdvisoryLevel::HandoffRequired => "handoff_required",
        }
    }
}

/// Snapshot of a session's current context health. Caller fills it
/// from ContextSnapshot rows + conversation_bus state + recent
/// benefit-score window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHealth {
    pub session_id: String,
    pub trace_id: String,

    /// Current request's injected token total.
    pub injected_tokens: u32,
    /// Configured budget cap for the model's window.
    pub budget_tokens: u32,
    /// `injected / budget`, derived but kept here for serde consumers.
    pub pressure_ratio: f32,

    /// Per-turn benefit score = (effective − noise) / injected. v1.9
    /// uses a simple "did the model reference the injected memory /
    /// artifact id" proxy (PRD §3.3 option 2).
    pub benefit_score: f32,
    /// Sliding window (last 5) for `declining` detection.
    pub benefit_trend: Vec<f32>,

    /// Length of the current rolling summary in tokens — used as a
    /// surrogate for "the session is getting old and its summary
    /// itself is now expensive".
    pub rolling_summary_tokens: u32,

    /// Total times force_compress has triggered for this session.
    pub force_compress_hits: u32,

    // Reverse-rule signals (PRD §1.6).

    /// True when an owner agent is currently waiting on the user.
    /// Don't suggest a handoff in this state — first resolve the open
    /// confirmation.
    pub waiting_user: bool,

    /// Seconds since the last advisory of `>= warn` was emitted for
    /// this session. Throttle to avoid back-to-back nags.
    pub seconds_since_last_advisory: u64,

    /// True when a SteerSignal landed in the very recent past — the
    /// agent hasn't had a turn to act on it yet.
    pub steer_just_injected: bool,
}

impl ContextHealth {
    pub fn pressure(&self) -> f32 {
        if self.budget_tokens == 0 {
            return 0.0;
        }
        self.injected_tokens as f32 / self.budget_tokens as f32
    }

    /// PRD §1.3: declining = last 3 entries strictly decreasing.
    pub fn declining(&self) -> bool {
        if self.benefit_trend.len() < 3 {
            return false;
        }
        let n = self.benefit_trend.len();
        let last = &self.benefit_trend[n - 3..];
        last[0] > last[1] && last[1] > last[2]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AdvisorPolicy {
    pub throttle_seconds: u64,                 // §1.6 reverse rule (60s default)
    pub recommended_pressure_threshold: f32,    // §1.3 (0.85)
    pub recommended_summary_tokens: u32,        // §1.3 rolling summary >1500
    pub recommended_benefit_floor: f32,         // §1.3 declining + score < 0.3
    pub required_pressure_threshold: f32,       // §1.3 (0.95)
    pub required_force_compress_hits: u32,      // §1.3 (>= 3)
}

impl AdvisorPolicy {
    pub const fn defaults() -> Self {
        Self {
            throttle_seconds: 60,
            recommended_pressure_threshold: 0.85,
            recommended_summary_tokens: 1500,
            recommended_benefit_floor: 0.3,
            required_pressure_threshold: 0.95,
            required_force_compress_hits: 3,
        }
    }
}

/// Apply the policy. Pure; testable in isolation.
pub fn evaluate(h: &ContextHealth, p: AdvisorPolicy) -> AdvisoryLevel {
    let pressure = h.pressure();

    // Reverse rules first — these can demote any further escalation.
    let throttled = h.seconds_since_last_advisory < p.throttle_seconds;
    if h.waiting_user || h.steer_just_injected {
        // While the agent is mid-turn waiting on user / freshly steered,
        // don't escalate above ok — let the in-flight thing settle.
        return AdvisoryLevel::Ok;
    }

    // Hard required: very high pressure + repeated force_compress.
    if pressure >= p.required_pressure_threshold
        && h.force_compress_hits >= p.required_force_compress_hits
    {
        return AdvisoryLevel::HandoffRequired;
    }

    // Recommended (any one of):
    let declining_low = h.declining() && h.benefit_score < p.recommended_benefit_floor;
    let pressure_with_summary = pressure > p.recommended_pressure_threshold
        && h.rolling_summary_tokens > p.recommended_summary_tokens;

    if declining_low || pressure_with_summary {
        // Throttle to one advisory per `throttle_seconds`.
        if throttled {
            return AdvisoryLevel::Warn;
        }
        return AdvisoryLevel::HandoffRecommended;
    }

    // Warn: low single-turn benefit but not (yet) trending down.
    if h.benefit_score < 0.4 && pressure < p.recommended_pressure_threshold {
        return AdvisoryLevel::Warn;
    }

    AdvisoryLevel::Ok
}
