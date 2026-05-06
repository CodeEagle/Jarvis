//! Tentacle file generator + reader (Section 10.4).
//!
//! Each medium/complex sub-task gets its own `.tentacle/` directory:
//!
//! ```text
//! .tentacle/
//!   CONTEXT.md   — task scope, file map, constraints (write-protected)
//!   todo.md      — checkbox execution plan
//!   NOTES.md     — running notes, steer entries (append only)
//!   HANDOFF.md   — final handoff document, written once at completion
//! ```
//!
//! Section 20.4 spec: writes are protected by short-lived in-process
//! file locks. CONTEXT.md is *write-protected for sub-agents* — only
//! `TentacleGenerator::create_or_replace_context` is allowed to write
//! it. HANDOFF.md is one-shot — attempts to overwrite are rejected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TentacleSpec {
    pub task_title: String,
    pub scope_summary: String,
    pub key_files: Vec<String>,
    pub constraints: Vec<String>,
    pub do_not_break: Vec<String>,
    pub todo_steps: Vec<String>,
}

impl TentacleSpec {
    pub fn render_context(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("# 任务作用域\n\n{}\n\n", self.task_title));
        s.push_str("## 范围\n\n");
        s.push_str(&self.scope_summary);
        s.push_str("\n\n## 关键文件\n\n");
        for f in &self.key_files {
            s.push_str(&format!("- {f}\n"));
        }
        s.push_str("\n## 约束\n\n");
        for c in &self.constraints {
            s.push_str(&format!("- {c}\n"));
        }
        if !self.do_not_break.is_empty() {
            s.push_str("\n## 不要破坏\n\n");
            for d in &self.do_not_break {
                s.push_str(&format!("- {d}\n"));
            }
        }
        s
    }

    pub fn render_todo(&self) -> String {
        let mut s = String::from("# 执行计划\n\n");
        for step in &self.todo_steps {
            s.push_str(&format!("- [ ] {step}\n"));
        }
        s
    }
}

#[derive(Debug, Clone)]
pub struct TentaclePaths {
    pub root: PathBuf,
    pub context_md: PathBuf,
    pub todo_md: PathBuf,
    pub notes_md: PathBuf,
    pub handoff_md: PathBuf,
}

impl TentaclePaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            context_md: root.join("CONTEXT.md"),
            todo_md: root.join("todo.md"),
            notes_md: root.join("NOTES.md"),
            handoff_md: root.join("HANDOFF.md"),
            root,
        }
    }
}

/// Read-write Tentacle directory. Use the methods on this type instead
/// of touching the filesystem directly so the lock and write-protection
/// invariants are preserved.
pub struct Tentacle {
    paths: TentaclePaths,
}

impl Tentacle {
    pub fn paths(&self) -> &TentaclePaths {
        &self.paths
    }

    pub fn read_context(&self) -> Result<String> {
        std::fs::read_to_string(&self.paths.context_md).map_err(Into::into)
    }

    pub fn read_todo(&self) -> Result<String> {
        std::fs::read_to_string(&self.paths.todo_md).map_err(Into::into)
    }

    pub fn read_notes(&self) -> Result<String> {
        if self.paths.notes_md.exists() {
            std::fs::read_to_string(&self.paths.notes_md).map_err(Into::into)
        } else {
            Ok(String::new())
        }
    }

    pub fn read_handoff(&self) -> Result<Option<String>> {
        if self.paths.handoff_md.exists() {
            Ok(Some(std::fs::read_to_string(&self.paths.handoff_md)?))
        } else {
            Ok(None)
        }
    }

