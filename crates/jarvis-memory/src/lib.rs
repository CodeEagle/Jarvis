//! Memory subsystem: write rules, trust score, naive token-overlap retrieval.
//!
//! Section 12 / 13. The retrieval implemented here is the "Jaccard token
//! overlap" path of the planned three-way hybrid (Section 13.1). FTS5 and
//! vector retrieval will be added once the storage layer ships sqlite-vec
//! support; until then this gives us deterministic, testable recall.

pub mod cold_start;
pub mod compression;
pub mod dream;
pub mod lint;
pub mod manager;
pub mod persona;
pub mod retrieval;
pub mod trust;
pub mod vectors;

pub use cold_start::{ColdStartSnapshot, ColdStartStore, SnapshotInputs};
pub use dream::{
    ClusterPolicy, ClusterReport, DreamCluster, DreamInference,
    EvaluationOutcome, InferenceCandidate, InferencePolicy, InferenceReport,
};
pub use lint::{LintReport, MemoryLint};
pub use persona::{Persona, PersonaLayer};
pub use vectors::{cosine_similarity, EmbeddingProvider, HashingEmbedder, VectorStore};

pub use compression::*;
pub use manager::{MemoryManager, WriteOutcome};
pub use retrieval::{Retrieval, RetrievedMemory};

#[cfg(test)]
mod tests;
