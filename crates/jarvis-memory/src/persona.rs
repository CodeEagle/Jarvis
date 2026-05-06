//! Persona layer (Section 12.7).
//!
//! `persona.md` carries the assistant's tone and operating rules
//! (user-editable). `user.md` carries the user profile, autosynced
//! from `preference_memory` rows every 6 hours. Both files are
//! injected into the system prompt's *stable* layer so prompt cache
//! hits are maximised.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jarvis_core::memory::{MemoryStatus, MemoryType};
use jarvis_db::error::DbResult;
use jarvis_db::memory_repo;
use jarvis_db::Db;

#[derive(Debug, Clone)]
pub struct Persona {
    pub persona_md: String,
    pub user_md: String,
}

#[derive(Debug, Clone)]
pub struct PersonaLayer {
    base: PathBuf,
}

impl PersonaLayer {
    /// `base` is typically `~/.jarvis/profiles/<user_id>/`.
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    fn persona_path(&self) -> PathBuf {
        self.base.join("persona.md")
    }

    fn user_path(&self) -> PathBuf {
        self.base.join("user.md")
    }

    /// Read the current persona/user blocks. Missing files are
    /// returned as empty strings; callers can decide whether to seed
    /// defaults.
    pub fn load(&self) -> Result<Persona> {
        std::fs::create_dir_all(&self.base)
            .with_context(|| format!("ensuring persona dir {}", self.base.display()))?;
        Ok(Persona {
            persona_md: read_or_empty(&self.persona_path())?,
            user_md: read_or_empty(&self.user_path())?,
        })
    }

    /// Replace `persona.md`. The user is allowed to edit this file
    /// directly outside the app — we never overwrite without an explicit
    /// request, so there's no auto-write counterpart.
    pub fn write_persona(&self, body: &str) -> Result<()> {
        std::fs::create_dir_all(&self.base)?;
        std::fs::write(self.persona_path(), body).map_err(Into::into)
    }

    /// Regenerate `user.md` from approved `preference_memory` rows in
    /// the global scope. Section 12.7 spec: every 6h. We don't enforce
    /// a schedule here — the caller decides when to invoke.
    pub fn sync_user_from_memory(&self, db: &Db) -> DbResult<String> {
        let prefs = memory_repo::list_by_scope(db, "global", 1024)?;
        let mut bullets: Vec<String> = prefs
            .iter()
            .filter(|m| {
                matches!(m.r#type, MemoryType::PreferenceMemory)
                    && matches!(m.status, MemoryStatus::Approved)
            })
            .map(|m| format!("- {}", m.content.trim()))
            .collect();
        bullets.sort();
        bullets.dedup();
        let body = if bullets.is_empty() {
            "## 用户偏好\n\n_(尚未记录任何长期偏好)_\n".to_string()
        } else {
            let mut s = String::from("## 用户偏好\n\n");
            for b in bullets {
                s.push_str(&b);
                s.push('\n');
            }
            s
        };
        if let Some(parent) = self.user_path().parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(self.user_path(), &body).ok();
        Ok(body)
    }

    /// Compose the stable-layer block to inject into the system prompt.
    /// Returns an empty string if nothing has been written yet.
    pub fn render_stable_block(&self, persona: &Persona) -> String {
        let mut s = String::new();
        if !persona.persona_md.trim().is_empty() {
            s.push_str(persona.persona_md.trim());
            s.push_str("\n\n");
        }
        if !persona.user_md.trim().is_empty() {
            s.push_str(persona.user_md.trim());
            s.push('\n');
        }
        s
    }
}

fn read_or_empty(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))
}