    /// Section 10.4.2 — sub-agent ticks an item by replacing its checkbox.
    pub fn tick(&self, todo_text: &str) -> Result<()> {
        let _guard = lock("todo_md", &self.paths.root)?;
        let body = std::fs::read_to_string(&self.paths.todo_md)?;
        let needle_unchecked = format!("- [ ] {todo_text}");
        let needle_checked = format!("- [x] {todo_text}");
        if !body.contains(&needle_unchecked) {
            return Err(anyhow!("todo.md does not contain `{todo_text}`"));
        }
        let updated = body.replacen(&needle_unchecked, &needle_checked, 1);
        std::fs::write(&self.paths.todo_md, updated)?;
        Ok(())
    }

    /// Append a steer / discovery line to NOTES.md.
    pub fn append_note(&self, line: &str) -> Result<()> {
        let _guard = lock("notes_md", &self.paths.root)?;
        let mut existing = if self.paths.notes_md.exists() {
            std::fs::read_to_string(&self.paths.notes_md)?
        } else {
            String::new()
        };
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str("- ");
        existing.push_str(line.trim_end());
        existing.push('\n');
        std::fs::write(&self.paths.notes_md, existing)?;
        Ok(())
    }

    /// Section 10.4.3 — HANDOFF.md is one-shot. We refuse to overwrite
    /// an existing one; callers should append to NOTES.md instead.
    pub fn write_handoff_once(&self, body: &str) -> Result<()> {
        let _guard = lock("handoff_md", &self.paths.root)?;
        if self.paths.handoff_md.exists() {
            return Err(anyhow!("HANDOFF.md already written"));
        }
        std::fs::write(&self.paths.handoff_md, body)?;
        Ok(())
    }

    /// Sub-agents call this; CONTEXT.md is write-protected. Any attempt
    /// to write returns an error per Section 20.4.
    pub fn try_overwrite_context(&self, _body: &str) -> Result<()> {
        Err(anyhow!(
            "CONTEXT.md is write-protected; only the orchestrator can replace it"
        ))
    }
}

pub struct TentacleGenerator;

impl TentacleGenerator {
    /// Materialize a `.tentacle/` directory at `root`. Idempotent: if
    /// the files already exist, CONTEXT.md is preserved and todo.md is
    /// regenerated only when missing.
    pub fn ensure(root: impl AsRef<Path>, spec: &TentacleSpec) -> Result<Tentacle> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let paths = TentaclePaths::from_root(root.to_path_buf());

        if !paths.context_md.exists() {
            let _guard = lock("context_write", &paths.root)?;
            std::fs::write(&paths.context_md, spec.render_context())?;
        }
        if !paths.todo_md.exists() {
            let _guard = lock("todo_md", &paths.root)?;
            std::fs::write(&paths.todo_md, spec.render_todo())?;
        }
        Ok(Tentacle { paths })
    }

    /// Reset the todo to the unchecked state. Used by the retry path
    /// described in Section 9.12.4 SA-6.
    pub fn reset_todo(tentacle: &Tentacle, spec: &TentacleSpec) -> Result<()> {
        let _guard = lock("todo_md", &tentacle.paths.root)?;
        std::fs::write(&tentacle.paths.todo_md, spec.render_todo())?;
        Ok(())
    }
}

// ── locks ───────────────────────────────────────────────────────────────

fn locks() -> &'static Mutex<HashMap<String, Instant>> {
    static REG: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock(kind: &str, root: &Path) -> Result<LockGuard> {
    let key = format!("{}::{}", kind, root.display());
    let mut map = locks().lock().expect("tentacle locks poisoned");
    if let Some(ts) = map.get(&key) {
        let elapsed = ts.elapsed();
        // 5s for todo, 10s for notes, 30s otherwise — we conservatively
        // use 30s as the upper bound.
        if elapsed.as_secs() < 30 {
            return Err(anyhow!(
                "tentacle lock busy: kind={kind} root={}",
                root.display()
            ));
        }
    }
    map.insert(key.clone(), Instant::now());
    Ok(LockGuard { key })
}

pub struct LockGuard {
    key: String,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = locks().lock() {
            map.remove(&self.key);
        }
    }
}
