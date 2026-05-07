//! RegressionOrchestrator (Section 8.16).
//!
//! Pre-release / on-demand regression: load every approved
//! `WalkthroughDoc`, replay its verification checks against the current
//! filesystem, compare to the original outcome, and classify any
//! discrepancy as either an *expected change* (the file was touched
//! this round) or a *potential bug* (the file wasn't touched but the
//! verification now fails).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::verifier::{self, VerifierCheck, VerifierStore};
use crate::walkthrough::{ApprovalStatus, WalkthroughStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pass,
    ExpectedChange,
    PotentialBug,
}

impl ItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemStatus::Pass => "pass",
            ItemStatus::ExpectedChange => "expected_change",
            ItemStatus::PotentialBug => "potential_bug",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyItem {
    pub section_title: String,
    pub expected: String,
    pub actual: String,
    pub analysis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionItem {
    pub walkthrough_doc_id: String,
    pub feature_title: String,
    pub status: ItemStatus,
    #[serde(default)]
    pub discrepancies: Vec<DiscrepancyItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionReport {
    pub id: String,
    pub session_id: Option<String>,
    pub triggered_at: DateTime<Utc>,
    pub total_checks: u32,
    pub passed: u32,
    pub expected_changes: u32,
    pub potential_bugs: u32,
    pub items: Vec<RegressionItem>,
}

/// Per-doc plan a caller hands to the orchestrator: which checks to
/// run, what files they target, and whether each file was touched in
/// the current change set (used to classify discrepancies).
pub struct RegressionPlan {
    pub doc_id: String,
    pub feature_title: String,
    pub checks: Vec<RegressionCheckPlan>,
}

pub struct RegressionCheckPlan {
    pub check: VerifierCheck,
    pub target: PathBuf,
    pub touched_in_changeset: bool,
}

pub struct RegressionOrchestrator {
    walkthrough: WalkthroughStore,
    verifier: VerifierStore,
    db: Db,
}

impl RegressionOrchestrator {
    pub fn new(db: Db) -> Self {
        Self {
            walkthrough: WalkthroughStore::new(db.clone()),
            verifier: VerifierStore::new(db.clone()),
            db,
        }
    }

    /// Helper for the common "rerun every approved doc" use case. The
    /// caller supplies the per-doc plan because only it knows which
    /// files were touched in the current change set.
    pub fn run(
        &self,
        base: &Path,
        session_id: Option<&str>,
        plans: Vec<RegressionPlan>,
    ) -> DbResult<RegressionReport> {
        let mut total_checks: u32 = 0;
        let mut passed: u32 = 0;
        let mut expected_changes: u32 = 0;
        let mut potential_bugs: u32 = 0;
        let mut items: Vec<RegressionItem> = Vec::new();

        for plan in plans {
            let doc = match self.walkthrough.get(&plan.doc_id)? {
                Some(d) => d,
                None => continue,
            };
            if doc.approval_status != ApprovalStatus::Approved {
                continue;
            }

            let mut item = RegressionItem {
                walkthrough_doc_id: plan.doc_id.clone(),
                feature_title: plan.feature_title.clone(),
                status: ItemStatus::Pass,
                discrepancies: Vec::new(),
            };
            let mut item_has_failure = false;
            let mut item_touched_only = true;

            for cp in plan.checks {
                total_checks += 1;
                let mut check = cp.check;
                let ok = verifier::execute_check(&mut check, base, &cp.target);
                self.verifier.save(&check)?;
                if ok {
                    continue;
                }
                item_has_failure = true;
                if !cp.touched_in_changeset {
                    item_touched_only = false;
                }
                let analysis = if cp.touched_in_changeset {
                    "discrepancy in a file modified by this change — likely expected".into()
                } else {
                    "discrepancy in a file NOT modified by this change — flag for review".into()
                };
                item.discrepancies.push(DiscrepancyItem {
                    section_title: format!(
                        "{} ({})",
                        check.check_type.as_str(),
                        cp.target.display()
                    ),
                    expected: check.expected.clone(),
                    actual: check.actual.clone().unwrap_or_default(),
                    analysis,
                });
            }

            item.status = if !item_has_failure {
                passed += 1;
                ItemStatus::Pass
            } else if item_touched_only {
                expected_changes += 1;
                ItemStatus::ExpectedChange
            } else {
                potential_bugs += 1;
                ItemStatus::PotentialBug
            };
            items.push(item);
        }

        let report = RegressionReport {
            id: new_id_with_prefix("regr"),
            session_id: session_id.map(str::to_string),
            triggered_at: now(),
            total_checks,
            passed,
            expected_changes,
            potential_bugs,
            items,
        };
        self.persist(&report)?;
        Ok(report)
    }

    fn persist(&self, report: &RegressionReport) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO regression_reports
                    (id, session_id, triggered_at, total_checks,
                     passed, expected_changes, potential_bugs, items_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    report.id,
                    report.session_id,
                    report.triggered_at.to_rfc3339(),
                    report.total_checks,
                    report.passed,
                    report.expected_changes,
                    report.potential_bugs,
                    serde_json::to_string(&report.items).unwrap_or_else(|_| "[]".into()),
                ],
            )?;
            Ok(())
        })
    }

    /// List the most recent N regression reports, newest first.
    pub fn list_recent(&self, limit: usize) -> DbResult<Vec<RegressionReport>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, triggered_at, total_checks,
                        passed, expected_changes, potential_bugs, items_json
                   FROM regression_reports
                  ORDER BY triggered_at DESC
                  LIMIT ?1",
            )?;
            let rows = stmt
                .query_map(params![limit as i64], |row| {
                    let ts_str: String = row.get(2)?;
                    let triggered_at = DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let items_json: String = row.get(7)?;
                    Ok(RegressionReport {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        triggered_at,
                        total_checks: row.get::<_, i64>(3)? as u32,
                        passed: row.get::<_, i64>(4)? as u32,
                        expected_changes: row.get::<_, i64>(5)? as u32,
                        potential_bugs: row.get::<_, i64>(6)? as u32,
                        items: serde_json::from_str(&items_json).unwrap_or_default(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn latest(&self) -> DbResult<Option<RegressionReport>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, triggered_at, total_checks,
                        passed, expected_changes, potential_bugs, items_json
                   FROM regression_reports
                  ORDER BY triggered_at DESC
                  LIMIT 1",
            )?;
            let r = stmt
                .query_row([], |row| {
                    let ts_str: String = row.get(2)?;
                    let triggered_at = DateTime::parse_from_rfc3339(&ts_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    let items_json: String = row.get(7)?;
                    Ok(RegressionReport {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        triggered_at,
                        total_checks: row.get::<_, i64>(3)? as u32,
                        passed: row.get::<_, i64>(4)? as u32,
                        expected_changes: row.get::<_, i64>(5)? as u32,
                        potential_bugs: row.get::<_, i64>(6)? as u32,
                        items: serde_json::from_str(&items_json).unwrap_or_default(),
                    })
                })
                .optional()?;
            Ok(r)
        })
    }
}
