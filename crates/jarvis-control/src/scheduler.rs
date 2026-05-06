//! Background scheduler.
//!
//! Owns the periodic jobs the spec mandates:
//!   - Dream Memory Lint (daily by default — Section 12.6)
//!   - Dream Cluster pass (daily, after lint)
//!   - Stale workspace lock sweep (configurable)
//!
//! The scheduler is built around `tokio::time::interval` so jobs can
//! co-tick with the rest of the runtime without each module wiring
//! its own timer. Every job invocation logs a GrowthEvent so the
//! Growth Engine can spot regressions in lint productivity.

use std::sync::Arc;
use std::time::Duration;

use jarvis_db::Db;
use jarvis_growth::{Collector, SourceModule};
use jarvis_memory::lint::MemoryLint;
use jarvis_memory::dream::DreamCluster;
use serde_json::json;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub dream_lint_period: Duration,
    pub dream_cluster_period: Duration,
    pub stale_lock_sweep_period: Duration,
    pub stale_lock_max_age: Duration,
    pub workspace_root: Option<std::path::PathBuf>,
    /// Memory scope to lint / cluster. Defaults to "global".
    pub scope: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            dream_lint_period: Duration::from_secs(24 * 3600),
            dream_cluster_period: Duration::from_secs(24 * 3600),
            stale_lock_sweep_period: Duration::from_secs(3600),
            stale_lock_max_age: Duration::from_secs(30 * 60),
            workspace_root: None,
            scope: "global".to_string(),
        }
    }
}

/// Scheduler handle. `start` returns a `JoinHandle` — drop it to
/// detach, abort it to stop the timers (jobs will finish their
/// current tick first).
pub struct Scheduler {
    db: Db,
    cfg: SchedulerConfig,
}

impl Scheduler {
    pub fn new(db: Db, cfg: SchedulerConfig) -> Self {
        Self { db, cfg }
    }

    pub fn start(self) -> Vec<tokio::task::JoinHandle<()>> {
        let lint = self.spawn_lint();
        let cluster = self.spawn_cluster();
        let sweep = self.spawn_stale_sweep();
        vec![lint, cluster, sweep]
    }

    fn spawn_lint(&self) -> tokio::task::JoinHandle<()> {
        let db = self.db.clone();
        let scope = self.cfg.scope.clone();
        let period = self.cfg.dream_lint_period;
        tokio::spawn(async move {
            let collector = Collector::new(db.clone());
            let lint = MemoryLint::new(db);
            let mut tick = tokio::time::interval(period);
            // Skip the immediate first tick — interval fires once on
            // construction by default, which would surprise the caller.
            tick.set_missed_tick_behavior(
                tokio::time::MissedTickBehavior::Delay,
            );
            tick.tick().await;
            loop {
                tick.tick().await;
                run_lint_once(&lint, &collector, &scope).await;
            }
        })
    }

    fn spawn_cluster(&self) -> tokio::task::JoinHandle<()> {
        let db = self.db.clone();
        let scope = self.cfg.scope.clone();
        let period = self.cfg.dream_cluster_period;
        tokio::spawn(async move {
            let collector = Collector::new(db.clone());
            let cluster = DreamCluster::new(db);
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(
                tokio::time::MissedTickBehavior::Delay,
            );
            tick.tick().await;
            loop {
                tick.tick().await;
                run_cluster_once(&cluster, &collector, &scope).await;
            }
        })
    }

    fn spawn_stale_sweep(&self) -> tokio::task::JoinHandle<()> {
        let workspace = self.cfg.workspace_root.clone();
        let max_age = self.cfg.stale_lock_max_age;
        let period = self.cfg.stale_lock_sweep_period;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(
                tokio::time::MissedTickBehavior::Delay,
            );
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Some(ws) = &workspace {
                    let result =
                        jarvis_orchestrator::workspace::DurableWorkspaceLock::clear_stale_locks(
                            ws, max_age,
                        );
                    match result {
                        Ok(n) if n > 0 => info!(target: "scheduler", "swept {n} stale workspace locks"),
                        Ok(_) => {}
                        Err(e) => warn!(target: "scheduler", "lock sweep error: {e}"),
                    }
                }
            }
        })
    }
}

async fn run_lint_once(lint: &MemoryLint, collector: &Collector, scope: &str) {
    let report = match lint.run(scope) {
        Ok(r) => r,
        Err(e) => {
            warn!(target: "scheduler", "memory lint error: {e}");
            return;
        }
    };
    let payload = json!({
        "scope": scope,
        "duplicates_deprecated": report.duplicates_deprecated,
        "scratch_purged": report.scratch_purged,
        "inferences_expired": report.inferences_expired,
        "weak_lessons_demoted": report.weak_lessons_demoted,
        "conflicts_dampened": report.conflicts_dampened,
    });
    info!(target: "scheduler", "lint {scope}: {payload}");
    let _ = collector.emit(SourceModule::Memory, "memory.lint", None, payload);
}

async fn run_cluster_once(cluster: &DreamCluster, collector: &Collector, scope: &str) {
    let report = match cluster.run(scope) {
        Ok(r) => r,
        Err(e) => {
            warn!(target: "scheduler", "dream cluster error: {e}");
            return;
        }
    };
    let payload = json!({
        "scope": scope,
        "clusters_created": report.clusters_created,
        "members_absorbed": report.members_absorbed,
    });
    info!(target: "scheduler", "cluster {scope}: {payload}");
    let _ = collector.emit(SourceModule::Memory, "memory.dream.cluster", None, payload);
}

/// Convenience handle for callers that want to fire the maintenance
/// passes synchronously (e.g. from a `jarvis maintenance` CLI flag)
/// without standing up the timer-driven Scheduler.
pub struct MaintenanceJobs {
    db: Db,
}

impl MaintenanceJobs {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn run_lint(self: Arc<Self>, scope: &str) -> jarvis_memory::lint::LintReport {
        let lint = MemoryLint::new(self.db.clone());
        lint.run(scope).unwrap_or_default()
    }

    pub async fn run_cluster(
        self: Arc<Self>,
        scope: &str,
    ) -> jarvis_memory::dream::ClusterReport {
        let cluster = DreamCluster::new(self.db.clone());
        cluster.run(scope).unwrap_or_default()
    }
}
