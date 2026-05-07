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
use jarvis_db::Db;
use jarvis_growth::{ArtifactStatus, ArtifactType, Collector};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = jarvis_control::init_tracing();

    let args: Vec<String> = env::args().collect();
    let db_path = env::var("JARVIS_DB").unwrap_or_else(|_| "jarvis.db".into());
    let db = Db::open(&db_path)?;

    // --json strips the flag from argv and signals to the dispatcher.
    let json_mode = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--json").collect();

    // Per-subcommand --help / -h: matches `jarvis <sub> --help` (or -h)
    // anywhere after the subcommand. Prints subcommand-specific help and
    // returns 0 without touching the DB. The top-level `--help` /
    // `jarvis help` still routes through print_usage() below.
    if let Some(sub) = args.get(1) {
        let asks_help = args
            .iter()
            .skip(2)
            .any(|a| a == "--help" || a == "-h");
        if asks_help {
            if let Some(text) = subcommand_help(sub) {
                println!("{text}");
                return Ok(());
            }
        }
    }

    match args.get(1).map(String::as_str) {
        Some("route") => {
            let input = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "帮我看个 OpenWrt 的报错".into());
            let pretty = match env::var("JARVIS_JUDGE").as_deref() {
                Ok("codex") => {
                    let judge = jarvis_codex::CodexJudge::new(
                        jarvis_codex::CodexConfig::from_env(),
                    );
                    jarvis_cli::cmd_route_with_judge(&db, &input, &judge).await?
                }
                _ => jarvis_cli::cmd_route(&db, &input)?,
            };
            println!("{pretty}");
        }
        Some("chat") => chat_repl(db).await?,
        Some("memory") => memory_command(db, &args[2..], json_mode)?,
        Some("raw-log") => {
            let session = args.get(2).cloned().unwrap_or_default();
            if json_mode {
                println!("{}", jarvis_cli::cmd_raw_log_json(&db, &session, 100)?);
            } else {
                for line in jarvis_cli::cmd_raw_log(&db, &session, 100)? {
                    println!("{line}");
                }
            }
        }
        Some("growth") => growth_command(db, &args[2..])?,
        Some("trace") => trace_command(db, &args[2..])?,
        Some("replay") => replay_command(db, &args[2..])?,
        Some("audit") => {
            let session = args.get(2).cloned().unwrap_or_default();
            if json_mode {
                println!("{}", jarvis_cli::cmd_audit_json(&db, &session, 100)?);
            } else {
                for line in jarvis_cli::cmd_audit(&db, &session, 100)? {
                    println!("{line}");
                }
            }
        }
        Some("trace-view") => {
            let trace_id = args.get(2).cloned().unwrap_or_default();
            for line in jarvis_cli::cmd_trace_view(&db, &trace_id)? {
                println!("{line}");
            }
        }
        Some("memory-history") => {
            let memory_id = args.get(2).cloned().unwrap_or_default();
            for line in jarvis_cli::cmd_memory_history(&db, &memory_id)? {
                println!("{line}");
            }
        }
        Some("dashboard") => {
            if json_mode {
                println!("{}", jarvis_cli::cmd_dashboard_summary_json(&db)?);
            } else {
                println!("{}", jarvis_cli::cmd_dashboard_summary(&db)?);
            }
        }
        Some("sessions") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") | None => {
                    if json_mode {
                        println!("{}", jarvis_cli::cmd_sessions_list_json(&db)?);
                    } else {
                        for line in jarvis_cli::cmd_sessions_list(&db)? {
                            println!("{line}");
                        }
                    }
                }
                Some("messages") => {
                    let session = args.get(3).cloned().unwrap_or_default();
                    for line in jarvis_cli::cmd_session_messages(&db, &session, 200)? {
                        println!("{line}");
                    }
                }
                Some("new") => {
                    let title = args.get(3).cloned().unwrap_or_default();
                    let domain = args.get(4).cloned().unwrap_or_else(|| "general".into());
                    println!("{}", jarvis_cli::cmd_sessions_new(&db, &title, &domain)?);
                }
                Some("archive") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_sessions_archive(&db, &id)?);
                }
                Some("capacity") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_sessions_capacity(&db, &id)?);
                }
                Some("handoff") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_sessions_handoff(&db, &id)?);
                }
                Some(other) => anyhow::bail!("unknown sessions subcommand: {other}"),
            }
        }
        Some("persona") => {
            let sub = args.get(2).map(String::as_str);
            let scope = args
                .iter()
                .position(|a| a == "--scope")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "global".into());
            match sub {
                Some("get") | None => {
                    println!("{}", jarvis_cli::cmd_persona_get(&db, &scope)?);
                }
                Some("set") => {
                    let content = args
                        .iter()
                        .skip(3)
                        .filter(|a| a.as_str() != "--scope" && *a != &scope)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    println!("{}", jarvis_cli::cmd_persona_set(&db, &scope, &content)?);
                }
                Some(other) => anyhow::bail!("unknown persona subcommand: {other}"),
            }
        }
        Some("activity-cards") => {
            let session = args.get(2).cloned().unwrap_or_default();
            for line in jarvis_cli::cmd_activity_cards(&db, &session)? {
                println!("{line}");
            }
        }
        Some("skills") => {
            if json_mode {
                println!("{}", jarvis_cli::cmd_skills_list_json(&db)?);
            } else {
                for line in jarvis_cli::cmd_skills_list(&db)? {
                    println!("{line}");
                }
            }
        }
        Some("walkthrough") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") => {
                    let session = args.get(3).cloned().unwrap_or_default();
                    for line in jarvis_cli::cmd_walkthrough_list(&db, &session)? {
                        println!("{line}");
                    }
                }
                Some("approve") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    let actor = args.get(4).cloned().unwrap_or_else(|| "user".into());
                    println!("{}", jarvis_cli::cmd_walkthrough_approve(&db, &id, &actor)?);
                }
                Some("reject") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    let actor = args.get(4).cloned().unwrap_or_else(|| "user".into());
                    let reason = args.get(5).map(|s| s.as_str());
                    println!(
                        "{}",
                        jarvis_cli::cmd_walkthrough_reject(&db, &id, &actor, reason)?
                    );
                }
                _ => anyhow::bail!(
                    "Usage: walkthrough list <session_id> | approve <id> [by] | reject <id> [by] [reason]; got {sub:?}"
                ),
            }
        }
        Some("outbox") => {
            println!("{}", jarvis_cli::cmd_outbox_pending(&db)?);
        }
        Some("serve") => serve_command(db, &args[2..]).await?,
        Some("maintenance") => maintenance_command(db, &args[2..]).await?,
        Some("demo") => demo_command(db).await?,
        Some("judge") => judge_command(&args[2..]).await?,
        Some("verifier") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    for line in jarvis_cli::cmd_verifier_list(&db, &id)? {
                        println!("{line}");
                    }
                }
                Some("status") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_verifier_status(&db, &id)?);
                }
                _ => println!("Usage: verifier list <doc_id> | verifier status <doc_id>"),
            }
        }
        Some("handoff") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") | None => {
                    let limit = args
                        .get(3)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(20usize);
                    for line in jarvis_cli::cmd_handoff_list(&db, limit)? {
                        println!("{line}");
                    }
                }
                Some("show") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_handoff_show(&db, &id)?);
                }
                Some("accept") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    let title = args.get(4).map(String::as_str);
                    println!("{}", jarvis_cli::cmd_handoff_accept(&db, &id, title)?);
                }
                Some("decline") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_handoff_decline(&db, &id)?);
                }
                Some(other) => anyhow::bail!("unknown handoff subcommand: {other}"),
            }
        }
        Some("commands") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") | None => {
                    if json_mode {
                        println!("{}", jarvis_cli::cmd_commands_list_json()?);
                    } else {
                        for line in jarvis_cli::cmd_commands_list()? {
                            println!("{line}");
                        }
                    }
                }
                Some("run") => {
                    let cmd_id = args.get(3).cloned().unwrap_or_default();
                    let sess = args.get(4).cloned().unwrap_or_default();
                    let actor = args
                        .get(5)
                        .cloned()
                        .unwrap_or_else(|| "user_button".into());
                    println!(
                        "{}",
                        jarvis_cli::cmd_commands_run(&db, &cmd_id, &sess, &actor)?
                    );
                }
                Some("status") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_commands_status(&db, &id)?);
                }
                Some("recent") => {
                    let sess = args.get(3).cloned();
                    let limit = args
                        .get(4)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(20usize);
                    for line in jarvis_cli::cmd_commands_recent(
                        &db,
                        sess.as_deref(),
                        limit,
                    )? {
                        println!("{line}");
                    }
                }
                Some(other) => anyhow::bail!("unknown commands subcommand: {other}"),
            }
        }
        Some("regression") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("latest") | None => {
                    println!("{}", jarvis_cli::cmd_regression_latest(&db)?);
                }
                Some("list") => {
                    let limit = args
                        .get(3)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(20usize);
                    for line in jarvis_cli::cmd_regression_list(&db, limit)? {
                        println!("{line}");
                    }
                }
                Some(other) => anyhow::bail!("unknown regression subcommand: {other}"),
            }
        }
        Some("model") => {
            let sub = args.get(2).map(String::as_str);
            match sub {
                Some("list") | None => {
                    if json_mode {
                        println!("{}", jarvis_cli::cmd_model_list_json()?);
                    } else {
                        for line in jarvis_cli::cmd_model_list()? {
                            println!("{line}");
                        }
                    }
                }
                Some("current") => {
                    println!("{}", jarvis_cli::cmd_model_current()?);
                }
                Some("set") => {
                    let id = args.get(3).cloned().unwrap_or_default();
                    println!("{}", jarvis_cli::cmd_model_set(&id)?);
                }
                Some(other) => anyhow::bail!("unknown model subcommand: {other}"),
            }
        }
        Some("--help") | Some("-h") | Some("help") | None => print_usage(),
        Some(other) => {
            eprintln!("unknown subcommand: {other}\n");
            print_usage();
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn chat_repl(db: Db) -> anyhow::Result<()> {
    // When a non-rule judge is selected, codex calls easily exceed the
    // default 2 s SLA. Bump fallback_ack so the REPL waits for the
    // judge to settle (still bounded — tweak via JARVIS_FALLBACK_SECS).
    let judge_kind = env::var("JARVIS_JUDGE").ok();
    let sla = if judge_kind.as_deref() == Some("codex") {
        let secs = env::var("JARVIS_FALLBACK_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(180);
        let mut s = jarvis_control::sla::ResponseSla::defaults();
        s.fallback_ack = std::time::Duration::from_secs(secs);
        s
    } else {
        jarvis_control::sla::ResponseSla::defaults()
    };
    let cp = ControlPlane::with_sla(db, sla);
    let codex_judge = if judge_kind.as_deref() == Some("codex") {
        Some(std::sync::Arc::new(jarvis_codex::CodexJudge::new(
            jarvis_codex::CodexConfig::from_env(),
        )))
    } else {
        None
    };
    // Load LLM config + a single Completion client for the REPL's
    // lifetime. Missing config is fine — the REPL still routes and
    // shows decisions; it just can't synthesise replies.
    //
    // The router dispatches by `<provider>/<model>` provider segment:
    // OAuth-only providers (claude-cli) get their own subprocess
    // backend; everything else falls through to genai.
    let llm_cfg = jarvis_llm::load_default().unwrap_or_default();
    let llm_client: Option<std::sync::Arc<dyn jarvis_llm::Completion>> =
        if llm_cfg.default_model.is_some() {
            let mut router = jarvis_llm::CompletionRouter::new(Box::new(
                jarvis_llm::GenAiCompletion::new(),
            ));
            // Register claude-cli only when the binary is on PATH; if
            // it's not, falling through to genai's default routing
            // produces a clearer error than "binary not found".
            if jarvis_llm::binary_on_path("claude") {
                router = router.with_provider(
                    "claude-cli",
                    Box::new(jarvis_claude_cli::ClaudeCliCompletion::new(
                        jarvis_claude_cli::ClaudeCliConfig::from_env(),
                    )),
                );
            }
            Some(std::sync::Arc::new(router))
        } else {
            None
        };
    let agent_registry = jarvis_router::builtin_agents();

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let banner = match judge_kind.as_deref() {
        Some("codex") => "Jarvis v0.1 — chat REPL [judge=codex]. Type :quit to exit.",
        _ => "Jarvis v0.1 — chat REPL. Type :quit to exit.",
    };
    println!("{banner}");
    match llm_cfg.default_model.as_deref() {
        Some(m) => println!("  · model = {m}"),
        None => println!(
            "  · no default model — `jarvis model set anthropic/claude-sonnet-4-6` to enable replies."
        ),
    }

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
        let resp = match codex_judge.as_ref() {
            Some(j) => {
                cp.handle_user_input_with_judge(
                    line.to_string(),
                    None,
                    vec![],
                    j.clone(),
                )
                .await
            }
            None => cp.handle_user_input(line.to_string(), None, vec![]).await,
        };
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

                if let (Some(client), Some(model)) = (
                    llm_client.as_ref(),
                    llm_cfg.default_model.as_deref(),
                ) {
                    if let Err(msg) = run_completion_turn(
                        client.as_ref(),
                        model,
                        &llm_cfg,
                        &agent_registry,
                        &decision.agent_type,
                        line,
                    )
                    .await
                    {
                        println!("  ↳ {msg}");
                    }
                }
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

/// Single-turn completion against the configured default model.
/// Looks up the agent definition matching `agent_type`, renders the
/// stable system prompt block, sends [system, user], and prints the
/// reply with model id, elapsed time, and token usage.
///
/// Returns `Err(message)` for soft failures (invalid id, missing key,
/// upstream error) so the caller can print a one-line `↳ ...` note
/// without crashing the REPL.
async fn run_completion_turn(
    client: &dyn jarvis_llm::Completion,
    model: &str,
    llm_cfg: &jarvis_llm::LlmConfig,
    registry: &[jarvis_core::agent::AgentDefinition],
    agent_type: &str,
    user_input: &str,
) -> Result<(), String> {
    let parsed = jarvis_llm::ModelId::parse(model)
        .map_err(|e| format!("invalid model id `{model}`: {e}"))?;
    if !jarvis_llm::provider_authed(&parsed.provider, llm_cfg) {
        let env = jarvis_llm::provider_env_var(&parsed.provider, llm_cfg)
            .unwrap_or_else(|| "<none>".into());
        return Err(format!(
            "provider `{}` not authed (set {}); skipping completion",
            parsed.provider, env
        ));
    }
    let agent = registry
        .iter()
        .find(|a| a.r#type == agent_type)
        .or_else(|| registry.iter().find(|a| a.r#type == "general"))
        .ok_or_else(|| "no agent matched and `general` is missing".to_string())?;
    let system = jarvis_router::render_stable_block(&jarvis_router::StablePromptInputs {
        agent,
        persona: None,
        framework_directives: &[],
    });
    let req = jarvis_llm::CompletionRequest::new(
        model,
        vec![
            jarvis_llm::ChatMessage::system(system),
            jarvis_llm::ChatMessage::user(user_input.to_string()),
        ],
    )
    .with_max_tokens(1024);
    let started = std::time::Instant::now();
    let reply = client
        .chat(req)
        .await
        .map_err(|e| format!("completion failed: {e}"))?;
    let ms = started.elapsed().as_millis();
    let usage = reply
        .usage
        .map(|u| format!(" · in={} out={}", u.input_tokens, u.output_tokens))
        .unwrap_or_default();
    println!(
        "\n{} {}\n  ({} · {}ms{})\n",
        agent.avatar_emoji,
        reply.text.trim(),
        reply.model,
        ms,
        usage,
    );
    Ok(())
}

fn memory_command(db: Db, args: &[String], json_mode: bool) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("write") => {
            let content = args
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            println!("{}", jarvis_cli::cmd_memory_write(&db, &content, "global")?);
        }
        Some("list") => {
            if json_mode {
                println!("{}", jarvis_cli::cmd_memory_list_json(&db, "global")?);
            } else {
                for line in jarvis_cli::cmd_memory_list(&db, "global")? {
                    println!("{line}");
                }
            }
            return Ok(());
        }
        Some("search") => {
            let query = args
                .iter()
                .skip(1)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if json_mode {
                println!("{}", jarvis_cli::cmd_memory_search_json(&db, "global", &query, 20)?);
            } else {
                for line in jarvis_cli::cmd_memory_search(&db, "global", &query, 20)? {
                    println!("{line}");
                }
            }
        }
        Some("forget") => {
            let id = args
                .get(1)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Usage: memory forget <id> [reason]"))?;
            let reason = args.get(2).map(String::as_str);
            println!("{}", jarvis_cli::cmd_memory_forget(&db, &id, reason)?);
        }
        _ => {
            println!(
                "Usage: memory write <text> | memory list | memory search <q> | memory forget <id> [reason]"
            );
        }
    }
    Ok(())
}

#[allow(dead_code)] // legacy helper, kept for reference
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

#[allow(dead_code)] // superseded by jarvis_cli::cmd_audit; kept for reference
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

#[allow(dead_code)] // superseded by jarvis_cli::cmd_trace_view
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

#[allow(dead_code)] // superseded by jarvis_cli::cmd_memory_history
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

async fn judge_command(args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("probe") | None => {
            // Adapter selection mirrors `jarvis route`'s JARVIS_JUDGE.
            let kind = env::var("JARVIS_JUDGE").unwrap_or_else(|_| "codex".into());
            match kind.as_str() {
                "codex" => {
                    let judge = jarvis_codex::CodexJudge::new(
                        jarvis_codex::CodexConfig::from_env(),
                    );
                    println!("{}", jarvis_cli::cmd_judge_probe(&judge).await?);
                }
                other => {
                    anyhow::bail!(
                        "unknown JARVIS_JUDGE={other:?}; supported: codex (set JARVIS_JUDGE=codex)"
                    );
                }
            }
        }
        Some(other) => anyhow::bail!("Usage: judge probe; got {other:?}"),
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

    // Background scheduler (Dream lint / cluster / lock sweep).
    let sched = jarvis_control::Scheduler::new(
        db.clone(),
        jarvis_control::SchedulerConfig::default(),
    );
    let sched_handles = sched.start();

    // Optional outbox replication if peer endpoint is configured.
    if let Ok(peer) = std::env::var("JARVIS_REPLICATION_PEER") {
        let cfg = jarvis_control::ReplicationConfig {
            peer_endpoint: peer,
            auth_token: std::env::var("JARVIS_REPLICATION_TOKEN").ok(),
            ..Default::default()
        };
        let rep = jarvis_control::Replicator::new(db.clone(), cfg);
        // Spawn returns a JoinHandle that lives for the lifetime of
        // the server; we deliberately let it run detached.
        let _replication_handle = rep.spawn();
    }

    let state = jarvis_api::ApiState { db };
    let result = jarvis_api::serve(state, addr).await;
    for h in sched_handles {
        h.abort();
    }
    result
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

/// Per-subcommand help text. Returns None when no specific help is
/// registered (caller falls through to top-level usage / dispatch).
/// Centralised so adding a subcommand only touches one match arm.
fn subcommand_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "route" => "\
jarvis route <input> [--json]

  Run the Router on <input> and print the RouteDecision.

  Args:
    <input>           User input string. Default: \"帮我看个 OpenWrt 的报错\".

  Flags:
    --json            Pretty-printed JSON (default: pretty JSON anyway).
    --help, -h        Show this help.

  Env:
    JARVIS_JUDGE=codex      Use the codex LLM judge instead of rule-only.
    CODEX_BINARY=/path      Override codex binary path.
    CODEX_MODEL=<id>        Override codex model.
    CODEX_TIMEOUT_SECS=<n>  Override codex per-call timeout.

  Examples:
    jarvis route \"openwrt 编译报错\"
    JARVIS_JUDGE=codex jarvis route \"@代码助手 重构 sync\"
",
        "chat" => "\
jarvis chat

  Interactive REPL. Reads stdin, routes each line, prints decision,
  and (when a default model is configured) calls the model and prints
  its reply tagged with the chosen agent. Type :quit to exit.

  Configure the model with `jarvis model set <provider/model>`. When
  no model is set, the REPL still routes and prints decisions but
  cannot synthesise replies.

  Env:
    JARVIS_JUDGE=codex          Route via codex LLM judge.
    JARVIS_FALLBACK_SECS=<n>    Override fallback SLA (default 180s with judge).
    ANTHROPIC_API_KEY / OPENAI_API_KEY / …  Auth for the chosen provider.
",
        "memory" => "\
jarvis memory <subcommand>

  write <text>          Record a user-explicit preference memory.
  list [--json]         List approved memories in scope `global`.
  search <q> [--json]   Hybrid retrieval; prints score + id per row.
  forget <id> [reason]  Deprecate a memory; audit-logged in memory_change_log.
",
        "memory-history" => "\
jarvis memory-history <mem_id>

  Dump the full memory_change_log chain for one memory.
  Useful for debugging Dream / inference / user-correction history.
",
        "sessions" => "\
jarvis sessions <subcommand>

  list [--json]                List active sessions.
  new <title> [domain]         Create a new active session (default domain=general).
  archive <id>                 Move session to status=archived (idempotent).
  messages <sess>              Recent messages for a session.
  capacity <sess>              v1.9: ContextHealth + advisory level.
  handoff <sess>               v1.9: generate a HandoffSnapshot for the session.

  --json on `list` prints a JSON array with id/title/domain/last_active.
",
        "handoff" => "\
jarvis handoff <subcommand>            (PRD v1.9)

  list [limit]                 List pending / deferred handoff snapshots.
  show <id>                    Pretty-print one snapshot as JSON.
  accept <id> [new_title]      Accept: archive source session, create
                               new session inheriting cold-start payload.
  decline <id>                 Decline; snapshot frozen as declined.
",
        "activity-cards" => "\
jarvis activity-cards <session_id>

  List ActivityCard rows for a session — the data layer behind the
  multi-Agent collaboration panel (PRD §8.13).
",
        "persona" => "\
jarvis persona <subcommand> [--scope <name>]

  get          Print persona content_json + updated_at.
  set <body>   Upsert persona; <body> can be raw JSON or plain text
               (plain text is wrapped as a JSON string).

  --scope defaults to `global`.
",
        "raw-log" => "\
jarvis raw-log <session_id> [--json]

  Dump the immutable raw_event_log entries for a session, oldest first.
",
        "audit" => "\
jarvis audit <session_id> [--json]

  List audit_log rows for a session.
",
        "trace" => "\
jarvis trace <trace_id>

  Print raw_event_log rows that share <trace_id>.
",
        "trace-view" => "\
jarvis trace-view <trace_id>

  Pretty-print trace events + correlated audit rows. Best for
  debugging a single user→agent→tool round-trip.
",
        "replay" => "\
jarvis replay <session_id> [iso8601]

  Reconstruct a session's state at a point in time using the latest
  session_snapshot ≤ ts plus subsequent raw_event_log rows.
",
        "walkthrough" => "\
jarvis walkthrough <subcommand>

  list <session_id>                       List walkthroughs in a session.
  approve <doc_id> [actor]                Manually approve a walkthrough.
  reject <doc_id> [actor] [reason]        Manually reject with reason.
",
        "verifier" => "\
jarvis verifier <subcommand>

  list <doc_id>     List saved VerifierCheck rows for a walkthrough.
  status <doc_id>   Show verification + approval state.

  Read-only. Re-running checks needs a populated worker driver.
",
        "regression" => "\
jarvis regression <subcommand>

  latest                   Show the most recent RegressionReport.
  list [limit]             List recent reports (default 20).
",
        "commands" => "\
jarvis commands <subcommand>

  list [--json]                       Show built-in command catalogue (PRD §8.17).
  run <command_id> <session_id> [by]  Begin executing; persists CommandExecution.
  status <execution_id>               Print current execution + step states.
  recent [session_id] [limit]         Recent executions, newest first.

  Note: `run` records the execution row; concrete step processing is
  driven by the sub-task dispatcher / parallel-explore runtime once
  those land. Use `status` to poll progress.
",
        "skills" => "\
jarvis skills [--json]

  List skills in the SkillRegistry, newest first.
",
        "outbox" => "\
jarvis outbox

  Show the pending row count of the replication outbox.
",
        "growth" => "\
jarvis growth <subcommand>

  events [type]                       List growth events (default route_decision).
  artifacts                           List growth artifacts.
  filter <type> [status]              Filter artifacts by type and optional status.
",
        "dashboard" => "\
jarvis dashboard [--json]

  active_sessions / raw_events / memories / pending_outbox counters.
",
        "judge" => "\
jarvis judge probe

  Run a one-shot canned input through the selected JARVIS_JUDGE adapter
  to verify auth + binary + network. Use to triage silent fallbacks.
",
        "model" => "\
jarvis model <subcommand>

  list [--json]                       Show configured providers + auth status.
  current                             Print the default model id.
  set <provider/model>                Set the default model (writes config file).

  Config file: ~/.jarvis/config.toml (override with JARVIS_CONFIG).
  Model id format: `<provider>/<model>` (e.g. anthropic/claude-sonnet-4-6).

  API-key providers (via genai): anthropic, openai, gemini, groq,
  cohere, deepseek, xai, fireworks, together, ollama. Each reads its
  well-known env var (ANTHROPIC_API_KEY / OPENAI_API_KEY / …).

  OAuth providers (subprocess-wrap an external CLI):
    claude-cli/<model>   Uses the `claude` CLI (Anthropic OAuth).
                          Requires `claude` on PATH and `claude login`
                          to have been run. Models: sonnet, opus, or
                          a full id like claude-sonnet-4-6.
                          Env: CLAUDE_BINARY, CLAUDE_TIMEOUT_SECS.
",
        "maintenance" => "\
jarvis maintenance [scope]

  Run Dream lint + cluster once for `scope` (default global).
",
        "serve" => "\
jarvis serve [host:port]

  Start the HTTP API server (default 127.0.0.1:7777).
",
        "demo" => "\
jarvis demo

  One-shot end-to-end smoke: write a memory, route an input, dispatch
  a sub-task, run Dream lint, dump dashboard counters.
",
        _ => return None,
    })
}

fn print_usage() {
    println!("Jarvis v1.0 CLI

Routing & chat:
  jarvis route <input>                 run Router and print decision
  jarvis chat                          interactive REPL

Memory:
  jarvis memory write <text>           record a user-explicit memory
  jarvis memory list                   list memories in scope `global`
  jarvis memory search <q>             hybrid-rank search across memories
  jarvis memory forget <id> [reason]   deprecate a memory (audit-logged)
  jarvis memory-history <mem_id>       full memory_change_log for one memory

Sessions:
  jarvis sessions list                 list active sessions
  jarvis sessions new <title> [domain] create a new active session
  jarvis sessions archive <id>         archive a session (idempotent)
  jarvis sessions messages <sess>      recent messages for a session
  jarvis sessions capacity <sess>      v1.9 ContextHealth advisory
  jarvis sessions handoff <sess>       v1.9 generate HandoffSnapshot
  jarvis activity-cards <sess>         list activity cards for a session

Handoff (v1.9):
  jarvis handoff list [limit]          pending handoff snapshots
  jarvis handoff show <id>             pretty-print snapshot JSON
  jarvis handoff accept <id> [title]   archive source + start new session
  jarvis handoff decline <id>          decline snapshot

Persona:
  jarvis persona get [--scope global]  print persona JSON
  jarvis persona set <text|json> [--scope global]  upsert persona
  jarvis raw-log <session_id>          dump raw_event_log
  jarvis audit <session_id>            dump audit_log
  jarvis trace <trace_id>              raw events for a trace
  jarvis trace-view <trace_id>         pretty-print trace + audit
  jarvis replay <session> [iso]        point-in-time replay (default: now)

Walkthroughs & skills:
  jarvis walkthrough list <session>    list walkthroughs for a session
  jarvis walkthrough approve <id> [by] manually approve a walkthrough
  jarvis walkthrough reject <id> [by] [reason]
  jarvis skills                        list skill catalogue

Growth & maintenance:
  jarvis growth events [type]          list growth events
  jarvis growth artifacts              list growth artifacts
  jarvis outbox                        outbox pending count
  jarvis maintenance [scope]           run Dream lint + cluster once
  jarvis dashboard [--json]            counters summary

Verification & Regression:
  jarvis verifier list <doc_id>        list saved verifier checks for a walkthrough
  jarvis verifier status <doc_id>      show verification + approval status
  jarvis regression latest             show most recent regression report
  jarvis regression list [limit]       list regression reports, newest first

Commands (PRD §8.17 quick-actions):
  jarvis commands list [--json]        show built-in command catalogue
  jarvis commands run <id> <sess>      begin execution
  jarvis commands status <exec_id>     print execution + step states
  jarvis commands recent [sess] [n]    recent executions

Provider:
  jarvis judge probe                   verify selected JARVIS_JUDGE adapter is live
  jarvis model list [--json]           show configured providers and auth status
  jarvis model current                 print the default model id
  jarvis model set <provider/model>    set the default model (writes config file)
                                       (claude-cli/<model> wraps the OAuth `claude` CLI)

Server / demo:
  jarvis serve [host:port]             start HTTP API (default 127.0.0.1:7777)
  jarvis demo                          one-shot end-to-end smoke test

Flags:
  --json                               JSON-format output (where supported)

Env:
  JARVIS_DB                            path to sqlite file (default: ./jarvis.db)
  JARVIS_LOG                           log filter (default: info)
  JARVIS_LOG_JSON=1                    JSON-format log output
  JARVIS_REPLICATION_PEER              outbox replication endpoint
  JARVIS_REPLICATION_TOKEN             bearer token for replication
  ANTHROPIC_API_KEY / OPENAI_API_KEY   for LLM judge adapters and chat completions
  JARVIS_JUDGE=codex                   route through local codex CLI subprocess
  CODEX_BINARY / CODEX_MODEL / CODEX_TIMEOUT_SECS  override codex defaults
  JARVIS_CONFIG                        path to TOML config (default: ~/.jarvis/config.toml)
");
}
