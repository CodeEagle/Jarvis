//! Memory subsystem: write rules, trust score, naive token-overlap retrieval.
//!
//! Section 12 / 13. The retrieval implemented here is the "Jaccard token
//! overlap" path of the planned three-way hybrid (Section 13.1). FTS5 and
//! vector retrieval will be added once the storage layer ships sqlite-vec
//! support; until then this gives us deterministic, testable recall.

pub mod cold_start;
pub mod compression;
pub mod lint;
pub mod manager;
pub mod retrieval;
pub mod trust;

pub use cold_start::{ColdStartSnapshot, ColdStartStore, SnapshotInputs};
pub use lint::{LintReport, MemoryLint};

pub use compression::*;
pub use manager::{MemoryManager, WriteOutcome};
pub use retrieval::{Retrieval, RetrievedMemory};

#[cfg(test)]
mod tests;
