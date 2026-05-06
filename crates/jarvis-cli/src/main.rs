//! Jarvis CLI.
//!
//! Subcommands:
//!   route <input>       - run the Router and print the RouteDecision
//!   chat                - interactive REPL going through the Control Plane
//!   memory write <text> - record a user-explicit memory
//!   memory list         - list memories in scope `global`
//!   raw-log <session>   - dump raw_event_log entries for a session
//!
//! Storage defaults to `./jarvis.db`; override with `JARVIS_DB`.

use std::env;
use std::io::{self, BufRead, Write};

use jarvis_control::ControlPlane;
use jarvis_core::memory::{EmotionPolarity, MemoryType, SourceType};
use jarvis_db::Db;
use jarvis_growth::{ArtifactStatus, ArtifactType, Collector};
use jarvis_memory::{
    manager::{MemoryManager, WriteRequest},
    Retrieval,
};
use jarvis_router::{Router, RouterInput};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = jarvis_control::init_tracing();

    let args: Vec<String> = env::args().collect();
    let db_path = env::var("JARVIS_DB").unwrap_or_else(|_| "jarvis.db".into());
    let db = Db::open(&db_path)?;

    match args.get(1).map(String::as_str) {
        Some("route") => {
            let input = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "帮我看个 OpenWrt 的报错".into());
            let r = Router::new(db);
            let (decision, diag) = r.route(RouterInput {
                user_input: &input,
                session_id_hint: None,
                running_agent_types: &[],
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&decision).unwrap_or_default()
            );
            eprintln!("\n— diagnostics —");
            eprintln!("rule_hints: {:?}", diag.rule_hints);
            eprintln!("mention:    {:?}", diag.mention.mention_mode);
            eprintln!("raw_seq:    {}", diag.raw_event_seq);
            if let Some(w) = diag.mention_warning {
                eprintln!("warning:    {w}");
            }
        }
        Some("chat") => chat_repl(db).await?,
        Some("memory") => memory_command(db, &args[2..])?,
        Some("raw-log") => raw_log_command(db, &args[2..])?,
        Some("growth") => growth_command(db, &args[2..])?,
        Some("trace") => trace_command(db, &args[2..])?,
        Some("replay") => replay_command(db, &args[2..])?,
        Some("audit") => audit_command(db, &args[2..])?,
        Some("trace-view") => trace_view_command(db, &args[2..])?,
        Some("memory-history") => memory_history_command(db, &args[2..])?,
        Some("serve") => serve_command(db, &args[2..]).await?,
        Some("maintenance") => maintenance_command(db, &args[2..]).await?,
        Some("demo") => demo_command(db).await?,
        _ => {
            print_usage();
        }
    }
    Ok(())
}

async fn chat_repl(db: Db) -> anyhow::Result<()> {
    let cp = ControlPlane::new(db);
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    println!("Jarvis v0.1 — chat REPL. Type :quit to exit.");

    loop {
        print!("> ");
        stdout.flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == ":quit" {
            break;
        }
        let resp = cp
            .handle_user_input(line.to_string(), None, vec![])
            .await;
        match resp {
            jarvis_control::HandledResponse::Resolved {
                decision,
                diagnostics_summary,
                kind,
                elapsed_ms,
            } => {
                println!(
                    "[{:?} · {}ms] {} → agent={} confidence={:.2}",
                    kind, elapsed_ms, decision.primary_intent, decision.agent_type, decision.confidence
                );
                if decision.clarification_needed {
                    println!(
                        "  ↳ clarification needed: {}",
                        decision.router_notes
                    );
                }
                if decision.mention_override {
                    println!("  ↳ user @ specified {}", decision.agent_type);
                }
                eprintln!("  diag: {diagnostics_summary}");
            }
            jarvis_control::HandledResponse::Fallback {
                message,
                elapsed_ms,
            } => {
                println!("[fallback · {elapsed_ms}ms] {message}");
            }
        }
    }
    Ok(())
}

