//! Storage layer.
//!
//! Wraps a single SQLite database. Tables fall into three groups:
//!
//! 1. Mutable state (sessions, messages, memories, task_nodes…)
//! 2. Immutable append-only logs (raw_event_log, memory_change_log,
//!    session_snapshots) — protected by SQL triggers that raise on any
//!    UPDATE or DELETE.
//! 3. Indices (FTS5 virtual tables for full-text retrieval).
//!
//! All public APIs are synchronous; the upper layers wrap them in
//! `tokio::task::spawn_blocking` when needed.

pub mod audit_log;
mod conn;
pub mod embeddings;
pub mod error;
pub mod memory_change_log;
pub mod memory_repo;
pub mod mention_log;
pub mod migrations;
pub mod handoff_repo;
pub mod outbox;
pub mod persona_repo;
pub mod provenance;
pub mod raw_event_log;
pub mod redactor;
pub mod session_repo;
pub mod session_snapshot;

pub use conn::Db;
pub use error::DbError;
pub use raw_event_log::{RawEvent, RawEventKind};

#[cfg(test)]
mod tests;
