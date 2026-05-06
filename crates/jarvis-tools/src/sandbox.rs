//! Tool sandbox spec.
//!
//! Real OS-level isolation (firejail / nsjail / containers) is the
//! deployment's job. What we own here is the *policy*: per-tool
//! command allowlists, argument validators, working-directory
//! restrictions, and forbidden-pattern rejections so the same checks
//! apply uniformly regardless of how the surrounding sandbox is wired.
//!
//! Used by the worker-process driver and shell-style tools to refuse
//! risky inputs before any process is even spawned.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxSpec {
    /// Programs that may be executed. Empty == nothing allowed.
    pub allowed_programs: Vec<String>,
    /// Substrings that, if present in any arg, force a denial.
    /// Defaults cover the obvious self-harm cases.
    pub forbidden_patterns: Vec<String>,
    /// Maximum cumulative arg length, characters. None = unbounded.
    pub max_args_length: Option<usize>,
    /// Working directories the tool is allowed to chdir into. Empty
    /// means inherit the parent — which the caller should still
    /// validate.
    pub allowed_cwds: Vec<String>,
}

impl SandboxSpec {
    /// Conservative default: nothing allowed; callers explicitly opt
    /// programs in.
    pub fn deny_all() -> Self {
        Self {
            allowed_programs: vec![],
            forbidden_patterns: default_forbidden(),
            max_args_length: Some(8192),
            allowed_cwds: vec![],
        }
    }

    pub fn allow_program(mut self, program: impl Into<String>) -> Self {
        self.allowed_programs.push(program.into());
        self
    }

    pub fn allow_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.allowed_cwds.push(cwd.into());
        self
    }

    pub fn forbid(mut self, pattern: impl Into<String>) -> Self {
        self.forbidden_patterns.push(pattern.into());
        self
    }

    pub fn validate(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<(), SandboxDenial> {
        if !self.allowed_programs.iter().any(|p| p == program) {
            return Err(SandboxDenial::ProgramNotAllowed(program.to_string()));
        }
        let combined: String = args.join(" ");
        if let Some(limit) = self.max_args_length {
            if combined.len() > limit {
                return Err(SandboxDenial::ArgsTooLong(combined.len(), limit));
            }
        }
        for pattern in &self.forbidden_patterns {
            if combined.contains(pattern) {
                return Err(SandboxDenial::ForbiddenPattern(pattern.clone()));
            }
            if program.contains(pattern) {
                return Err(SandboxDenial::ForbiddenPattern(pattern.clone()));
            }
        }
        if let Some(c) = cwd {
            if !self.allowed_cwds.is_empty()
                && !self.allowed_cwds.iter().any(|allowed| c.starts_with(allowed))
            {
                return Err(SandboxDenial::CwdNotAllowed(c.to_string()));
            }
        }
        Ok(())
    }
}

fn default_forbidden() -> Vec<String> {
    vec![
        "rm -rf /".into(),
        ":(){".into(), // fork bomb idiom
        "/dev/sda".into(),
        "mkfs.".into(),
        "dd if=/dev/zero".into(),
        "shutdown".into(),
        "reboot".into(),
    ]
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SandboxDenial {
    #[error("program '{0}' not in allowlist")]
    ProgramNotAllowed(String),
    #[error("args too long: {0} > {1} chars")]
    ArgsTooLong(usize, usize),
    #[error("forbidden pattern: {0}")]
    ForbiddenPattern(String),
    #[error("cwd '{0}' not allowed")]
    CwdNotAllowed(String),
}
