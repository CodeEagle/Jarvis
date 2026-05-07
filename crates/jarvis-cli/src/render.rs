//! Pure formatters for CLI output. Writing strings instead of
//! printing to stdout makes them trivial to assert on in tests.

use chrono::{DateTime, Utc};

pub fn format_raw_event(
    seq: i64,
    ts: DateTime<Utc>,
    event_type: &str,
    raw_content: &str,
) -> String {
    format!(
        "{:>5} {} [{}] {}",
        seq,
        ts.to_rfc3339(),
        event_type,
        truncate(raw_content, 200)
    )
}

pub fn format_audit_line(
    ts: DateTime<Utc>,
    actor: &str,
    action: &str,
    target: Option<&str>,
    status: &str,
    summary: Option<&str>,
) -> String {
    format!(
        "{} [{}] {} {} → {} {}",
        ts.to_rfc3339(),
        actor,
        action,
        target.unwrap_or("-"),
        status,
        summary.unwrap_or("")
    )
}

pub fn format_memory_line(ty: &str, content: &str, trust: f32) -> String {
    format!("[{}] {} trust={:.2}", ty, truncate(content, 200), trust)
}

pub fn format_memory_line_with_id(
    id: &str,
    ty: &str,
    content: &str,
    trust: f32,
) -> String {
    format!(
        "{}  [{}] {} trust={:.2}",
        id,
        ty,
        truncate(content, 200),
        trust
    )
}

pub fn format_dashboard_summary(
    active_sessions: usize,
    raw_events: i64,
    memories: i64,
    pending_outbox: i64,
) -> String {
    format!(
        "active_sessions={active_sessions} raw_events={raw_events} \
memories={memories} pending_outbox={pending_outbox}"
    )
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

pub fn format_lint_summary(
    duplicates_deprecated: u32,
    scratch_purged: u32,
    inferences_expired: u32,
    weak_lessons_demoted: u32,
    conflicts_dampened: u32,
) -> String {
    format!(
        "lint: duplicates_deprecated={duplicates_deprecated} \
scratch_purged={scratch_purged} inferences_expired={inferences_expired} \
weak_lessons={weak_lessons_demoted} conflicts_dampened={conflicts_dampened}"
    )
}
