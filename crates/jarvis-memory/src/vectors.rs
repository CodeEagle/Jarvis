//! Vector store — in-memory cosine path for the third leg of the
//! hybrid retrieval (Section 13.1).
//!
//! The store keeps a `(memory_id, embedding)` map keyed by string id
//! and exposes nearest-neighbour cosine search. Embeddings come from a
//! pluggable `EmbeddingProvider` trait so callers can swap in any
//! provider (Anthropic, OpenAI, local sentence transformer) without
//! touching the rest of the retrieval pipeline.
//!
//! This is intentionally not persisted: persistence lands when we wire
//! in sqlite-vec (the SQL extension). The shape stays the same — at
//! that point this in-memory map gets replaced by a vec table.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Return an embedding vector for `text`. Implementors are
    /// expected to use a fixed dimensionality across calls — the
    /// store doesn't enforce it but cosine breaks otherwise.
    async fn embed(&self, text: &str) -> Option<Vec<f32>>;
}

/// Deterministic test/dev embedding: lowercase token-hashing into a
/// fixed-dim float vector. Useful for unit tests and as a placeholder
/// before a real provider is wired in. Don't use this in production —
/// it's a bag-of-tokens hash, not a real semantic embedding.
pub struct HashingEmbedder {
    pub dim: usize,
}

impl HashingEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl EmbeddingProvider for HashingEmbedder {
    async fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let mut v = vec![0.0_f32; self.dim];
        for token in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
        {
            let hash = fnv1a(token);
            let bucket = (hash as usize) % self.dim;
            v[bucket] += 1.0;
        }
        normalise(&mut v);
        Some(v)
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325_u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>().clamp(-1.0, 1.0)
}

pub struct VectorStore {
    embeddings: RwLock<HashMap<String, Vec<f32>>>,
}

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorStore {
    pub fn new() -> Self {
        Self {
            embeddings: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, memory_id: impl Into<String>, embedding: Vec<f32>) {
        let mut map = self.embeddings.write().expect("vec store lock");
        map.insert(memory_id.into(), embedding);
    }

    pub fn remove(&self, memory_id: &str) {
        let mut map = self.embeddings.write().expect("vec store lock");
        map.remove(memory_id);
    }

    pub fn len(&self) -> usize {
        self.embeddings.read().expect("vec store lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Insert + persist to the durable `memory_embeddings` mirror
    /// table. The `Db` argument keeps both views in lockstep so a
    /// restart can `load_from_db` and recover the index.
    pub fn insert_persist(
        &self,
        db: &jarvis_db::Db,
        memory_id: impl Into<String>,
        embedding: Vec<f32>,
    ) -> jarvis_db::error::DbResult<()> {
        let id = memory_id.into();
        jarvis_db::embeddings::upsert(db, &id, &embedding)?;
        self.insert(id, embedding);
        Ok(())
    }

    /// Hydrate a VectorStore from the persistent mirror table. Use
    /// at process startup; subsequent inserts go through
    /// `insert_persist` to keep both sides in sync.
    pub fn load_from_db(&self, db: &jarvis_db::Db) -> jarvis_db::error::DbResult<usize> {
        let rows = jarvis_db::embeddings::list_all(db)?;
        let n = rows.len();
        for r in rows {
            self.insert(r.memory_id, r.vector);
        }
        Ok(n)
    }

    /// Return memory ids ranked by cosine similarity to `query`. Only
    /// hits with cosine ≥ `min_score` are returned.
    pub fn search(&self, query: &[f32], top_k: usize, min_score: f32) -> Vec<(String, f32)> {
        let map = self.embeddings.read().expect("vec store lock");
        let mut scored: Vec<(String, f32)> = map
            .iter()
            .map(|(id, vec)| (id.clone(), cosine_similarity(query, vec)))
            .filter(|(_, s)| *s >= min_score)
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }
}
