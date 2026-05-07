//! Worker-process SubAgentDriver (Section 9.4).
//!
//! Spawns the configured command as a child process with the
//! SubTaskEnvelope serialised to stdin (JSON line-delimited). The
//! driver expects the child to emit a SubTaskResult JSON object on
//! stdout. Stderr is captured and surfaced through `escalation` when
//! the child exits non-zero or returns malformed JSON.
//!
//! This is intentionally driver-side glue, not a sandbox. Sandboxing
//! is an environment concern (firejail / nsjail / containers) and
//! belongs upstream of `WorkerProcessDriver::new`.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::dispatch::{failed_result, SubAgentDriver};
use crate::sub_task::{Escalation, SubTaskEnvelope, SubTaskResult, SubTaskStatus};

#[derive(Debug, Clone)]
pub struct WorkerCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Soft per-call timeout; the driver kills the child when exceeded.
    pub timeout: Duration,
    /// Working directory for the child.
    pub cwd: Option<std::path::PathBuf>,
    /// Extra env vars (k=v) — leaks of the parent env are intentional;
    /// scrubbing is the orchestrator's job, not the driver's.
    pub env: Vec<(String, String)>,
}

impl WorkerCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: vec![],
            timeout: Duration::from_secs(60),
            cwd: None,
            env: vec![],
        }
    }
    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
    pub fn cwd(mut self, p: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }
    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }
}

pub struct WorkerProcessDriver {
    name: &'static str,
    command: WorkerCommand,
}

impl WorkerProcessDriver {
    pub fn new(name: &'static str, command: WorkerCommand) -> Self {
        Self { name, command }
    }
}

#[async_trait]
impl SubAgentDriver for WorkerProcessDriver {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn execute(&self, envelope: SubTaskEnvelope) -> SubTaskResult {
        let sub_task_id = envelope.sub_task_id.clone();
        let envelope_json = match serde_json::to_string(&envelope) {
            Ok(s) => s,
            Err(e) => {
                return failed_result(
                    &sub_task_id,
                    &format!("envelope serialization failed: {e}"),
                )
            }
        };

        let mut cmd = Command::new(&self.command.program);
        cmd.args(&self.command.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = &self.command.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &self.command.env {
            cmd.env(k, v);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return failed_result(&sub_task_id, &format!("spawn failed: {e}"));
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(envelope_json.as_bytes()).await {
                return failed_result(&sub_task_id, &format!("stdin write failed: {e}"));
            }
            // Tell the child we're done sending.
            drop(stdin);
        }

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Drain stdout and stderr on dedicated tasks so neither pipe
        // backs up the child while we're waiting on the other.
        // Tasks naturally complete when their pipe hits EOF after the
        // child exits (or after we kill it on timeout).
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut last = None;
            while let Ok(Some(l)) = reader.next_line().await {
                last = Some(l);
            }
            last
        });
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut buf = String::new();
            while let Ok(Some(l)) = reader.next_line().await {
                buf.push_str(&l);
                buf.push('\n');
            }
            buf
        });

        let status = match timeout(self.command.timeout, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return failed_result(&sub_task_id, &format!("child wait failed: {e}"));
            }
            Err(_) => {
                // Timeout: kill the child. We can't await the drain
                // tasks here — child.kill() only signals the direct
                // child (e.g. /bin/sh), and any grandchild it forked
                // (e.g. `sleep 60`) keeps the pipes open and would
                // hang the drain. Abort instead and report the
                // partial stderr that's already been read.
                let _ = child.kill().await;
                stdout_task.abort();
                stderr_task.abort();
                return SubTaskResult {
                    sub_task_id,
                    status: SubTaskStatus::Failed,
                    summary: "worker process exceeded timeout".into(),
                    artifact_ids: vec![],
                    escalation: Some(Escalation {
                        reason: "timeout".into(),
                        options: vec![],
                    }),
                    token_used: 0,
                    tool_calls_count: 0,
                    completed_at: Utc::now(),
                };
            }
        };

        // Child exited; both pipes hit EOF, so the tasks complete
        // naturally. Awaiting them guarantees stderr is fully drained
        // before we inspect it (eliminates the previous select-loop
        // race where an early stdout-EOF could exit before stderr was
        // read).
        let last_line = stdout_task.await.unwrap_or(None);
        let stderr_buf = stderr_task.await.unwrap_or_default();

        if !status.success() {
            return SubTaskResult {
                sub_task_id,
                status: SubTaskStatus::Failed,
                summary: format!("worker exited with {status}"),
                artifact_ids: vec![],
                escalation: Some(Escalation {
                    reason: stderr_buf.trim().to_string(),
                    options: vec![],
                }),
                token_used: 0,
                tool_calls_count: 0,
                completed_at: Utc::now(),
            };
        }

        match last_line {
            None => failed_result(&sub_task_id, "worker produced no result line"),
            Some(line) => match serde_json::from_str::<SubTaskResult>(&line) {
                Ok(mut result) => {
                    // Trust the worker's payload but ensure the
                    // sub_task_id stays correct end-to-end.
                    result.sub_task_id = sub_task_id;
                    result
                }
                Err(e) => failed_result(
                    &sub_task_id,
                    &format!("worker JSON parse error: {e}; raw: {line}"),
                ),
            },
        }
    }
}
