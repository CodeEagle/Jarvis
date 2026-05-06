//! Server-Sent Events stream of raw_event_log entries.
//!
//! GET /sessions/{id}/stream returns a `text/event-stream` body that
//! tails the immutable raw_event_log for the given session. Each event
//! is rendered as
//!
//! ```text
//! id: <seq>
//! event: <event_type>
//! data: {json blob}
//! ```
//!
//! We poll the database every 500ms — the simplest correct shape that
//! avoids requiring a notify channel from the writers.

use std::convert::Infallible;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, StreamBody};
use hyper::body::Frame;
use hyper::Response;
use jarvis_db::Db;

pub fn open_stream(db: Db, session_id: String) -> Response<BoxBody<Bytes, Infallible>> {
    let state = StreamState {
        db,
        session_id,
        last_seq: 0,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        let rows = jarvis_db::raw_event_log::list_for_session(
            &state.db,
            &state.session_id,
            256,
        )
        .unwrap_or_default();
        let new_rows: Vec<_> = rows
            .into_iter()
            .filter(|r| r.seq > state.last_seq)
            .collect();
        if let Some(last) = new_rows.last() {
            state.last_seq = last.seq;
        }
        if !new_rows.is_empty() {
            let chunk = render_rows(&new_rows);
            return Some((Ok::<_, Infallible>(Frame::data(Bytes::from(chunk))), state));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        Some((
            Ok(Frame::data(Bytes::from(b": keep-alive\n\n".to_vec()))),
            state,
        ))
    });
    let body = StreamBody::new(stream);
    Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .header("x-accel-buffering", "no")
        .body(BoxBody::new(body))
        .unwrap()
}

struct StreamState {
    db: Db,
    session_id: String,
    last_seq: i64,
}

fn render_rows(rows: &[jarvis_db::RawEvent]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!("id: {}\n", row.seq));
        out.push_str(&format!("event: {}\n", row.event_type));
        let data = serde_json::json!({
            "ts": row.ts.to_rfc3339(),
            "agent_id": row.agent_id,
            "trace_id": row.trace_id,
            "raw_content": row.raw_content,
            "safe_content": row.safe_content,
        });
        out.push_str(&format!("data: {}\n\n", data));
    }
    out
}
