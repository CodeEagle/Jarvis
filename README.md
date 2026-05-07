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

**352 unit + integration tests pass** across the workspace; an additional
6 `#[ignore]`'d tests hit the live codex CLI for end-to-end LLM-judge
verification.

Tracking PRD **v1.8** (2026-05-04 spec). All v1.4–v1.8 changelog items
are either implemented or have a concrete extension seam:

- v1.4 §12.3a emotion gate, §12.6.1 Dream gap-trigger, §8.14.4 HANDOFF
  primary-input, §20.4 Tentacle locks — ✅
- v1.5 Control/Task plane separation, raw_event_log immutability,
  Steer protocol — ✅
- v1.6 §23 API expansion, §24 dependency graph + 5-stage MVP — ✅
- v1.7 Section 30 全模块测试规范 (30.1–30.14) — ✅ (30.15 below)
- v1.8 @mention 协议 — ✅ (parser produces 4 modes, Router applies
  override / Steer demotion, ActivityCard renders `[@ 指定]` chip,
  unresolved mentions emit `mention.unresolved` GrowthEvent for
  new-agent-need discovery)

---

## Install

### Option A — One-line installer (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/CodeEagle/Jarvis/main/scripts/install.sh | bash
```

Auto-detects platform (Linux x86_64, macOS arm64, macOS x86_64),
downloads the matching release tarball if available, falls back to
`cargo build` from `main` HEAD if no release exists for your target
yet. Verifies SHA256, installs to `~/.local/bin/jarvis`. Use
`JARVIS_INSTALL_DIR=…` to override.

### Option B — Manual download

Grab the asset that matches your platform from
[Releases](https://github.com/CodeEagle/Jarvis/releases) and:

```bash
tar -xzf jarvis-vX.Y.Z-<target>.tar.gz
chmod +x jarvis
mv jarvis ~/.local/bin/        # or any PATH dir
jarvis --help
```

### Option C — Fresh macOS dev build

The `macos-build` workflow runs on every push to `main` and uploads
arm64 + x86_64 binaries as workflow artifacts (30-day retention). To
grab one without waiting for a tagged release:

1. Go to **Actions → macos-build** on GitHub
2. Click the latest green run
3. Download `jarvis-…-aarch64-apple-darwin.tar.gz`
   (or `…-x86_64-apple-darwin.tar.gz` for Intel)
4. `tar -xzf …; chmod +x jarvis; ./jarvis --help`

### Option D — Build from source

```bash
git clone https://github.com/CodeEagle/Jarvis.git
cd Jarvis
cargo build --release -p jarvis-cli
./target/release/jarvis --help
```

Requires Rust ≥ 1.80. SQLite is statically linked
(`rusqlite` bundled feature), no system SQLite needed.

### Run the test suite

```bash
cargo test --workspace                                # 352 default tests
cargo test -p jarvis-codex --lib -- --ignored         # 6 real codex e2e (needs codex login)
```

---

## Quick start (5-minute tour)

```bash
# Pick a sqlite location. Defaults to ./jarvis.db.
export JARVIS_DB="$HOME/.jarvis/jarvis.db"
mkdir -p "$(dirname "$JARVIS_DB")"

# 1. Route a single input through the rule-based Router
jarvis route "OpenWrt DNS hosts 不生效"

# 2. Create a persistent task session
jarvis sessions new "Mirage 重构" coding
jarvis sessions list
jarvis sessions list --json                  # programmatic JSON output

# 3. Record a long-term preference (Tier 1 user_explicit memory)
jarvis memory write "用户偏好函数式编程"
jarvis memory write "Mirage 项目用 Riverpod"
jarvis memory list                           # rows include id, type, trust score
jarvis memory search "vim" --json            # hybrid retrieval: fts + vector + jaccard

# 4. Forget a memory (audit-logged, not hard-deleted)
jarvis memory forget mem_abcdef0123 "privacy cleanup"
jarvis memory-history mem_abcdef0123         # full change-log chain

# 5. Persona — assistant tone + user profile
jarvis persona set '{"style":"terse","language":"zh"}'
jarvis persona get

# 6. Inspect immutable logs (PRD §15.7)
jarvis raw-log sess_abc123
jarvis trace trc_xxxxxx                       # all events sharing one trace id
jarvis trace-view trc_xxxxxx                  # pretty trace + audit
jarvis audit sess_abc123 --json
jarvis replay sess_abc123 2026-01-15T10:00:00Z   # point-in-time replay

# 7. Interactive chat REPL (uses ControlPlane SLA + fallback)
jarvis chat
> openwrt 这个报错怎么办
> :quit

# 8. Run the periodic Dream pipeline manually
jarvis maintenance global

# 9. Health snapshot
jarvis dashboard --json
jarvis outbox

