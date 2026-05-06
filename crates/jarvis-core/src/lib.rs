//! Core domain types for Jarvis.
//!
//! Pure data — no I/O, no async, no external dependencies beyond serde/chrono/uuid.
//! Other crates depend on these types; this crate depends on nothing else in the
//! workspace.

pub mod agent;
pub mod ids;
pub mod intent;
pub mod memory;
pub mod mention;
pub mod route;
pub mod session;
pub mod task;
pub mod time;
pub mod tool;

pub use agent::*;
pub use ids::*;
pub use intent::*;
pub use memory::*;
pub use mention::*;
pub use route::*;
pub use session::*;
pub use task::*;
pub use time::*;
pub use tool::*;

#[cfg(test)]
mod tests;
