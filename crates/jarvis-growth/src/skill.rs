//! Skill registry + Regression Runner (Sections 18 / 16.7).
//!
//! Skills are multi-step playbooks promoted from successful trace
//! evidence. The runner replays a skill's steps against a mock tool
//! runtime so promotion can verify that the steps still produce the
//! expected sequence — Section 16.7 says the runner must be sandboxed,
//! never touching real tools.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "medium" => Self::Medium,
            "high" => Self::High,
            _ => Self::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Candidate,
    Testing,
    Promoted,
    Deprecated,
}

impl SkillStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillStatus::Candidate => "candidate",
            SkillStatus::Testing => "testing",
            SkillStatus::Promoted => "promoted",
            SkillStatus::Deprecated => "deprecated",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "testing" => Self::Testing,
            "promoted" => Self::Promoted,
            "deprecated" => Self::Deprecated,
            _ => Self::Candidate,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStep {
    pub action: String,
    pub tool: Option<String>,
    pub args_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStep {
    pub tool: String,
    pub args_json: String,
    pub expected_substring: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger_intents: Vec<String>,
    pub trigger_entities: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_level: RiskLevel,
    pub steps: Vec<SkillStep>,
    pub verification: Vec<VerificationStep>,
    pub success_count: u32,
    pub failure_count: u32,
    pub status: SkillStatus,
    pub evidence_trace_ids: Vec<String>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Skill {
    pub fn failure_rate(&self) -> f32 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.0
        } else {
            self.failure_count as f32 / total as f32
        }
    }
}

#[derive(Clone)]
pub struct SkillRegistry {
    db: Db,
}