# 10. List built-in quick-action commands (PRD §8.17)
jarvis commands list
jarvis commands run regression_check sess_abc123
jarvis commands status cmd_xxxx
```

Per-subcommand help is consistent: `jarvis <cmd> --help` shows usage
for any subcommand. `--json` is honored by `dashboard`, `sessions list`,
`memory list/search`, `raw-log`, `audit`, `skills`, and `commands list`.

---

## Use a real LLM judge (codex / ChatGPT plan)

Jarvis ships a subprocess adapter that drives the
[OpenAI Codex CLI](https://www.npmjs.com/package/@openai/codex). When
enabled, the Router consults a real model for high-confidence routing
decisions before falling back to the rule layer.

```bash
# 1. Install codex
npm install -g @openai/codex
codex login --device-auth          # complete the device-auth flow once

# 2. Verify the adapter is reachable end-to-end
JARVIS_JUDGE=codex jarvis judge probe
# → judge probe ok (~7s): agent=general confidence=0.95 ...

# 3. Route through codex
JARVIS_JUDGE=codex jarvis route "openwrt 编译报错 no rule to make target"
# → agent_type=coding (rule layer would have said devops),
#   router_notes contains both rule hint and judge reasoning

# 4. Chat REPL with codex (SLA auto-bumps to 180s for codex calls)
JARVIS_JUDGE=codex jarvis chat
```

Tunables:

| Env | Default | Purpose |
|---|---|---|
| `JARVIS_JUDGE` | unset (rule-only) | `codex` to enable the codex adapter |
| `CODEX_BINARY` | `codex` (PATH) | Override codex binary path |
| `CODEX_MODEL` | codex's default | Pin a specific model id |
| `CODEX_TIMEOUT_SECS` | `180` | Per-call hard deadline |
| `JARVIS_FALLBACK_SECS` | `180` (with codex), `2` (rule) | Chat REPL fallback budget |

If codex is unreachable, the Router transparently falls back to the
rule layer and marks `fallback_used=true` — the call never panics.

---

## Run the HTTP API

```bash
jarvis serve 127.0.0.1:7777                  # blocks; Ctrl+C to stop

# in another shell
curl -s http://127.0.0.1:7777/healthz
curl -s http://127.0.0.1:7777/dashboard/metrics | jq
curl -s http://127.0.0.1:7777/sessions/recent | jq
curl -s http://127.0.0.1:7777/conversation/sess_abc/ownership | jq

# Server-Sent Events stream of raw_event_log for a session
curl -N http://127.0.0.1:7777/sessions/sess_abc/stream

# Send a route request via API
curl -X POST http://127.0.0.1:7777/router/input \
  -H 'content-type: application/json' \
  -d '{"user_input":"openwrt 排错","session_id_hint":null}' | jq
```

A read-only dashboard at `http://127.0.0.1:7777/dashboard` polls the
metrics endpoint and exposes recent activity.

Full endpoint list:

```
GET  /healthz
GET  /dashboard           (HTML)
GET  /dashboard/metrics   (JSON)
POST /router/input
GET  /sessions/recent
GET  /sessions/:id
GET  /sessions/:id/messages
GET  /sessions/:id/stream                   (SSE — raw_event_log tail)
GET  /memory/:scope
POST /memory
GET  /raw-log/:session
GET  /trace/:trace_id
GET  /audit/:session
GET  /growth/{events,artifacts}
GET  /walkthrough/:session
POST /walkthrough/:doc_id/{approve,reject}
POST /steer
POST /interrupt
POST /maintenance/lint
GET  /conversation/:session/{ownership,sub-channels,activity,pending}
POST /conversation/:session/{reply,interrupt,steer}
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
jarvis-anthropic       5 tests
jarvis-api            17 tests   (+ Conversation API + SSE concurrency)
jarvis-cli            53 tests   (+ JSON variants + commands subcommand)
jarvis-codex           6 tests   (+ 6 #[ignore] real codex e2e)
jarvis-control        16 tests   (+ TurnSummary hook + judge path)
jarvis-core           13 tests
jarvis-db             28 tests
jarvis-growth         21 tests
jarvis-memory         44 tests
jarvis-openai          3 tests
jarvis-orchestrator   80 tests   (+ worker e2e + lock cross-proc + synthesizer)
jarvis-router         46 tests
jarvis-tools          18 tests
─────────────────────────────────
DEFAULT              352 tests   (0 failed)
IGNORED               +6 tests   (real codex CLI; requires codex login)
```

Each crate is independently testable: `cargo test -p jarvis-orchestrator`,
etc. Real codex e2e: `cargo test -p jarvis-codex --lib -- --ignored`.

---

## QA evidence

`docs/qa/v1.8/` contains a 30-feature audit of the v1.8 PRD with real
terminal transcripts (`docs/qa/v1.8/EVIDENCE.md` collects 50+ command
runs and `cargo test --nocapture` outputs end-to-end). `docs/qa/v1.8/INDEX.md`
maps each PRD section to its implementation status (✅ / 🟡 / ⏸️ / ❌ / 🖥️).

`docs/prd/v1.9-context-handoff.md` is the design spec for the next
version (CapacityAdvisor + AI-led HandoffSnapshot when the session
context outgrows its useful budget — to be implemented after v0.4).

`docs/eng/storage-tiering.md` is the v1.0 storage-tiering plan
(Hot 30d / Warm 180d zstd / Cold object store) referenced by code
TODOs in `raw_event_log.rs` and `scheduler.rs`.

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
