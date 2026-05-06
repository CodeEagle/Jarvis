//! Main Router (Section 5).
//!
//! The router is the only scheduling center: it receives every user input,
//! emits a `RouteDecision`, and is responsible for writing the input to
//! `raw_event_log` *before any other processing*. (Section 23.1)
//!
//! v0.1 ships only the rule layer + `@mention` parsing + `SessionResolver`
//! scoring. The LLM judgment layer described in Section 5.4 is left as a
//! seam (`IntentClassifier::classify`) so it can be plugged in later
//! without touching callers.

pub mod agent_registry;
pub mod intent;
pub mod llm_judge;
pub mod mention;
pub mod router;
pub mod session_resolver;
pub mod system_prompt;

pub use agent_registry::*;
pub use intent::*;
pub use llm_judge::{JudgeInputs, JudgeOutcome, LlmJudge, RuleBasedJudge};
pub use mention::*;
pub use router::*;
pub use session_resolver::*;
pub use system_prompt::{render_stable_block, StablePromptInputs};

#[cfg(test)]
mod tests;