impl SkillRegistry {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn upsert(&self, skill: &Skill) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO skills
                    (id, name, description, trigger_intents_json,
                     trigger_entities_json, required_tools_json, risk_level,
                     steps_json, verification_json, success_count,
                     failure_count, status, evidence_trace_ids_json,
                     version, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                         ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(id) DO UPDATE SET
                    description = excluded.description,
                    trigger_intents_json = excluded.trigger_intents_json,
                    trigger_entities_json = excluded.trigger_entities_json,
                    required_tools_json = excluded.required_tools_json,
                    risk_level = excluded.risk_level,
                    steps_json = excluded.steps_json,
                    verification_json = excluded.verification_json,
                    success_count = excluded.success_count,
                    failure_count = excluded.failure_count,
                    status = excluded.status,
                    evidence_trace_ids_json = excluded.evidence_trace_ids_json,
                    version = excluded.version,
                    updated_at = excluded.updated_at",
                params![
                    skill.id,
                    skill.name,
                    skill.description,
                    serde_json::to_string(&skill.trigger_intents)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&skill.trigger_entities)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&skill.required_tools)
                        .unwrap_or_else(|_| "[]".into()),
                    skill.risk_level.as_str(),
                    serde_json::to_string(&skill.steps)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&skill.verification)
                        .unwrap_or_else(|_| "[]".into()),
                    skill.success_count,
                    skill.failure_count,
                    skill.status.as_str(),
                    serde_json::to_string(&skill.evidence_trace_ids)
                        .unwrap_or_else(|_| "[]".into()),
                    skill.version,
                    skill.created_at.to_rfc3339(),
                    skill.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<Skill>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(SELECT_SKILL)?;
            let r = stmt.query_row(params![id], parse_skill).optional()?;
            Ok(r)
        })
    }

    pub fn get_by_name(&self, name: &str) -> DbResult<Option<Skill>> {
        self.db.with(|conn| {
            let sql = format!("{} WHERE name = ?1", SELECT_SKILL_PREFIX);
            let mut stmt = conn.prepare(&sql)?;
            let r = stmt.query_row(params![name], parse_skill).optional()?;
            Ok(r)
        })
    }

    pub fn list(&self) -> DbResult<Vec<Skill>> {
        self.db.with(|conn| {
            let sql = format!("{} ORDER BY name ASC", SELECT_SKILL_PREFIX);
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([], parse_skill)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Match by intent / entity overlap. Returns up to `top_k` Skills
    /// scored by simple overlap count.
    pub fn match_skills(
        &self,
        intent: &str,
        entities: &[String],
        top_k: usize,
    ) -> DbResult<Vec<Skill>> {
        let all = self.list()?;
        let mut scored: Vec<(u32, Skill)> = all
            .into_iter()
            .filter(|s| s.status == SkillStatus::Promoted)
            .map(|s| {
                let mut score = 0u32;
                if s.trigger_intents.iter().any(|p| intent.starts_with(p)) {
                    score += 4;
                }
                for e in entities {
                    if s.trigger_entities.contains(e) {
                        score += 1;
                    }
                }
                (score, s)
            })
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(top_k);
        Ok(scored.into_iter().map(|(_, s)| s).collect())
    }
}

const SELECT_SKILL_PREFIX: &str = "SELECT id, name, description,
       trigger_intents_json, trigger_entities_json, required_tools_json,
       risk_level, steps_json, verification_json,
       success_count, failure_count, status,
       evidence_trace_ids_json, version, created_at, updated_at
  FROM skills";

const SELECT_SKILL: &str = "SELECT id, name, description,
       trigger_intents_json, trigger_entities_json, required_tools_json,
       risk_level, steps_json, verification_json,
       success_count, failure_count, status,
       evidence_trace_ids_json, version, created_at, updated_at
  FROM skills WHERE id = ?1";

fn parse_skill(row: &rusqlite::Row<'_>) -> rusqlite::Result<Skill> {
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
    Ok(Skill {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        trigger_intents: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        trigger_entities: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
        required_tools: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        risk_level: RiskLevel::parse(row.get::<_, String>(6)?.as_str()),
        steps: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
        verification: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
        success_count: row.get::<_, i64>(9)? as u32,
        failure_count: row.get::<_, i64>(10)? as u32,
        status: SkillStatus::parse(row.get::<_, String>(11)?.as_str()),
        evidence_trace_ids: serde_json::from_str(&row.get::<_, String>(12)?)
            .unwrap_or_default(),
        version: row.get::<_, i64>(13)? as u32,
        created_at: parse_dt(row.get(14)?)?,
        updated_at: parse_dt(row.get(15)?)?,
    })
}

pub fn new_skill(name: &str, description: &str) -> Skill {
    let n = now();
    Skill {
        id: new_id_with_prefix("skill"),
        name: name.to_string(),
        description: description.to_string(),
        trigger_intents: vec![],
        trigger_entities: vec![],
        required_tools: vec![],
        risk_level: RiskLevel::Low,
        steps: vec![],
        verification: vec![],
        success_count: 0,
        failure_count: 0,
        status: SkillStatus::Candidate,
        evidence_trace_ids: vec![],
        version: 1,
        created_at: n,
        updated_at: n,
    }
}

// ── Regression Runner (Section 16.7) ───────────────────────────────────

/// A mock-tool result hook the runner consults instead of a real tool
/// runtime. Returning `None` means the mock had no expectation — the
/// runner counts that as a "skip" rather than a failure.
pub trait MockToolRuntime: Send + Sync {
    fn invoke(&self, tool: &str, args_json: Option<&str>) -> Option<String>;
}

#[derive(Debug, Clone, Default)]
pub struct ReplayReport {
    pub steps_attempted: u32,
    pub steps_matched: u32,
    pub steps_skipped: u32,
    pub failures: Vec<String>,
}

impl ReplayReport {
    pub fn pass_rate(&self) -> f32 {
        let denom = (self.steps_attempted - self.steps_skipped).max(1);
        self.steps_matched as f32 / denom as f32
    }
}

pub struct RegressionRunner<'a, M: MockToolRuntime> {
    pub mock: &'a M,
}

impl<'a, M: MockToolRuntime> RegressionRunner<'a, M> {
    pub fn new(mock: &'a M) -> Self {
        Self { mock }
    }

    /// Replay a skill's steps. Each step's tool is invoked through the
    /// mock; we never touch a real ToolRuntime here. A step's outcome
    /// counts as a match when the mock returns Some.
    pub fn replay(&self, skill: &Skill) -> ReplayReport {
        let mut report = ReplayReport::default();
        for step in &skill.steps {
            report.steps_attempted += 1;
            match step.tool.as_deref() {
                None => {
                    report.steps_skipped += 1;
                }
                Some(tool) => match self.mock.invoke(tool, step.args_json.as_deref()) {
                    Some(_) => report.steps_matched += 1,
                    None => report
                        .failures
                        .push(format!("step '{}' had no mock result", step.action)),
                },
            }
        }
        report
    }
}
