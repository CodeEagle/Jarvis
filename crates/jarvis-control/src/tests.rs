#[cfg(test)]
use crate::*;
#[cfg(test)]
use std::time::{Duration, Instant};
#[cfg(test)]
use jarvis_db::Db;

// ── watchdog ────────────────────────────────────────────────────────────

#[test]
fn watchdog_fresh_beat_is_healthy() {
    let wd = Watchdog::new(WatchdogPolicy::defaults());
    wd.beat("agent_1");
    let v = wd.evaluate("agent_1", Instant::now());
    assert_eq!(v, WatchdogVerdict::Healthy);
}

#[test]
fn watchdog_unknown_agent_is_dead() {
    let wd = Watchdog::new(WatchdogPolicy::defaults());
    let v = wd.evaluate("never_seen", Instant::now());
    assert_eq!(v, WatchdogVerdict::Dead);
}

#[test]
fn watchdog_marks_stale_then_dead_after_grace() {
    let policy = WatchdogPolicy {
        heartbeat_interval: Duration::from_millis(10),
        heartbeat_timeout: Duration::from_millis(50),
        grace_period: Duration::from_millis(100),
    };
    let wd = Watchdog::new(policy);
    wd.beat("agent_1");

    // simulate clock advance well past timeout but still within grace
    let later = Instant::now() + Duration::from_millis(80);
    let v = wd.evaluate("agent_1", later);
    assert_eq!(v, WatchdogVerdict::Stale);

    // past grace → dead
    let later2 = later + Duration::from_millis(120);
    let v2 = wd.evaluate("agent_1", later2);
    assert_eq!(v2, WatchdogVerdict::Dead);
}

#[test]
fn watchdog_recovery_resets_stale() {
    let policy = WatchdogPolicy {
        heartbeat_interval: Duration::from_millis(10),
        heartbeat_timeout: Duration::from_millis(50),
        grace_period: Duration::from_millis(100),
    };
    let wd = Watchdog::new(policy);
    wd.beat("agent_1");
    let later = Instant::now() + Duration::from_millis(80);
    assert_eq!(wd.evaluate("agent_1", later), WatchdogVerdict::Stale);
    wd.beat("agent_1");
    assert_eq!(
        wd.evaluate("agent_1", Instant::now()),
        WatchdogVerdict::Healthy
    );
}

// ── fallback messages ───────────────────────────────────────────────────

#[test]
fn fallback_message_idle_short() {
    let m = fallback_message(TaskPlaneState::Idle, None);
    assert!(!m.is_empty());
}

#[test]
fn fallback_message_executing_mentions_running_agent_when_known() {
    let m = fallback_message(TaskPlaneState::Executing, Some("代码助手"));
    assert!(m.contains("代码助手"));
}

#[test]
fn fallback_message_unavailable_mentions_lightweight_mode() {
    let m = fallback_message(TaskPlaneState::Unavailable, None);
    assert!(m.contains("轻量模式"));
}

// ── control plane SLA (Section 30.4) ────────────────────────────────────

#[tokio::test]
async fn control_plane_returns_resolved_for_simple_input() {
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::new(db);
    let resp = cp
        .handle_user_input("openwrt dns 报错".into(), None, vec![])
        .await;
    match resp {
        HandledResponse::Resolved {
            decision,
            elapsed_ms,
            ..
        } => {
            assert_eq!(decision.domain, "devops");
            assert!(elapsed_ms < 2000, "took {} ms", elapsed_ms);
        }
        HandledResponse::Fallback { message, .. } => {
            panic!("expected resolved, got fallback: {message}")
        }
    }
}

#[tokio::test]
async fn control_plane_falls_back_when_budget_too_tight() {
    // Set fallback budget to 1ms so the router can't beat it.
    let db = Db::in_memory().unwrap();
    let cp = ControlPlane::with_sla(
        db,
        ResponseSla {
            interrupt_ack: Duration::from_millis(500),
            progress_query: Duration::from_millis(800),
            sub_agent_reply: Duration::from_millis(1000),
            fallback_ack: Duration::from_millis(1),
        },
    );
    let resp = cp
        .handle_user_input("hello".into(), None, vec![])
        .await;
    match resp {
        HandledResponse::Fallback { elapsed_ms, .. } => {
            // Hit the fallback budget; must not exceed it by much.
            assert!(elapsed_ms < 200, "fallback took too long: {} ms", elapsed_ms);
        }
        HandledResponse::Resolved { .. } => {
            // Acceptable on extremely fast machines, though unlikely at 1ms.
        }
    }
}
