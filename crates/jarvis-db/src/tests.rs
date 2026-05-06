#[cfg(test)]
use crate::*;
#[cfg(test)]
use jarvis_core::memory::*;
#[cfg(test)]
use jarvis_core::session::*;
#[cfg(test)]
use jarvis_core::time::now;

fn fresh_db() -> Db {
    Db::in_memory().expect("open in-memory db")
}

fn sample_session(id: &str) -> Session {
    let n = now();
    Session {
        id: id.to_string(),
        title: "test".into(),
        domain: "coding".into(),
        topic: "test".into(),
        summary: String::new(),
        long_summary: String::new(),
        active_entities: vec![],
        unresolved: vec![],
        resolved: vec![],
        recent_message_ids: vec![],
        memory_refs: vec![],
        skill_refs: vec![],
        status: SessionStatus::Active,
        created_at: n,
        updated_at: n,
        last_active_at: n,
    }
}

fn sample_memory(id: &str, ty: MemoryType) -> Memory {
    let n = now();
    Memory {
        id: id.to_string(),
        r#type: ty,
        scope: "global".into(),
        content: format!("memory {}", id),
        entities: vec![],
        confidence: SourceType::UserExplicit.default_confidence(),
        trust_score: 0.95,
        half_life_days: ty.half_life_days(),
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::UserExplicit,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 0.0,
        emotion_polarity: EmotionPolarity::Neutral,
        tier: 1,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: n,
        updated_at: n,
    }
}

// ── raw_event_log immutability (Section 30.13) ──────────────────────────

#[test]
fn raw_event_seq_is_monotonic() {
    let db = fresh_db();
    let s1 = raw_event_log::append(
        &db,
        raw_event_log::AppendEvent {
            event_type: RawEventKind::UserMessage,
            session_id: Some("sess_1"),
            trace_id: None,
            agent_id: None,
            raw_content: "hello",
            safe_content: None,
        },
    )
    .unwrap();
    let s2 = raw_event_log::append(
        &db,
        raw_event_log::AppendEvent {
            event_type: RawEventKind::AgentMessage,
            session_id: Some("sess_1"),
            trace_id: None,
            agent_id: None,
            raw_content: "hi",
            safe_content: None,
        },
    )
    .unwrap();
    assert!(s2 > s1);
}

#[test]
fn raw_event_log_blocks_update() {
    let db = fresh_db();
    let seq = raw_event_log::append(
        &db,
        raw_event_log::AppendEvent {
            event_type: RawEventKind::UserMessage,
            session_id: None,
            trace_id: None,
            agent_id: None,
            raw_content: "original",
            safe_content: None,
        },
    )
    .unwrap();
    let err = db
        .with(|conn| {
            conn.execute(
                "UPDATE raw_event_log SET raw_content = 'tampered' WHERE seq = ?1",
                rusqlite::params![seq],
            )
            .map_err(error::DbError::Sqlite)?;
            Ok(())
        })
        .unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("immutable"),
        "expected immutability violation, got: {msg}"
    );
}

#[test]
fn raw_event_log_blocks_delete() {
    let db = fresh_db();
    let seq = raw_event_log::append(
        &db,
        raw_event_log::AppendEvent {
            event_type: RawEventKind::UserMessage,
            session_id: None,
            trace_id: None,
            agent_id: None,
            raw_content: "x",
            safe_content: None,
        },
    )
    .unwrap();
    let err = db
        .with(|conn| {
            conn.execute(
                "DELETE FROM raw_event_log WHERE seq = ?1",
                rusqlite::params![seq],
            )
            .map_err(error::DbError::Sqlite)?;
            Ok(())
        })
        .unwrap_err();
    assert!(format!("{}", err).contains("immutable"));
}

#[test]
fn raw_event_checksum_verifies() {
    let db = fresh_db();
    let seq = raw_event_log::append(
        &db,
        raw_event_log::AppendEvent {
            event_type: RawEventKind::UserMessage,
            session_id: None,
            trace_id: None,
            agent_id: None,
            raw_content: "hello",
            safe_content: None,
        },
    )
    .unwrap();
    let event = raw_event_log::get(&db, seq).unwrap();
    assert!(event.verify(), "checksum should match stored content");
}