fn memory_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("write") => {
            let content = args
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            anyhow::ensure!(!content.is_empty(), "memory write requires content");
            let mgr = MemoryManager::new(db);
            let outcome = mgr.write(WriteRequest {
                r#type: MemoryType::PreferenceMemory,
                scope: "global",
                content: &content,
                entities: vec![],
                source_type: SourceType::UserExplicit,
                source_trace_id: None,
                tier: 1,
                emotion_energy: 0.0,
                emotion_polarity: EmotionPolarity::Neutral,
                reason: Some("CLI"),
            })?;
            println!(
                "wrote {} status={:?} trust={:.2}",
                outcome.id, outcome.status, outcome.trust_score
            );
        }
        Some("list") => {
            let r = Retrieval::new(db);
            let results = r.retrieve("global", "", None, 50, 0.0)?;
            for hit in results {
                println!(
                    "[{}] {} trust={:.2}",
                    hit.memory.r#type.as_str(),
                    hit.memory.content,
                    hit.trust_now
                );
            }
        }
        _ => {
            println!("Usage: memory write <text> | memory list");
        }
    }
    Ok(())
}

fn raw_log_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let session = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!session.is_empty(), "raw-log requires <session_id>");
    let rows = jarvis_db::raw_event_log::list_for_session(&db, &session, 100)?;
    for row in rows {
        println!(
            "{:>5} {} [{}] {}",
            row.seq,
            row.ts.to_rfc3339(),
            row.event_type,
            row.raw_content
        );
    }
    Ok(())
}

fn growth_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let collector = Collector::new(db);
    match args.first().map(String::as_str) {
        Some("events") => {
            let event_type = args.get(1).cloned().unwrap_or_else(|| "route_decision".into());
            let rows = collector.list_events_for_event_type(&event_type, 50)?;
            for row in rows {
                println!(
                    "[{}] {} {} {}",
                    row.ts.to_rfc3339(),
                    row.source_module.as_str(),
                    row.event_type,
                    row.payload_json
                );
            }
        }
        Some("artifacts") => {
            let arts = collector.list_artifacts(None, None)?;
            for a in arts {
                println!(
                    "{}  type={}  status={}  v{}  conf={:.2}",
                    a.id,
                    a.r#type.as_str(),
                    a.status.as_str(),
                    a.version,
                    a.confidence
                );
            }
        }
        Some("filter") => {
            let by_type = args.get(1).and_then(|s| ArtifactType::parse(s));
            let by_status = args.get(2).and_then(|s| match s.as_str() {
                "promoted" => Some(ArtifactStatus::Promoted),
                "candidate" => Some(ArtifactStatus::Candidate),
                "rejected" => Some(ArtifactStatus::Rejected),
                _ => None,
            });
            let arts = collector.list_artifacts(by_type, by_status)?;
            for a in arts {
                println!("{} {}", a.r#type.as_str(), a.id);
            }
        }
        _ => {
            println!(
                "Usage: growth events [event_type] | growth artifacts | growth filter <type> [status]"
            );
        }
    }
    Ok(())
}

fn trace_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let trace_id = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!trace_id.is_empty(), "trace requires <trace_id>");
    let events = jarvis_db::provenance::trace_events(&db, &trace_id)?;
    for e in events {
        println!(
            "{:>5} {} [{}] {}",
            e.seq,
            e.ts.to_rfc3339(),
            e.event_type,
            e.raw_content
        );
    }
    Ok(())
}

fn replay_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let session_id = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!session_id.is_empty(), "replay requires <session_id>");
    let at = match args.get(1) {
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        None => chrono::Utc::now(),
    };
    let window = jarvis_db::provenance::replay_session_at(&db, &session_id, at)?;
    if let Some(b) = window.baseline {
        println!(
            "baseline seq={} reason={} captured_at={}",
            b.seq,
            b.snapshot_reason,
            b.created_at.to_rfc3339()
        );
    } else {
        println!("baseline: (none)");
    }
    for e in window.events {
        println!(
            "{:>5} {} [{}] {}",
            e.seq,
            e.ts.to_rfc3339(),
            e.event_type,
            e.raw_content
        );
    }
    Ok(())
}

