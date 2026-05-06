# Jarvis — Global AI Runtime (Rust)

Rust implementation of the **Jarvis Global AI Runtime** PRD (v1.8 spec).
This repo currently ships the **v0.1 MVP foundation**: rule-layer Router,
SessionResolver, immutable raw event log, Memory + change-log, Control
Plane response SLA + sub-agent watchdog, plus a CLI to drive it all.

The PRD describes a much larger system (Orchestrator, Walkthrough/
Verifier, Dream, Steer, Tentacle files, Growth Engine, Codex driver, …).
Those land in v0.2 / v0.3 / v0.4 / v1.0 — see [Roadmap](#roadmap) below.

---

## Workspace layout

```
crates/
  jarvis-core/      Pure domain types (RouteDecision, Memory, Session …)
  jarvis-db/        SQLite storage; immutable triggers; raw_event_log;
                    memory_change_log; session_repo; memory_repo
  jarvis-router/    Main Router: rule-layer IntentClassifier, @mention
                    parser, SessionResolver scoring, agent registry
  jarvis-memory/    MemoryManager (write rules + change-log atomicity),
                    Jaccard-based retrieval + emotion resonance, trust score
  jarvis-control/   Control Plane / Task Plane separation, response SLA,
                    sub-agent Watchdog, fallback responder
  jarvis-cli/       `jarvis` binary — route / chat / memory / raw-log
```

Each crate has its own unit-test module. **71 tests pass** across the
workspace as of the current commit.

---

## Quickstart

```bash
# build
cargo build --release

# unit tests (everything)
cargo test --workspace

# route a single input through the rule layer
JARVIS_DB=./jarvis.db ./target/release/jarvis route "OpenWrt DNS hosts 不生效"

# interactive REPL through the Control Plane (with response SLA)
./target/release/jarvis chat

# record a long-term preference
./target/release/jarvis memory write "用户偏好函数式风格"

# inspect immutable raw event log for a session
./target/release/jarvis raw-log sess_xxxx
```

---

## What v0.1 actually does

The PRD's non-negotiable v0.1 invariants (Section 24.1) are all live:

| Invariant | Where |
|---|---|
| Raw input written to `raw_event_log` *before* any classification | `jarvis-router/src/router.rs::Router::route` Step 0 |
| `raw_event_log` blocks `UPDATE` / `DELETE` via SQL triggers | `jarvis-db/src/migrations.rs`, tested in `jarvis-db/src/tests.rs` |
| Each row carries a `sha256(ts \| event_type \| raw_content)` checksum | `jarvis-db/src/raw_event_log.rs::compute_checksum` |
| Memory writes are atomic with `memory_change_log` | `jarvis-db/src/memory_repo.rs::upsert` (single transaction) |
| Control Plane response SLA — fallback ≤ 2000ms | `jarvis-control/src/control_plane.rs::handle_user_input` |
| Sub-agent Watchdog with stale → grace → dead state machine | `jarvis-control/src/watchdog.rs` |
| Control Plane / Task Plane separation (Task Plane = `spawn_blocking`) | `jarvis-control/src/control_plane.rs` |
| `@mention` parsing with `mentionable=false` for internal agents | `jarvis-router/src/mention.rs`, `jarvis-router/src/agent_registry.rs` |
| Emotion-coordinate gate — fact/preference forced neutral | `jarvis-core/src/memory.rs::Memory::enforce_emotion_gate` |
| Tier-1 trust-score floor ≥ 0.30, retrieve-boost cap ≤ 0.20 | `jarvis-memory/src/trust.rs::compute` |

### Router decision flow

```
user input
  ↓
[Step 0]  raw_event_log::append           — survives any downstream panic
  ↓
[Step 0a] @mention parse + mention_log    — single / multi / steer / unresolved
  ↓
[Step 1]  rule layer apply_rules          — keyword → IntentHint(weight)
  ↓
[Step 2]  SessionResolver                 — explicit_reference > scored top
  ↓
[Step 3]  agent selection                 — @mention overrides intent match
  ↓
RouteDecision (+ RouterDiagnostics)
```

The LLM judgment layer described in PRD §5.4 is intentionally **left as
a seam** — `Router::route` consumes only rule hints today. When v0.2
plugs in an LLM, the rule layer's `IntentHint` list becomes the prompt
input and `RouteDecision` stays the same, so callers don't change.

---

## Test inventory

```
jarvis-core      13 tests   ID prefixes, domain parsing, emotion gate,
                            trust-score signatures, ToolScope semantics
jarvis-db        11 tests   raw_event_log immutability (UPDATE/DELETE),
                            checksum verification, atomic memory write,
                            change-log before/after snapshots, session repo
jarvis-memory    11 tests   write rules per source_type, trust decay,
                            high-emotion slow decay, Tier-1 floor,
                            retrieve-boost cap, hybrid retrieval, emotion
                            resonance (negative→positive bonus, low-energy
                            no-trigger, ≤0.15 cap)
jarvis-router    27 tests   intent rules (incl. creative.icon vs design),
                            session score weights/thresholds, recency
                            decay, @mention single/multi/alias/unresolved/
                            internal-not-mentionable, full Router pipeline
                            including raw_event_log ordering, mention
                            override, multi → orchestrator, steer detection,
                            explicit_reference continues old session,
                            confidence < 1.0
jarvis-control    9 tests   Watchdog healthy/stale/dead/recovery,
                            fallback message variants, ControlPlane
                            resolved + budget-tight fallback
─────────────────
TOTAL            71 tests
```

Run any individually:

```bash
cargo test -p jarvis-router route_writes_raw_event_log_first
cargo test -p jarvis-db   raw_event_log_blocks_update
```

---

## Storage

SQLite (bundled). `WAL` journal + `recursive_triggers = ON` so the
immutability triggers fire from any cascade.

The schema is in `jarvis-db/src/migrations.rs` and applied lazily on
`Db::open` / `Db::in_memory`. Tables:

- **Mutable**: `sessions`, `messages`, `memories`, `routing_examples`,
  `mention_log`
- **Immutable (triggers)**: `raw_event_log`, `memory_change_log`,
  `session_snapshots`

For tests we use `Db::in_memory()` which still installs all triggers.

---

## Roadmap

The PRD's 5-phase roadmap (Section 24) maps onto crates as follows. v0.1
is **done**; later phases are tracked but **not implemented**.

### v0.1 — shipped (this commit)
- Router rule layer, SessionResolver, raw_event_log, Memory + change-log,
  Control Plane / Task Plane split, Watchdog, CLI

### v0.2 — Growth Engine + Orchestrator basics
- New crate `jarvis-growth`: Collector, Evaluator, Extractor,
  PromotionGate, Regression Runner (mock-tool replay)
- Orchestrator path inside `jarvis-router`: TaskTree + ArtifactRegistry,
  ConversationBus with ownership state machine, Tentacle file generator
- Dynamic compression policy (three-dim threshold)
- Tentacle file Lock (Section 20.4)

### v0.3 — Walkthrough + Verifier + Steer
- New crate `jarvis-orchestrator`: WalkthroughAgent, VerifierAgent,
  RegressionOrchestrator, CompareAgent
- New crate `jarvis-tools`: tool runtime with permission & confirmation
- Steer protocol: `SteerSignal`, `steer_queue`, Codex steer adapter
- Soft / hard / async interrupt protocol with `SubTaskCheckpoint`

### v0.4 — Memory deepening + cost optimization
- Dream system (lint + cluster + inference) inside `jarvis-memory`
- Emotion coordinates + emotion resonance retrieval (already wired in
  `Retrieval::retrieve`; the Dream system populates the values)
- Dynamic model up/downgrade (haiku ↔ sonnet ↔ opus) with debouncing
- token-budget self-learning (`preferred_context_budget`)
- Persona layer (`persona.md` + `user.md` injected into stable layer)

### v1.0 — Productization
- Growth Dashboard, multi-device sync, Qdrant / pgvector upgrade,
  MCP / plugin registry, Trace Viewer, sandboxed tool runtime.

---

## Design choices that aren't obvious from the PRD

1. **`rusqlite` not leaked through `Router`.** The router talks to
   storage only via repo functions (`raw_event_log::append`,
   `mention_log::append`, …) so swapping SQLite for Postgres later
   doesn't touch routing logic.
2. **Hybrid score is one path today.** PRD §13.1 specifies
   FTS5 + vector + Jaccard; v0.1 ships only Jaccard. The function
   shape `(jaccard * w) * (0.7 + trust * 0.3) + emotion_bonus` is
   already the planned final form, with extra retrievers folded into
   the parenthesised expression — no API churn when they arrive.
3. **`Db` is `Arc<Mutex<Connection>>`.** `rusqlite` isn't `Sync` and
   Jarvis is single-machine; the mutex isn't a real bottleneck given
   SQLite's own write serialization. If/when we move to a connection
   pool, `Db` is the only thing that has to change.
4. **Control Plane = `tokio::time::timeout` + `spawn_blocking`.** The
   PRD demands separation so a stuck Task Plane can never block user
   responses. With sync rusqlite + a fallback budget, this is the
   simplest correct shape; we can promote the Task Plane to a real
   subprocess later without changing the call site.
5. **Internal agents (Jarvis / Verifier / Walkthrough / Memory) are
   `mentionable: false`.** PRD §5.3a §8.12.2. Keeping this in the
   `AgentDefinition` rather than as a separate list makes the registry
   the single source of truth for both routing and `@mention` resolution.

---

## License

MIT.