#[test]
fn list_for_session_returns_ordered_rows() {
    let db = fresh_db();
    for i in 0..3 {
        raw_event_log::append(
            &db,
            raw_event_log::AppendEvent {
                event_type: RawEventKind::UserMessage,
                session_id: Some("sess_a"),
                trace_id: None,
                agent_id: None,
                raw_content: &format!("msg-{i}"),
                safe_content: None,
            },
        )
        .unwrap();
    }
    let rows = raw_event_log::list_for_session(&db, "sess_a", 10).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].raw_content, "msg-0");
    assert_eq!(rows[2].raw_content, "msg-2");
    assert!(rows[2].seq > rows[0].seq);
}

// ── memory + change-log atomicity ───────────────────────────────────────

#[test]
fn memory_write_records_change_log() {
    let db = fresh_db();
    let m = sample_memory("m1", MemoryType::PreferenceMemory);
    memory_repo::upsert(
        &db,
        &m,
        memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: Some("test"),
            reason: Some("unit test"),
        },
    )
    .unwrap();
    let history = memory_change_log::history_for(&db, "m1").unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].change_type, "created");
    assert!(history[0].before_json.is_none());
    assert!(history[0].after_json.is_some());
}

#[test]
fn memory_update_records_before_and_after() {
    let db = fresh_db();
    let mut m = sample_memory("m1", MemoryType::PreferenceMemory);
    memory_repo::upsert(
        &db,
        &m,
        memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();

    m.trust_score = 0.5;
    m.updated_at = now();
    memory_repo::upsert(
        &db,
        &m,
        memory_repo::WriteOpts {
            change_type: MemoryChangeType::TrustScoreUpdated,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();

    let history = memory_change_log::history_for(&db, "m1").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].change_type, "trust_score_updated");
    assert!(history[1].before_json.is_some());
    assert!(history[1].after_json.is_some());
}

#[test]
fn fact_memory_emotion_is_forced_neutral_on_write() {
    let db = fresh_db();
    let mut m = sample_memory("m1", MemoryType::FactMemory);
    m.emotion_energy = 8.0;
    m.emotion_polarity = EmotionPolarity::Positive;
    memory_repo::upsert(
        &db,
        &m,
        memory_repo::WriteOpts {
            change_type: MemoryChangeType::Created,
            source_module: None,
            reason: None,
        },
    )
    .unwrap();
    let stored = memory_repo::get(&db, "m1").unwrap().unwrap();
    assert_eq!(stored.emotion_energy, 0.0);
    assert_eq!(stored.emotion_polarity, EmotionPolarity::Neutral);
}

// ── session + messages ──────────────────────────────────────────────────

#[test]
fn session_upsert_and_get() {
    let db = fresh_db();
    let sess = sample_session("sess_1");
    session_repo::upsert_session(&db, &sess).unwrap();
    let read = session_repo::get_session(&db, "sess_1").unwrap().unwrap();
    assert_eq!(read.id, "sess_1");
    assert_eq!(read.domain, "coding");
}

#[test]
fn append_message_touches_session_last_active() {
    let db = fresh_db();
    let sess = sample_session("sess_1");
    session_repo::upsert_session(&db, &sess).unwrap();

    let msg = jarvis_core::session::Message {
        id: "msg_1".into(),
        session_id: "sess_1".into(),
        trace_id: None,
        role: jarvis_core::session::MessageRole::User,
        content: "hi".into(),
        token_count: 1,
        summary_id: None,
        created_at: now(),
    };
    session_repo::append_message(&db, &msg).unwrap();

    let recent = session_repo::recent_messages(&db, "sess_1", 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].content, "hi");
}

#[test]
fn list_recent_returns_only_active_sessions() {
    let db = fresh_db();
    let mut a = sample_session("sess_a");
    let mut b = sample_session("sess_b");
    b.status = SessionStatus::Archived;
    session_repo::upsert_session(&db, &a).unwrap();
    session_repo::upsert_session(&db, &b).unwrap();
    a.title = "updated".into();
    session_repo::upsert_session(&db, &a).unwrap();

    let recent = session_repo::list_recent(&db, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, "sess_a");
}