fn audit_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let session = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!session.is_empty(), "audit requires <session_id>");
    let entries = jarvis_db::audit_log::list_for_session(&db, &session, 100)?;
    for e in entries {
        println!(
            "{} [{}] {} {} → {} {}",
            e.ts.to_rfc3339(),
            e.actor,
            e.action,
            e.target.as_deref().unwrap_or("-"),
            e.status.as_str(),
            e.output_summary.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

fn trace_view_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let trace_id = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!trace_id.is_empty(), "trace-view requires <trace_id>");
    let events = jarvis_db::provenance::trace_events(&db, &trace_id)?;
    println!("─── trace {trace_id} ─── {} events ───", events.len());
    for e in &events {
        let safe = e.safe_content.as_deref().unwrap_or(&e.raw_content);
        let agent = e.agent_id.as_deref().unwrap_or("-");
        println!(
            "  [{}] {:>5} {} agent={agent} session={:?}\n    {}",
            e.ts.format("%H:%M:%S%.3f"),
            e.seq,
            e.event_type,
            e.session_id,
            safe.chars().take(180).collect::<String>(),
        );
    }
    let session_id = events.first().and_then(|e| e.session_id.clone());
    if let Some(sid) = session_id {
        let audit = jarvis_db::audit_log::list_for_session(&db, &sid, 50)?;
        let related: Vec<_> = audit
            .into_iter()
            .filter(|a| a.trace_id.as_deref() == Some(trace_id.as_str()))
            .collect();
        if !related.is_empty() {
            println!("\n─── audit ── {} entries ───", related.len());
            for a in related {
                println!(
                    "  [{}] {} {} → {} {}",
                    a.ts.format("%H:%M:%S%.3f"),
                    a.actor,
                    a.target.as_deref().unwrap_or("-"),
                    a.status.as_str(),
                    a.output_summary.as_deref().unwrap_or(""),
                );
            }
        }
    }
    Ok(())
}

