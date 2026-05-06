//! Memory Lint — the "tidy" layer of the Dream system.
//!
//! Section 12.6 / 30.8. Every day the Dream runner walks the memory
//! store and:
//!
//! 1. flags duplicates (similarity > 0.9) and keeps the higher
//!    `trust_score`, deprecating the loser;
//! 2. drops `scratch_memory` rows that have aged past 24h;
//! 3. expires `inference_memory` whose `expires_at` has passed;
//! 4. weakens `lesson_memory` rows that have never been retrieved and
//!    decayed below a low threshold;
//! 5. nudges trust_score downward on long-standing conflict pairs.
//!
//! Anything we mutate is paired with a `memory_change_log` entry so
//! the audit trail in Section 15.7.3 stays intact.

use chrono::{Duration, Utc};
use jarvis_core::memory::{Memory, MemoryChangeType, MemoryStatus, MemoryType};
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::memory_repo;
use jarvis_db::Db;

use crate::trust;

#[derive(Debug, Clone, Default)]
pub struct LintReport {
    pub duplicates_deprecated: u32,
    pub scratch_purged: u32,
    pub inferences_expired: u32,
    pub weak_lessons_demoted: u32,
    pub conflicts_dampened: u32,
    pub touched_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct LintPolicy {
    pub similarity_threshold: f32,
    pub scratch_max_age_hours: i64,
    pub weak_lesson_trust: f32,
    pub conflict_age_days: i64,
}

impl LintPolicy {
    pub const fn defaults() -> Self {
        Self {
            similarity_threshold: 0.90,
            scratch_max_age_hours: 24,
            weak_lesson_trust: 0.10,
            conflict_age_days: 30,
        }
    }
}

#[derive(Clone)]
pub struct MemoryLint {
    db: Db,
    policy: LintPolicy,
}

impl MemoryLint {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            policy: LintPolicy::defaults(),
        }
    }

    pub fn with_policy(db: Db, policy: LintPolicy) -> Self {
        Self { db, policy }
    }

    /// Run all lint checks across `scope`. Mutations are atomic per
    /// memory (each upsert pairs with its own change-log entry).
    pub fn run(&self, scope: &str) -> DbResult<LintReport> {
        let mut report = LintReport::default();
        let memories = memory_repo::list_by_scope(&self.db, scope, 4096)?;
        let now_ts = now();

        // Pass 1: duplicates within the active set.
        let mut active: Vec<Memory> = memories
            .iter()
            .filter(|m| matches!(m.status, MemoryStatus::Approved))
            .cloned()
            .collect();
        active.sort_by(|a, b| {
            b.trust_score
                .partial_cmp(&a.trust_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let a = &active[i];
                let b = &active[j];
                if a.r#type != b.r#type {
                    continue;
                }
                let sim = jaccard_similarity(&a.content, &b.content);
                if sim > self.policy.similarity_threshold {
                    if matches!(b.status, MemoryStatus::Deprecated) {
                        continue;
                    }
                    let mut deprecated = b.clone();
                    deprecated.status = MemoryStatus::Deprecated;
                    deprecated.updated_at = now_ts;
                    memory_repo::upsert(
                        &self.db,
                        &deprecated,
                        memory_repo::WriteOpts {
                            change_type: MemoryChangeType::Deprecated,
                            source_module: Some("memory_lint"),
                            reason: Some("duplicate of higher-trust memory"),
                        },
                    )?;
                    report.duplicates_deprecated += 1;
                    report.touched_ids.push(deprecated.id.clone());
                }
            }
        }

        // Pass 2: stale scratch memories.
        let cutoff_scratch = now_ts - Duration::hours(self.policy.scratch_max_age_hours);
        for m in &memories {
            if matches!(m.r#type, MemoryType::ScratchMemory)
                && m.created_at < cutoff_scratch
                && !matches!(m.status, MemoryStatus::Deprecated)
            {
                let mut updated = m.clone();
                updated.status = MemoryStatus::Deprecated;
                updated.updated_at = now_ts;
                memory_repo::upsert(
                    &self.db,
                    &updated,
                    memory_repo::WriteOpts {
                        change_type: MemoryChangeType::Deprecated,
                        source_module: Some("memory_lint"),
                        reason: Some("scratch memory aged out"),
                    },
                )?;
                report.scratch_purged += 1;
                report.touched_ids.push(m.id.clone());
            }
        }

        // Pass 3: expired inference memories.
        for m in &memories {
            if matches!(m.r#type, MemoryType::InferenceMemory) {
                if let Some(expires_at) = m.expires_at {
                    if expires_at < Utc::now() && !matches!(m.status, MemoryStatus::Deprecated) {
                        let mut updated = m.clone();
                        updated.status = MemoryStatus::Deprecated;
                        updated.updated_at = now_ts;
                        memory_repo::upsert(
                            &self.db,
                            &updated,
                            memory_repo::WriteOpts {
                                change_type: MemoryChangeType::Deprecated,
                                source_module: Some("memory_lint"),
                                reason: Some("inference expired"),
                            },
                        )?;
                        report.inferences_expired += 1;
                        report.touched_ids.push(m.id.clone());
                    }
                }
            }
        }

        // Pass 4: weak lessons (never retrieved, low trust).
        for m in &memories {
            if matches!(m.r#type, MemoryType::LessonMemory)
                && m.retrieve_count == 0
                && trust::compute(m, now_ts) < self.policy.weak_lesson_trust
                && !matches!(m.status, MemoryStatus::Deprecated)
            {
                let mut updated = m.clone();
                updated.status = MemoryStatus::Deprecated;
                updated.updated_at = now_ts;
                memory_repo::upsert(
                    &self.db,
                    &updated,
                    memory_repo::WriteOpts {
                        change_type: MemoryChangeType::Deprecated,
                        source_module: Some("memory_lint"),
                        reason: Some("weak lesson demoted"),
                    },
                )?;
                report.weak_lessons_demoted += 1;
                report.touched_ids.push(m.id.clone());
            }
        }

        // Pass 5: long-standing conflicts.
        let conflict_cutoff = now_ts - Duration::days(self.policy.conflict_age_days);
        for m in &memories {
            if !m.conflict_ids.is_empty()
                && m.created_at < conflict_cutoff
                && !matches!(m.status, MemoryStatus::Deprecated)
                && m.trust_score > 0.05
            {
                let mut updated = m.clone();
                updated.trust_score = (updated.trust_score - 0.05).max(0.05);
                updated.updated_at = now_ts;
                memory_repo::upsert(
                    &self.db,
                    &updated,
                    memory_repo::WriteOpts {
                        change_type: MemoryChangeType::TrustScoreUpdated,
                        source_module: Some("memory_lint"),
                        reason: Some("long-standing conflict"),
                    },
                )?;
                report.conflicts_dampened += 1;
                report.touched_ids.push(m.id.clone());
            }
        }

        Ok(report)
    }
}

/// Jaccard over both ASCII word tokens *and* individual non-ASCII
/// codepoints. Chinese phrases share characters even when whole-word
/// tokenization would say they don't, and that's what we need for
/// duplicate detection.
fn jaccard_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let tok = |s: &str| -> HashSet<String> {
        let mut out: HashSet<String> = HashSet::new();
        let lowered = s.to_lowercase();
        for word in lowered
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
        {
            if word.is_ascii() {
                out.insert(word.to_string());
            } else {
                for ch in word.chars() {
                    out.insert(ch.to_string());
                }
            }
        }
        out
    };
    let sa = tok(a);
    let sb = tok(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}
