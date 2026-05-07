//! Outbox replication daemon (Section 24.5).
//!
//! Drains `outbox` rows in seq order, ships each one to a configured
//! peer's `/replicate` endpoint, and marks `delivered_at` on success.
//! Failures back off and retry on the next tick. The protocol is a
//! plain HTTP POST with a JSON body; no signing yet — that's a
//! deployment-level concern that lives outside this loop.

use std::time::Duration;

use jarvis_db::{outbox, Db};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    pub peer_endpoint: String,
    pub auth_token: Option<String>,
    pub poll_interval: Duration,
    pub batch_size: usize,
    pub request_timeout: Duration,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            peer_endpoint: String::new(),
            auth_token: None,
            poll_interval: Duration::from_secs(10),
            batch_size: 32,
            request_timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicationEnvelope {
    pub seq: i64,
    pub kind: String,
    pub target_table: String,
    pub target_id: String,
    pub payload_json: serde_json::Value,
    pub ts: String,
}

pub struct Replicator {
    db: Db,
    cfg: ReplicationConfig,
    client: reqwest::Client,
}

impl Replicator {
    pub fn new(db: Db, cfg: ReplicationConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(cfg.request_timeout)
            .build()
            .expect("reqwest client");
        Self { db, cfg, client }
    }

    /// Drain at most `batch_size` rows once. Returns the number of
    /// rows successfully delivered. Useful for tests and one-shot
    /// drains.
    pub async fn drain_once(&self) -> jarvis_db::error::DbResult<usize> {
        if self.cfg.peer_endpoint.is_empty() {
            return Ok(0);
        }
        let rows = outbox::take_pending(&self.db, self.cfg.batch_size)?;
        let mut delivered = 0;
        for row in rows {
            let envelope = ReplicationEnvelope {
                seq: row.seq,
                kind: row.kind,
                target_table: row.target_table,
                target_id: row.target_id,
                payload_json: serde_json::from_str(&row.payload_json)
                    .unwrap_or(serde_json::Value::Null),
                ts: row.ts.to_rfc3339(),
            };
            let mut req = self.client.post(&self.cfg.peer_endpoint).json(&envelope);
            if let Some(token) = &self.cfg.auth_token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Err(e) = outbox::mark_delivered(&self.db, row.seq) {
                        warn!(target: "replicator", "mark_delivered failed for seq={}: {e}", row.seq);
                    } else {
                        delivered += 1;
                    }
                }
                Ok(resp) => {
                    warn!(target: "replicator", "peer rejected seq={} status={}", row.seq, resp.status());
                    break; // back off — try again next tick
                }
                Err(e) => {
                    warn!(target: "replicator", "transport error seq={}: {e}", row.seq);
                    break;
                }
            }
        }
        Ok(delivered)
    }

    /// Spawn the periodic replication loop. The handle can be aborted
    /// to stop the daemon.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let cfg = self.cfg.clone();
        let db = self.db.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(cfg.poll_interval);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await;
            loop {
                tick.tick().await;
                let r = Replicator {
                    db: db.clone(),
                    cfg: cfg.clone(),
                    client: client.clone(),
                };
                match r.drain_once().await {
                    Ok(0) => {}
                    Ok(n) => info!(target: "replicator", "delivered {n} outbox rows"),
                    Err(e) => warn!(target: "replicator", "drain error: {e}"),
                }
            }
        })
    }
}
