//! WalkthroughAgent + WalkthroughDoc + auto-approval policy.
//!
//! Section 8.14. The doc is a structured summary the human (or the
//! Orchestrator) approves before a sub-task's output is considered
//! "done". v0.2 ships the data + persistence + approval policy. The
//! generator that synthesises a doc from a HANDOFF.md and SubTaskResult
//! lands when the LLM judge arrives.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionType {
    Summary,
    Change,
    Screenshot,
    Diff,
    TestResult,
    Risk,
}

impl SectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SectionType::Summary => "summary",
            SectionType::Change => "change",
            SectionType::Screenshot => "screenshot",
            SectionType::Diff => "diff",
            SectionType::TestResult => "test_result",
            SectionType::Risk => "risk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::None => "none",
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    Verified,
    Disputed,
    Skipped,
}

impl VerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationStatus::Pending => "pending",
            VerificationStatus::Verified => "verified",
            VerificationStatus::Disputed => "disputed",
            VerificationStatus::Skipped => "skipped",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "verified" => Self::Verified,
            "disputed" => Self::Disputed,
            "skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkthroughSection {
    pub id: String,
    pub r#type: SectionType,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub risk_level: Option<RiskLevel>,
    #[serde(default)]
    pub risk_note: Option<String>,
    /// True when the section reports test outcomes and at least one
    /// case failed. Used by the auto-approval rule.
    #[serde(default)]
    pub has_test_failure: bool,
    /// Number of files this section claims to have changed (used by
    /// auto-approval cap).
    #[serde(default)]
    pub files_changed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkthroughDoc {
    pub id: String,
    pub sub_task_id: String,
    pub session_id: String,
    pub trace_id: Option<String>,
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub agent_type: String,
    pub sections: Vec<WalkthroughSection>,
    pub verification_status: VerificationStatus,
    pub verification_notes: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub approval_status: ApprovalStatus,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

impl WalkthroughDoc {
    /// Section 8.14.6 auto-approval rules.
    ///
    /// auto-approve when:
    /// - verification_status == Verified
    /// - all section risk_level <= Low
    /// - no test failures across `test_result` sections
    /// - total files_changed across `change` sections <= 5
    ///
    /// auto-reject when verification_status == Disputed or any section
    /// reports `risk_level == High`.
    ///
    /// otherwise: human review required.
    pub fn auto_decide(&self) -> AutoApprovalDecision {
        if self.verification_status == VerificationStatus::Disputed {
            return AutoApprovalDecision::AutoReject(
                "verifier disputed the walkthrough".into(),
            );
        }
        if self.sections.iter().any(|s| {
            matches!(s.risk_level, Some(RiskLevel::High))
        }) {
            return AutoApprovalDecision::RequireHuman(
                "high-risk section present".into(),
            );
        }
        if self.verification_status != VerificationStatus::Verified {
            return AutoApprovalDecision::RequireHuman(
                "verifier has not yet confirmed".into(),
            );
        }
        let highest_risk = self
            .sections
            .iter()
            .filter_map(|s| s.risk_level)
            .max_by_key(|r| match r {
                RiskLevel::None => 0,
                RiskLevel::Low => 1,
                RiskLevel::Medium => 2,
                RiskLevel::High => 3,
            })
            .unwrap_or(RiskLevel::None);
        if matches!(highest_risk, RiskLevel::Medium) {
            return AutoApprovalDecision::RequireHuman(
                "medium-risk section present".into(),
            );
        }
        let any_test_failure = self
            .sections
            .iter()
            .filter(|s| s.r#type == SectionType::TestResult)
            .any(|s| s.has_test_failure);
        if any_test_failure {
            return AutoApprovalDecision::RequireHuman("test failure present".into());
        }
        let total_files_changed: u32 = self
            .sections
            .iter()
            .filter(|s| s.r#type == SectionType::Change)
            .map(|s| s.files_changed)
            .sum();
        if total_files_changed > 5 {
            return AutoApprovalDecision::RequireHuman(format!(
                "{total_files_changed} files changed (cap=5)"
            ));
        }
        AutoApprovalDecision::AutoApprove
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoApprovalDecision {
    AutoApprove,
    AutoReject(String),
    RequireHuman(String),
}

#[derive(Clone)]
pub struct WalkthroughStore {
    db: Db,
}

impl WalkthroughStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn save(&self, doc: &WalkthroughDoc) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO walkthrough_docs
                    (id, sub_task_id, session_id, trace_id, title, generated_at,
                     agent_type, sections_json, verification_status,
                     verification_notes, verified_at, approval_status,
                     approved_by, approved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    sections_json = excluded.sections_json,
                    verification_status = excluded.verification_status,
                    verification_notes = excluded.verification_notes,
                    verified_at = excluded.verified_at,
                    approval_status = excluded.approval_status,
                    approved_by = excluded.approved_by,
                    approved_at = excluded.approved_at",
                params![
                    doc.id,
                    doc.sub_task_id,
                    doc.session_id,
                    doc.trace_id,
                    doc.title,
                    doc.generated_at.to_rfc3339(),
                    doc.agent_type,
                    serde_json::to_string(&doc.sections).unwrap_or_else(|_| "[]".into()),
                    doc.verification_status.as_str(),
                    doc.verification_notes,
                    doc.verified_at.map(|t| t.to_rfc3339()),
                    doc.approval_status.as_str(),
                    doc.approved_by,
                    doc.approved_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<WalkthroughDoc>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(SELECT_DOC)?;
            let r = stmt.query_row(params![id], parse_doc).optional()?;
            Ok(r)
        })
    }

    /// Apply Section 8.14.6 rules and persist.
    pub fn auto_review(&self, id: &str) -> DbResult<AutoApprovalDecision> {
        let mut doc = match self.get(id)? {
            Some(d) => d,
            None => {
                return Err(jarvis_db::error::DbError::NotFound(format!(
                    "walkthrough {id}"
                )))
            }
        };
        let decision = doc.auto_decide();
        match &decision {
            AutoApprovalDecision::AutoApprove => {
                doc.approval_status = ApprovalStatus::Approved;
                doc.approved_by = Some("auto".into());
                doc.approved_at = Some(now());
            }
            AutoApprovalDecision::AutoReject(_) => {
                doc.approval_status = ApprovalStatus::Rejected;
                doc.approved_by = Some("auto".into());
                doc.approved_at = Some(now());
            }
            AutoApprovalDecision::RequireHuman(_) => {
                doc.approval_status = ApprovalStatus::Pending;
            }
        }
        self.save(&doc)?;
        Ok(decision)
    }

    /// Manually approve a walkthrough — used by callers that drove
    /// the human review loop themselves.
    pub fn approve(&self, id: &str, by: &str) -> DbResult<WalkthroughDoc> {
        let mut doc = self
            .get(id)?
            .ok_or_else(|| jarvis_db::error::DbError::NotFound(format!("walkthrough {id}")))?;
        doc.approval_status = ApprovalStatus::Approved;
        doc.approved_by = Some(by.to_string());
        doc.approved_at = Some(now());
        self.save(&doc)?;
        Ok(doc)
    }

    pub fn reject(&self, id: &str, by: &str, reason: Option<&str>) -> DbResult<WalkthroughDoc> {
        let mut doc = self
            .get(id)?
            .ok_or_else(|| jarvis_db::error::DbError::NotFound(format!("walkthrough {id}")))?;
        doc.approval_status = ApprovalStatus::Rejected;
        doc.approved_by = Some(by.to_string());
        doc.approved_at = Some(now());
        if let Some(r) = reason {
            doc.verification_notes = Some(match doc.verification_notes {
                Some(prev) => format!("{prev}\nrejected: {r}"),
                None => format!("rejected: {r}"),
            });
        }
        self.save(&doc)?;
        Ok(doc)
    }

    pub fn list_for_session(&self, session_id: &str) -> DbResult<Vec<WalkthroughDoc>> {
        self.db.with(|conn| {
            let sql = format!(
                "{} WHERE session_id = ?1 ORDER BY generated_at DESC",
                SELECT_DOC_PREFIX
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params![session_id], parse_doc)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn list_approved(&self) -> DbResult<Vec<WalkthroughDoc>> {
        self.db.with(|conn| {
            let sql = format!(
                "{} WHERE approval_status = 'approved' ORDER BY generated_at DESC",
                SELECT_DOC_PREFIX
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([], parse_doc)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }
}

const SELECT_DOC_PREFIX: &str = "SELECT id, sub_task_id, session_id, trace_id, title,
       generated_at, agent_type, sections_json,
       verification_status, verification_notes, verified_at,
       approval_status, approved_by, approved_at
  FROM walkthrough_docs";

const SELECT_DOC: &str = "SELECT id, sub_task_id, session_id, trace_id, title,
       generated_at, agent_type, sections_json,
       verification_status, verification_notes, verified_at,
       approval_status, approved_by, approved_at
  FROM walkthrough_docs WHERE id = ?1";

fn parse_doc(row: &rusqlite::Row<'_>) -> rusqlite::Result<WalkthroughDoc> {
    let parse_dt = |s: String| -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let sections_json: String = row.get(7)?;
    let verified_str: Option<String> = row.get(10)?;
    let approved_str: Option<String> = row.get(13)?;
    Ok(WalkthroughDoc {
        id: row.get(0)?,
        sub_task_id: row.get(1)?,
        session_id: row.get(2)?,
        trace_id: row.get(3)?,
        title: row.get(4)?,
        generated_at: parse_dt(row.get(5)?)?,
        agent_type: row.get(6)?,
        sections: serde_json::from_str(&sections_json).unwrap_or_default(),
        verification_status: VerificationStatus::parse(
            row.get::<_, String>(8)?.as_str(),
        ),
        verification_notes: row.get(9)?,
        verified_at: verified_str.map(parse_dt).transpose()?,
        approval_status: ApprovalStatus::parse(row.get::<_, String>(11)?.as_str()),
        approved_by: row.get(12)?,
        approved_at: approved_str.map(parse_dt).transpose()?,
    })
}

/// Construct a draft doc with sensible default ids/timestamps. Callers
/// should still set the title and sections.
pub fn new_draft(
    sub_task_id: &str,
    session_id: &str,
    agent_type: &str,
    title: &str,
) -> WalkthroughDoc {
    WalkthroughDoc {
        id: new_id_with_prefix("wt"),
        sub_task_id: sub_task_id.to_string(),
        session_id: session_id.to_string(),
        trace_id: None,
        title: title.to_string(),
        generated_at: now(),
        agent_type: agent_type.to_string(),
        sections: vec![],
        verification_status: VerificationStatus::Pending,
        verification_notes: None,
        verified_at: None,
        approval_status: ApprovalStatus::Pending,
        approved_by: None,
        approved_at: None,
    }
}

pub fn new_section(
    section_type: SectionType,
    title: &str,
    content: &str,
) -> WalkthroughSection {
    WalkthroughSection {
        id: new_id_with_prefix("sec"),
        r#type: section_type,
        title: title.to_string(),
        content: content.to_string(),
        risk_level: None,
        risk_note: None,
        has_test_failure: false,
        files_changed: 0,
    }
}
