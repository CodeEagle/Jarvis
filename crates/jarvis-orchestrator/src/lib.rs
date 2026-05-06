//! Orchestrator (Section 8).
//!
//! Hosts the task graph, artifact registry, conversation bus, tentacle
//! file generator, sub-task checkpoints, and steer protocol. v0.2 ships
//! the storage + state-machine layer; the dispatch + sub-agent execution
//! pieces land alongside the LLM judge in v0.3.

pub mod activity_card;
pub mod artifact_registry;
pub mod checkpoint;
pub mod conversation_bus;
pub mod steer;
pub mod sub_task;
pub mod task_tree;
pub mod tentacle;
pub mod verifier;
pub mod walkthrough;
pub mod workspace;

pub use activity_card::*;
pub use artifact_registry::*;
pub use checkpoint::*;
pub use conversation_bus::*;
pub use steer::*;
pub use sub_task::*;
pub use task_tree::*;
pub use tentacle::*;
pub use verifier::{
    execute_check as execute_verifier_check, run as run_verifier,
    CheckType, VerifierCheck, VerifierReport, VerifierStore,
};
pub use walkthrough::{
    new_draft as new_walkthrough_draft, new_section as new_walkthrough_section,
    ApprovalStatus, AutoApprovalDecision, RiskLevel, SectionType,
    VerificationStatus, WalkthroughDoc, WalkthroughSection, WalkthroughStore,
};
pub use workspace::*;

#[cfg(test)]
mod tests;
