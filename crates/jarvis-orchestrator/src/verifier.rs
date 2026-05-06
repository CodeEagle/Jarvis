//! VerifierAgent (Section 8.15).
//!
//! Independent verification — never trusts a WalkthroughDoc's claims, but
//! re-checks them against the filesystem (and, eventually, a re-run of
//! the test/lint commands). v0.2 implements the data layer + the
//! `file_exists` and `file_content_contains` checks against the real
//! filesystem; the test/lint runners arrive with the tool runtime
//! upgrades in v0.3.

use std::path::Path;

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::walkthrough::{VerificationStatus, WalkthroughStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    FileExists,
    FileContent,
    TestRun,
    LintRun,
    DiffMatch,
}

impl CheckType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckType::FileExists => "file_exists",
            CheckType::FileContent => "file_content",
            CheckType::TestRun => "test_run",
            CheckType::LintRun => "lint_run",
            CheckType::DiffMatch => "diff_match",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierCheck {
    pub id: String,
    pub walkthrough_doc_id: String,
    pub section_id: String,
    pub check_type: CheckType,
    pub expected: String,
    pub actual: Option<String>,
    pub r#match: Option<bool>,
    pub discrepancy: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
}

impl VerifierCheck {
    pub fn new(
        walkthrough_doc_id: &str,
        section_id: &str,
        check_type: CheckType,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            id: new_id_with_prefix("vc"),
            walkthrough_doc_id: walkthrough_doc_id.to_string(),
            section_id: section_id.to_string(),
            check_type,
            expected: expected.into(),
            actual: None,
            r#match: None,
            discrepancy: None,
            executed_at: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerifierReport {
    pub doc_id: String,
    pub all_match: bool,
    pub failures: Vec<String>,
}

#[derive(Clone)]
pub struct VerifierStore {
    db: Db,
}

impl VerifierStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn save(&self, check: &VerifierCheck) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO verifier_checks
                    (id, walkthrough_doc_id, section_id, check_type,
                     expected, actual, match_result, discrepancy, executed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    actual = excluded.actual,
                    match_result = excluded.match_result,
                    discrepancy = excluded.discrepancy,
                    executed_at = excluded.executed_at",
                params![
                    check.id,
                    check.walkthrough_doc_id,
                    check.section_id,
                    check.check_type.as_str(),
                    check.expected,
                    check.actual,
                    check.r#match.map(|b| if b { 1 } else { 0 }),
                    check.discrepancy,
                    check.executed_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_for_doc(&self, doc_id: &str) -> DbResult<Vec<VerifierCheck>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, walkthrough_doc_id, section_id, check_type,
                        expected, actual, match_result, discrepancy, executed_at
                   FROM verifier_checks
                  WHERE walkthrough_doc_id = ?1
                  ORDER BY executed_at ASC NULLS FIRST",
            )?;
            let rows = stmt
                .query_map(params![doc_id], parse_check)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}

fn parse_check(row: &rusqlite::Row<'_>) -> rusqlite::Result<VerifierCheck> {
    let check_type_str: String = row.get(3)?;
    let check_type = match check_type_str.as_str() {
        "file_exists" => CheckType::FileExists,
        "file_content" => CheckType::FileContent,
        "test_run" => CheckType::TestRun,
        "lint_run" => CheckType::LintRun,
        "diff_match" => CheckType::DiffMatch,
        _ => CheckType::FileExists,
    };
    let match_int: Option<i64> = row.get(6)?;
    let executed_str: Option<String> = row.get(8)?;
    let executed_at = executed_str
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
        })
        .transpose()?;
    Ok(VerifierCheck {
        id: row.get(0)?,
        walkthrough_doc_id: row.get(1)?,
        section_id: row.get(2)?,
        check_type,
        expected: row.get(4)?,
        actual: row.get(5)?,
        r#match: match_int.map(|n| n != 0),
        discrepancy: row.get(7)?,
        executed_at,
    })
}

/// Run a single check against the local filesystem. Test/lint runners
/// will route through `jarvis-tools::ToolRuntime` once it's available
/// to verifier code; for now we cover the static checks that don't
/// require process spawning.
pub fn execute_check(check: &mut VerifierCheck, base: &Path, expected_path: &Path) -> bool {
    check.executed_at = Some(now());
    match check.check_type {
        CheckType::FileExists => {
            let full = base.join(expected_path);
            let exists = full.exists();
            check.actual = Some(if exists {
                format!("{} exists", full.display())
            } else {
                format!("{} missing", full.display())
            });
            check.r#match = Some(exists);
            if !exists {
                check.discrepancy = Some(format!(
                    "expected `{}` to exist but it does not",
                    full.display()
                ));
            }
            exists
        }
        CheckType::FileContent => {
            let full = base.join(expected_path);
            let body = std::fs::read_to_string(&full).unwrap_or_default();
            let contains = body.contains(&check.expected);
            check.actual = Some(if contains {
                "contains expected substring".into()
            } else {
                "does not contain expected substring".into()
            });
            check.r#match = Some(contains);
            if !contains {
                check.discrepancy = Some(format!(
                    "`{}` does not contain `{}`",
                    full.display(),
                    check.expected
                ));
            }
            contains
        }
        // Process-based runners deferred — `match` stays None.
        CheckType::TestRun | CheckType::LintRun | CheckType::DiffMatch => {
            check.r#match = None;
            check.discrepancy = Some("runner not yet implemented".into());
            false
        }
    }
}

/// Apply all checks to the doc and update its verification status.
pub fn run(
    walkthrough: &WalkthroughStore,
    verifier: &VerifierStore,
    doc_id: &str,
    base: &Path,
    checks: Vec<(VerifierCheck, std::path::PathBuf)>,
) -> DbResult<VerifierReport> {
    let mut all_match = true;
    let mut failures: Vec<String> = Vec::new();

    for (mut check, target) in checks {
        let ok = execute_check(&mut check, base, &target);
        verifier.save(&check)?;
        if !ok {
            all_match = false;
            if let Some(d) = &check.discrepancy {
                failures.push(d.clone());
            }
        }
    }

    let mut doc = walkthrough
        .get(doc_id)?
        .ok_or_else(|| jarvis_db::error::DbError::NotFound(format!("walkthrough {doc_id}")))?;
    doc.verification_status = if all_match {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Disputed
    };
    doc.verified_at = Some(now());
    if !failures.is_empty() {
        doc.verification_notes = Some(failures.join("\n"));
    }
    walkthrough.save(&doc)?;

    Ok(VerifierReport {
        doc_id: doc_id.to_string(),
        all_match,
        failures,
    })
}
