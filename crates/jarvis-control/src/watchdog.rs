//! Sub-agent watchdog. Section 9.10.4.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct WatchdogPolicy {
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    pub grace_period: Duration,
}

impl WatchdogPolicy {
    pub const fn defaults() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(20),
            grace_period: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogVerdict {
    Healthy,
    Stale,
    Dead,
}

#[derive(Debug)]
struct AgentState {
    last_heartbeat: Instant,
    stale_since: Option<Instant>,
}

#[derive(Debug)]
pub struct Watchdog {
    policy: WatchdogPolicy,
    agents: Mutex<HashMap<String, AgentState>>,
}

impl Watchdog {
    pub fn new(policy: WatchdogPolicy) -> Self {
        Self {
            policy,
            agents: Mutex::new(HashMap::new()),
        }
    }

    pub fn beat(&self, agent_id: &str) {
        let now = Instant::now();
        let mut map = self.agents.lock().expect("watchdog lock");
        map.insert(
            agent_id.to_string(),
            AgentState {
                last_heartbeat: now,
                stale_since: None,
            },
        );
    }

    /// Evaluate `agent_id` against the policy at the given instant.
    /// Pass `Instant::now()` for live checks; tests pass a synthetic clock.
    pub fn evaluate(&self, agent_id: &str, now: Instant) -> WatchdogVerdict {
        let mut map = self.agents.lock().expect("watchdog lock");
        let Some(state) = map.get_mut(agent_id) else {
            return WatchdogVerdict::Dead; // never beat
        };
        let elapsed = now.saturating_duration_since(state.last_heartbeat);
        if elapsed < self.policy.heartbeat_timeout {
            state.stale_since = None;
            return WatchdogVerdict::Healthy;
        }
        // Mark stale on first transition to compute grace period.
        if state.stale_since.is_none() {
            state.stale_since = Some(now);
        }
        let stale_for = state
            .stale_since
            .map(|t| now.saturating_duration_since(t))
            .unwrap_or_default();
        if stale_for < self.policy.grace_period {
            WatchdogVerdict::Stale
        } else {
            WatchdogVerdict::Dead
        }
    }

    pub fn forget_agent(&self, agent_id: &str) {
        self.agents
            .lock()
            .expect("watchdog lock")
            .remove(agent_id);
    }

    pub fn forget(&self, agent_id: &str) {
        self.forget_agent(agent_id);
    }

    /// Sweep all currently-tracked agents at `now` and return the
    /// list of agents whose verdict is `Stale` or `Dead`. The
    /// verdict map mutates internal state (stale_since), so calling
    /// this is the canonical "tick" the scheduler should run.
    pub fn into_heartbeat(self: Arc<Self>) -> WatchdogHeartbeat {
        WatchdogHeartbeat { inner: self }
    }

    pub fn sweep(&self, now: Instant) -> Vec<(String, WatchdogVerdict)> {
        let ids: Vec<String> = self
            .agents
            .lock()
            .expect("watchdog lock")
            .keys()
            .cloned()
            .collect();
        let mut out = Vec::new();
        for id in ids {
            let v = self.evaluate(&id, now);
            if !matches!(v, WatchdogVerdict::Healthy) {
                out.push((id, v));
            }
        }
        out
    }
}

/// Adapter implementing `jarvis_orchestrator::dispatch::Heartbeat` so
/// the orchestrator dispatcher can pulse this Watchdog without taking
/// a hard dependency on the Watchdog type.
#[derive(Clone)]
pub struct WatchdogHeartbeat {
    inner: Arc<Watchdog>,
}

impl jarvis_orchestrator::dispatch::Heartbeat for WatchdogHeartbeat {
    fn beat(&self, agent_id: &str) {
        self.inner.beat(agent_id);
    }
    fn forget(&self, agent_id: &str) {
        self.inner.forget_agent(agent_id);
    }
}
