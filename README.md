# Jarvis — Global AI Runtime (Rust)

Rust implementation of the **Jarvis Global AI Runtime** PRD (v1.8 spec).

This repo currently ships **v0.1 + most of v0.2**:

- v0.1 (shipped): rule-layer Router, SessionResolver, immutable raw
  event log, Memory + change-log, Control Plane response SLA + Watchdog,
  CLI.
- v0.2 (this commit, partial): Tool Runtime with permission model,
  Growth Engine (Collector + PromotionGate + Regression-aware skill
  promotion), Compression (TurnSummary + Rolling Summary versioning +
  dynamic threshold policy), Orchestrator subsystem (TaskTree +
  ArtifactRegistry + SubTaskEnvelope), ConversationBus with ownership
  state machine + sub-channels + user-message router, SubTaskCheckpoint
  for soft-interrupt resume, Steer protocol with throttle + audit,
  Tentacle file generator (`CONTEXT.md` write-protected, `todo.md`
  checkboxes, `NOTES.md` append, `HANDOFF.md` one-shot, file locks).

Still TODO before tagging v0.2 final: real LLM judge layer, sub-agent
dispatch with Codex driver, ActivityCard rendering, Worktree management.

The PRD describes more (Walkthrough/Verifier, Dream system, Persona
layer, full @mention REPL polish). Those land in v0.3/v0.4/v1.0 — see
[Roadmap](#roadmap).

---

## Workspace layout

```
crates/
  jarvis-core/          Pure domain types
                        (RouteDecision, Memory, Session, AgentDefinition,
                         ToolScope, MentionMode, TaskNode, …)
  jarvis-db/            SQLite storage; immutable triggers; raw_event_log;
                        memory_change_log; session/memory/mention repos;
                        full v0.2 schema (task_trees, artifacts,
                        conversation_ownerships, sub_channels,
                        sub_task_checkpoints, steer_signals,
                        turn_summaries, rolling_summary_versions,
                        growth_events, growth_artifacts, …)
  jarvis-router/        Main Router: rule-layer IntentClassifier, @mention
                        parser, SessionResolver scoring, agent registry,
                        Growth Engine wiring
  jarvis-memory/        MemoryManager (write rules + change-log atomicity),
                        Jaccard retrieval + emotion resonance, trust score,
                        compression (TurnSummary, RollingSummary versions,
                        three-dim dynamic policy)
  jarvis-tools/         Tool Runtime: scope validation, confirmation gate,
                        timeout, raw_event_log audit on every call
  jarvis-growth/        GrowthEvent / GrowthArtifact / Collector /
                        PromotionGate (Section 16.6 rules)
  jarvis-orchestrator/  TaskTree + ArtifactRegistry + SubTaskEnvelope +
                        ConversationBus + ownership state machine +
                        SubTaskCheckpoint + Steer protocol +
                        Tentacle file generator with locks +
                        Workspace lock (in-memory + durable) +
                        Walkthrough + Verifier + ActivityCard +
                        RegressionOrchestrator + commands.json runner +
                        SubAgentDispatcher + InProcessDriver +
                        WorkerProcessDriver + Interrupt protocol +
                        OrchestrationPipeline + SteerAdapter
  jarvis-control/       Control Plane / Task Plane separation, response SLA,
                        sub-agent Watchdog, fallback responder, periodic
                        Scheduler (Dream lint / cluster / lock sweep)
  jarvis-api/           HTTP API — POST /router/input, GET /sessions/*,
                        GET /memory/*, POST /memory, GET /raw-log/*,
                        GET /trace/*, GET /audit/*, GET /growth/*,
                        GET /walkthrough/*, GET /healthz
  jarvis-cli/           `jarvis` binary — route / chat / memory / raw-log /
                        growth / trace / replay / audit / maintenance / serve
```

**242 unit tests pass** across the workspace.

---

## Quickstart

```bash
cargo build --release
cargo test --workspace

# route a single input through the rule layer
JARVIS_DB=./jarvis.db ./target/release/jarvis route "OpenWrt DNS hosts 不生效"

# interactive REPL through the Control Plane
./target/release/jarvis chat

# record a long-term preference
./target/release/jarvis memory write "用户偏好函数式风格"

# inspect Growth Engine
./target/release/jarvis growth events route_decision
./target/release/jarvis growth artifacts

# inspect immutable raw event log for a session
./target/release/jarvis raw-log sess_xxxx
```

---

## What's wired up

### v0.1 (Section 24.1)

| Invariant | Where |
|---|---|
| Raw input → `raw_event_log` *before* any classification | `jarvis-router/src/router.rs::Router::route` Step 0 |
| `raw_event_log` blocks UPDATE/DELETE via SQL triggers | `jarvis-db/src/migrations.rs` |
| sha256 checksum on every raw event | `jarvis-db/src/raw_event_log.rs::compute_checksum` |
| Memory writes atomic with `memory_change_log` | `jarvis-db/src/memory_repo.rs::upsert` |
| Control Plane response SLA fallback ≤ 2000ms | `jarvis-control/src/control_plane.rs` |
| Sub-agent Watchdog state machine | `jarvis-control/src/watchdog.rs` |
| Control / Task plane separation | `jarvis-control/src/control_plane.rs` |
| `@mention` parsing + `mentionable=false` for internal agents | `jarvis-router/src/{mention,agent_registry}.rs` |
| Emotion-coordinate gate forces neutral on non-emotion types | `jarvis-core/src/memory.rs::Memory::enforce_emotion_gate` |
| Tier-1 trust floor ≥ 0.30, retrieve-boost cap ≤ 0.20 | `jarvis-memory/src/trust.rs::compute` |

### v0.2

| Invariant | Where |
|---|---|
| Tool calls scope-checked, confirmation-gated, timeout-bounded | `jarvis-tools/src/runtime.rs::ToolRuntime::call` |
| Every tool call audited to `raw_event_log` (call + result) | same — `audit_result` |
| Router emits `route_decision` GrowthEvent (Section 5.5 tolerant) | `jarvis-router/src/router.rs` end of `route()` |
| Skill promotion blocked without ≥3 successes / <20% failure / regression ≥80% | `jarvis-growth/src/promotion.rs::PromotionGate::eval_skill` |
| Compression threshold tracks task complexity + usage budget | `jarvis-memory/src/compression.rs::compression_threshold` |
| Rolling Summary writes are versioned and update `sessions.long_summary` in one tx | `jarvis-memory/src/compression.rs::CompressionStore::append_rolling_version` |
| TaskTreeView is recomputed on demand (recent completed + active) | `jarvis-orchestrator/src/task_tree.rs::TaskTreeStore::build_view` |
| Ownership acquire releases prior holder atomically | `jarvis-orchestrator/src/conversation_bus.rs::acquire_ownership` |
| User message classifier — interrupt > steer > progress > normal | `…::classify_user_message` |
| Steer signals throttled at 3/60s, audited to `raw_event_log` | `jarvis-orchestrator/src/steer.rs::SteerController::enqueue` |
| Tentacle CONTEXT.md is write-protected for sub-agents | `jarvis-orchestrator/src/tentacle.rs::Tentacle::try_overwrite_context` |
| Tentacle HANDOFF.md is one-shot (no overwrite) | `…::Tentacle::write_handoff_once` |
| File-level locks on todo/notes/handoff with 30s upper bound | `…::tentacle::lock` |

---

## Test inventory

```
jarvis-core         13 tests
jarvis-db           24 tests
jarvis-memory       40 tests
jarvis-tools        13 tests   (+3: plugin install / duplicate id /
                               unregister provenance)
jarvis-growth       21 tests
jarvis-orchestrator 66 tests   (+2: walkthrough manual approve / reject)
jarvis-router       38 tests
jarvis-control      11 tests
jarvis-api           7 tests   (+1: session messages endpoint)
jarvis-anthropic     5 tests
jarvis-openai        3 tests   (NEW: well-formed response, 500 → None,
                               Bearer authorization sent)
─────────────────
TOTAL              241 tests
```

Each crate is independently testable: `cargo test -p jarvis-orchestrator`,
etc.

---

## Roadmap

### v0.1 ✅ shipped
- Router rule layer, SessionResolver, raw_event_log, Memory + change-log,
  Control Plane / Task Plane split, Watchdog, CLI

### v0.2 (this commit, mostly done)
- ✅ Tool Runtime with permission validation
- ✅ Growth Engine (Collector + PromotionGate + ArtifactStore)
- ✅ Compression (TurnSummary + Rolling Summary + dynamic policy)
- ✅ TaskTree + ArtifactRegistry
- ✅ ConversationBus with ownership state machine + sub-channels
- ✅ SubTaskCheckpoint for soft-interrupt resume
- ✅ Steer protocol with throttle + audit
- ✅ Tentacle file generator (CONTEXT.md write-protected; HANDOFF.md one-shot)
- ✅ Workspace lock (Reader/Writer mode) + Worktree manager
- ✅ ActivityCard storage with lifecycle transitions
- ✅ session_snapshots immutable store (Section 15.7.4)
- ⬜ LLM judgment layer (currently rule-only)
- ⬜ Sub-agent dispatch with worker-process driver

### v0.3 (in progress)
- ✅ WalkthroughDoc + auto-approval policy (data layer in v0.2)
- ✅ VerifierAgent + file_exists / file_content checks (v0.2)
- ✅ RegressionOrchestrator with expected_change vs potential_bug
  classification (this commit)
- ✅ ColdStartSnapshot capture + retire (this commit)
- ✅ Memory Lint (duplicates, stale scratch, expired inference, weak
  lessons, conflict dampening) — Dream "tidy" layer (this commit)
- ✅ Hybrid retrieval: FTS5 + Jaccard + emotion resonance + trust score
- ⬜ Test/lint runners through ToolRuntime
- ⬜ Codex steer adapter
- ⬜ soft / hard / async interrupt with Watchdog escalation

### v0.4 (mostly done)
- ✅ Dream system: lint, cluster, inference
- ✅ Dynamic model up/downgrade with debouncing
- ✅ token-budget self-learning with ±40% guardrail
- ✅ Persona layer (persona.md + user.md sync)
- ✅ Stable system-prompt assembler (`render_stable_block`) consumes the
  Persona layer and the agent definition; ready for downstream callers
  to feed into LLM clients.
- ✅ commands.json catalogue + CommandRunner state machine
  (Section 8.17) — defaults catalogue covers 6 of the 7 PRD commands
- ✅ Pluggable `LlmJudge` trait + `RuleBasedJudge` fallback (Section 5.4
  + 5.5) and `Router::route_with_judge` integration.
- ✅ `SubAgentDriver` trait + `InProcessDriver` + `SubAgentDispatcher`
  glue (Section 8.6 / 9.4).
- ✅ Soft / hard / async interrupt protocol with auto-escalation
  (Section 8.11.6).
- ✅ Provenance / time-point replay (Section 15.7.5) + CLI surface
  (`jarvis trace` / `jarvis replay`).
- ✅ Skill registry + Regression Runner (Sections 18 / 16.7) — runner
  is sandboxed against a `MockToolRuntime` per the PRD.
- ✅ Steer adapter trait + RecordingAdapter + `admissibility_check`
  for protocol-level safety guards (Section 9.11.5).
- ✅ DurableWorkspaceLock (cross-process file lock + stale sweep).

### v1.0
- Growth Dashboard, multi-device sync, Qdrant / pgvector,
  MCP / plugin registry, Trace Viewer, sandboxed tool runtime

---

## License

MIT.