fn memory_history_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let memory_id = args.first().cloned().unwrap_or_default();
    anyhow::ensure!(!memory_id.is_empty(), "memory-history requires <memory_id>");
    let history = jarvis_db::provenance::memory_history(&db, &memory_id)?;
    println!("─── memory {memory_id} ─── {} entries ───", history.len());
    for h in history {
        println!(
            "  [{}] {} module={} reason={:?}",
            h.ts.format("%Y-%m-%d %H:%M:%S"),
            h.change_type,
            h.source_module.as_deref().unwrap_or("-"),
            h.reason.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

async fn demo_command(db: Db) -> anyhow::Result<()> {
    use jarvis_orchestrator as orch;
    let session_id = "demo_session";
    let n = jarvis_core::time::now();
    jarvis_db::session_repo::upsert_session(
        &db,
        &jarvis_core::session::Session {
            id: session_id.into(),
            title: "Demo".into(),
            domain: "coding".into(),
            topic: "demo".into(),
            summary: "demo session".into(),
            long_summary: "demo run".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: jarvis_core::session::SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )?;

    println!("[demo] 1. Writing a user-explicit memory…");
    let mgr = jarvis_memory::manager::MemoryManager::new(db.clone());
    mgr.write(jarvis_memory::manager::WriteRequest {
        r#type: jarvis_core::memory::MemoryType::PreferenceMemory,
        scope: "global",
        content: "用户偏好函数式编程",
        entities: vec!["函数式".into()],
        source_type: jarvis_core::memory::SourceType::UserExplicit,
        source_trace_id: None,
        tier: 1,
        emotion_energy: 0.0,
        emotion_polarity: jarvis_core::memory::EmotionPolarity::Neutral,
        reason: Some("demo"),
    })?;

    println!("[demo] 2. Routing input through Router…");
    let router = jarvis_router::Router::new(db.clone());
    let (decision, _) = router.route(jarvis_router::RouterInput {
        user_input: "openwrt dns 报错",
        session_id_hint: Some(session_id),
        running_agent_types: &[],
    })?;
    println!(
        "         agent={} confidence={:.2}",
        decision.agent_type, decision.confidence
    );

    println!("[demo] 3. Dispatching an in-process sub-task…");
    let driver: orch::DriverHandle = std::sync::Arc::new(orch::InProcessDriver::new(
        "demo",
        |env| jarvis_orchestrator::sub_task::SubTaskResult {
            sub_task_id: env.sub_task_id,
            status: jarvis_orchestrator::sub_task::SubTaskStatus::Success,
            summary: "demo done".into(),
            artifact_ids: vec![],
            escalation: None,
            token_used: 1,
            tool_calls_count: 0,
            completed_at: chrono::Utc::now(),
        },
    ));
    let envelope = jarvis_orchestrator::sub_task::SubTaskEnvelope {
        sub_task_id: jarvis_core::ids::new_id_with_prefix("st"),
        parent_task_id: "task_demo".into(),
        trace_id: decision.trace_id.clone(),
        title: "demo".into(),
        instruction: "say hi".into(),
        depends_on_results: vec![],
        input_artifact_refs: vec![],
        tool_scope: jarvis_core::tool::ToolScope::empty(),
        output_spec: jarvis_orchestrator::sub_task::OutputSpec {
            format: "text".into(),
            max_tokens: 100,
        },
        constraints: jarvis_orchestrator::sub_task::SubTaskConstraints {
            max_tool_calls: 0,
            max_file_reads: 0,
            token_budget: 100,
            timeout_ms: 1000,
        },
        tentacle_path: None,
    };
    let pipeline = orch::OrchestrationPipeline::new(db.clone());
    let builder: orch::WalkthroughBuilder = std::sync::Arc::new(|res| {
        let mut doc = orch::walkthrough::new_draft(
            &res.sub_task_id,
            "_set_by_pipeline",
            "coding",
            "demo walkthrough",
        );
        doc.verification_status = orch::walkthrough::VerificationStatus::Verified;
        doc.sections = vec![orch::walkthrough::new_section(
            orch::walkthrough::SectionType::Summary,
            "summary",
            &res.summary,
        )];
        doc
    });
    let outcome = pipeline
        .run_sub_task(session_id, "coding", envelope, driver, Some(builder))
        .await?;
    println!(
        "         sub_task={:?} walkthrough_doc={:?} decision={:?}",
        outcome.sub_task_result.status,
        outcome.walkthrough_doc_id,
        outcome.auto_decision,
    );

    println!("[demo] 4. Running Dream lint + cluster…");
    let jobs = std::sync::Arc::new(jarvis_control::MaintenanceJobs::new(db.clone()));
    let lint = jobs.clone().run_lint("global").await;
    println!(
        "         lint duplicates={} scratch={} weak_lessons={}",
        lint.duplicates_deprecated, lint.scratch_purged, lint.weak_lessons_demoted
    );

    println!("[demo] 5. Dumping dashboard metrics…");
    println!(
        "         active_sessions={} raw_events={} memories={} pending_outbox={}",
        jarvis_db::session_repo::list_recent(&db, 100)?.len(),
        jarvis_db::raw_event_log::count(&db)?,
        jarvis_db::memory_repo::count(&db)?,
        jarvis_db::outbox::pending_count(&db)?,
    );

    println!("[demo] all good. ✨");
    Ok(())
}

async fn serve_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let addr_str = args.first().cloned().unwrap_or_else(|| "127.0.0.1:7777".into());
    let addr: std::net::SocketAddr = addr_str.parse()?;
    let state = jarvis_api::ApiState { db };
    jarvis_api::serve(state, addr).await
}

async fn maintenance_command(db: Db, args: &[String]) -> anyhow::Result<()> {
    let scope = args.first().cloned().unwrap_or_else(|| "global".into());
    let jobs = std::sync::Arc::new(jarvis_control::MaintenanceJobs::new(db));
    let lint = jobs.clone().run_lint(&scope).await;
    println!(
        "lint: duplicates_deprecated={} scratch_purged={} inferences_expired={} weak_lessons={} conflicts_dampened={}",
        lint.duplicates_deprecated,
        lint.scratch_purged,
        lint.inferences_expired,
        lint.weak_lessons_demoted,
        lint.conflicts_dampened,
    );
    let cluster = jobs.run_cluster(&scope).await;
    println!(
        "cluster: clusters_created={} members_absorbed={}",
        cluster.clusters_created, cluster.members_absorbed
    );
    Ok(())
}

fn print_usage() {
    println!("Jarvis v0.4 CLI

Commands:
  jarvis route <input>             run Router and print decision
  jarvis chat                      interactive REPL
  jarvis memory write <text>       record a user-explicit memory
  jarvis memory list               list memories
  jarvis raw-log <session_id>      dump raw_event_log
  jarvis growth events [type]      list growth events
  jarvis growth artifacts          list growth artifacts
  jarvis trace <trace_id>          dump every raw event for a trace
  jarvis replay <session> [iso]    point-in-time replay (default: now)
  jarvis audit <session_id>        dump audit_log for a session
  jarvis trace-view <trace_id>     pretty-print trace + audit
  jarvis memory-history <mem_id>   show full Memory change log
  jarvis maintenance [scope]       run Dream lint + cluster once
  jarvis serve [host:port]         start HTTP API (default 127.0.0.1:7777)

Env:
  JARVIS_DB                        path to sqlite file (default: ./jarvis.db)
");
}
