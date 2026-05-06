//! Growth Engine (Section 16).
//!
//! Other modules emit `GrowthEvent`s; the engine accumulates them, applies
//! Section 16.6 promotion rules to candidates, and publishes promoted
//! `GrowthArtifact`s.
//!
//! v0.2 ships:
//!   - `Collector` for emitting events to the SQLite store
//!   - `Evaluator` for tallying success/failure per artifact id
//!   - `PromotionGate` with the rule of "≥3 success, failure rate < 20%"
//!     — minus the Regression Runner, which we mock for now
//!
//! Skill regression replay (Section 16.7), Memory Lint, and the Dream
//! system come in v0.4.

pub mod artifact;
pub mod collector;
pub mod event;
pub mod model_policy;
pub mod promotion;
pub mod skill;

pub use artifact::*;
pub use collector::*;
pub use event::*;
pub use model_policy::*;
pub use promotion::*;
pub use skill::{
    new_skill, MockToolRuntime, RegressionRunner, ReplayReport, RiskLevel, Skill,
    SkillRegistry, SkillStatus, SkillStep, VerificationStep,
};

#[cfg(test)]
mod tests;
