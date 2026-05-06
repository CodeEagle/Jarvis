//! Workspace lock + worktree manager (Section 10.4 / 20.3).
//!
//! A "workspace" is a project root that may host concurrent agent
//! activity. Section 20.3 forbids two write-capable agents from
//! operating on the same workspace at the same time. Read-only agents
//! (e.g. ResearchAgent reading files) are allowed to run in parallel.
//!
//! A "worktree" is a per-task git-style isolated copy under
//! `.jarvis/worktrees/<task_id>/`. v0.2 ships file-system bookkeeping
//! only — actual `git worktree add` integration arrives when the
//! Codex driver lands in v0.3.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    /// Read-only agent — multiple may share the workspace.
    Reader,
    /// Write-capable agent — exclusive lock required.
    Writer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLockInfo {
    pub workspace: String,
    pub owner_task_id: String,
    pub mode: WorkspaceMode,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
struct WorkspaceState {
    writer: Option<WorkspaceLockInfo>,
    readers: HashMap<String, WorkspaceLockInfo>, // keyed by task id
    last_touched: Instant,
}

fn registry() -> &'static Mutex<HashMap<String, WorkspaceState>> {
    static R: OnceLock<Mutex<HashMap<String, WorkspaceState>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(workspace: &Path) -> String {
    workspace.display().to_string()
}

/// RAII guard returned by `acquire`. Drop releases the lock.
pub struct WorkspaceLock {
    workspace: String,
    task_id: String,
    mode: WorkspaceMode,
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let mut map = match registry().lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        if let Some(state) = map.get_mut(&self.workspace) {
            match self.mode {
                WorkspaceMode::Writer => {
                    if state
                        .writer
                        .as_ref()
                        .map(|w| w.owner_task_id == self.task_id)
                        .unwrap_or(false)
                    {
                        state.writer = None;
                    }
                }
                WorkspaceMode::Reader => {
                    state.readers.remove(&self.task_id);
                }
            }
            state.last_touched = Instant::now();
        }
    }
}

impl WorkspaceLock {
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn mode(&self) -> WorkspaceMode {
        self.mode
    }
}

pub struct WorkspaceLocker;

impl WorkspaceLocker {
    pub fn acquire(
        workspace: &Path,
        task_id: &str,
        mode: WorkspaceMode,
    ) -> Result<WorkspaceLock> {
        let key = key(workspace);
        let mut map = registry().lock().expect("workspace lock map poisoned");
        let state = map.entry(key.clone()).or_insert(WorkspaceState {
            writer: None,
            readers: HashMap::new(),
            last_touched: Instant::now(),
        });

        match mode {
            WorkspaceMode::Writer => {
                if let Some(w) = &state.writer {
                    return Err(anyhow!(
                        "workspace `{}` is already locked by writer {}",
                        key,
                        w.owner_task_id
                    ));
                }
                if !state.readers.is_empty() {
                    return Err(anyhow!(
                        "workspace `{}` has active readers; writer cannot acquire",
                        key
                    ));
                }
                state.writer = Some(WorkspaceLockInfo {
                    workspace: key.clone(),
                    owner_task_id: task_id.to_string(),
                    mode,
                    acquired_at: chrono::Utc::now(),
                });
            }
            WorkspaceMode::Reader => {
                if state.writer.is_some() {
                    return Err(anyhow!(
                        "workspace `{}` is locked by a writer; reader cannot acquire",
                        key
                    ));
                }
                state.readers.insert(
                    task_id.to_string(),
                    WorkspaceLockInfo {
                        workspace: key.clone(),
                        owner_task_id: task_id.to_string(),
                        mode,
                        acquired_at: chrono::Utc::now(),
                    },
                );
            }
        }
        state.last_touched = Instant::now();

        Ok(WorkspaceLock {
            workspace: key,
            task_id: task_id.to_string(),
            mode,
        })
    }

    /// Snapshot for debugging / dashboards.
    pub fn inspect(workspace: &Path) -> Option<(Option<WorkspaceLockInfo>, Vec<WorkspaceLockInfo>)> {
        let map = registry().lock().ok()?;
        let state = map.get(&key(workspace))?;
        Some((state.writer.clone(), state.readers.values().cloned().collect()))
    }
}

// ── worktree management ─────────────────────────────────────────────────

/// Per-task isolated worktree under `.jarvis/worktrees/<task_id>`.
#[derive(Debug, Clone)]
pub struct Worktree {
    pub task_id: String,
    pub root: PathBuf,
}

pub struct WorktreeManager {
    base: PathBuf,
}

impl WorktreeManager {
    /// `base` is typically the workspace root. The manager creates and
    /// removes worktrees beneath `<base>/.jarvis/worktrees/`.
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn worktrees_root(&self) -> PathBuf {
        self.base.join(".jarvis").join("worktrees")
    }

    pub fn create(&self, task_id: &str) -> Result<Worktree> {
        let root = self.worktrees_root().join(task_id);
        std::fs::create_dir_all(&root).with_context(|| {
            format!("creating worktree dir {}", root.display())
        })?;
        Ok(Worktree {
            task_id: task_id.to_string(),
            root,
        })
    }

    pub fn discard(&self, worktree: &Worktree) -> Result<()> {
        if worktree.root.exists() {
            std::fs::remove_dir_all(&worktree.root)
                .with_context(|| format!("removing worktree {}", worktree.root.display()))?;
        }
        Ok(())
    }

    pub fn exists(&self, task_id: &str) -> bool {
        self.worktrees_root().join(task_id).exists()
    }
}

// ── durable file-based workspace lock ───────────────────────────────────
//
// The in-memory `WorkspaceLocker` only coordinates within one process.
// When two `jarvis` invocations target the same workspace at the same
// time we need a sentinel on disk. `DurableWorkspaceLock` uses
// `OpenOptions::create_new(true)` for atomic create-if-absent, then
// removes the file on Drop. Stale locks (e.g. process killed
// mid-write) get cleaned up by `clear_stale_lock` when the caller
// supplies a max_age.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::time::SystemTime;

#[derive(Debug)]
pub struct DurableWorkspaceLock {
    path: PathBuf,
}

impl DurableWorkspaceLock {
    /// Acquire `<workspace>/.jarvis/locks/<task_id>.lock`. Returns
    /// Err with kind AlreadyExists when a lock file is present.
    pub fn acquire(workspace: &Path, task_id: &str) -> Result<Self> {
        let dir = workspace.join(".jarvis").join("locks");
        std::fs::create_dir_all(&dir).with_context(|| {
            format!("creating lock dir {}", dir.display())
        })?;
        let path = dir.join(format!("{task_id}.lock"));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| anyhow!("acquire lock {}: {}", path.display(), e))?;
        let pid = std::process::id();
        let now_ts = chrono::Utc::now().to_rfc3339();
        writeln!(f, "{{\"task_id\":\"{task_id}\",\"pid\":{pid},\"acquired_at\":\"{now_ts}\"}}")?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sweep the locks directory for files older than `max_age` and
    /// remove them. Returns the count of files removed.
    pub fn clear_stale_locks(workspace: &Path, max_age: std::time::Duration) -> Result<u32> {
        let dir = workspace.join(".jarvis").join("locks");
        if !dir.exists() {
            return Ok(0);
        }
        let mut removed = 0u32;
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata.modified().unwrap_or(SystemTime::now());
            if SystemTime::now()
                .duration_since(modified)
                .map(|d| d > max_age)
                .unwrap_or(false)
            {
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    if e.kind() != ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl Drop for DurableWorkspaceLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
