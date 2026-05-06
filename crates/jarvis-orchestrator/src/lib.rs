//! Orchestrator (Section 8).
//!
//! Hosts the task graph, artifact registry, conversation bus, tentacle
//! file generator, sub-task checkpoints, and steer protocol. v0.2 ships
//! the storage + state-machine layer; the dispatch + sub-agent execution
//! pieces land alongside the LLM judge in v0.3.

pub mod artifact_registry;
pub mod checkpoint;
pub mod conversation_bus;
pub mod steer;
pub mod sub_task;
pub mod task_tree;
pub mod tentacle;

pub use artifact_registry::*;
pub use checkpoint::*;
pub use conversation_bus::*;
pub use steer::*;
pub use sub_task::*;
pub use task_tree::*;
pub use tentacle::*;

#[cfg(test)]
mod tests;
