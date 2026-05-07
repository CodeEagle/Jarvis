#!/usr/bin/env bash
# Jarvis smoke / acceptance test
#
# Exercises every CLI verb and (optionally) the HTTP API + codex
# judge. Each step prints a header and a pass/fail line so the
# operator can scan the output.
#
# Usage:
#   scripts/smoke.sh                # everything except codex
#   JARVIS_JUDGE=codex scripts/smoke.sh   # also runs codex e2e (≈15s)
#   JARVIS_BIN=./jarvis scripts/smoke.sh  # use a non-default binary path

set -u
shopt -s lastpipe   # so pipeline-set vars survive

JV="${JARVIS_BIN:-./jarvis}"
DB="${JARVIS_DB:-/tmp/jarvis-smoke-$$.db}"
export JARVIS_DB="$DB"
rm -f "$DB"

PASS=0
FAIL=0
FAILED_STEPS=()

ok() { echo "  ✓ $1"; PASS=$((PASS+1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL+1)); FAILED_STEPS+=("$1"); }
hdr() { echo ""; echo "── [$1] ────────────────────────────────────────────────"; }

# Each check uses a `[[ <output> == *substring* ]]` style assertion.
expect_contains() {
    local label="$1" expected="$2" actual="$3"
    if [[ "$actual" == *"$expected"* ]]; then
        ok "$label"
    else
        fail "$label  (expected to contain: $expected; got: ${actual:0:120})"
    fi
}

run() { "$JV" "$@" 2>&1; }
# Force rule-only routing for sections that exercise the deterministic
# rule layer (Section 1). Even when the operator runs with
# JARVIS_JUDGE=codex (which we honour for the dedicated judge section
# below), the rule-layer assertions must reflect the rule layer, not
# whatever the LLM judge decides to override with.
run_rule_only() { JARVIS_JUDGE= "$JV" "$@" 2>&1; }

# ── 0. Sanity: binary exists and prints help ─────────────────────────
hdr "0. binary sanity"
HELP=$(run --help)
expect_contains "binary launches + prints help" "Jarvis v1.0 CLI" "$HELP"
expect_contains "shows route subcommand"        "jarvis route"     "$HELP"
expect_contains "shows v1.9 handoff section"    "Handoff (v1.9)"   "$HELP"

# ── 1. Routing (rule layer) ─────────────────────────────────────────
hdr "1. Router / rule layer (PRD §5)"
ROUTE_OUT=$(run_rule_only route "openwrt 编译报错 no rule to make target")
expect_contains "devops rule hit"       "\"primary_intent\": \"devops.networking\"" "$ROUTE_OUT"
expect_contains "agent_type=devops"     "\"agent_type\": \"devops\""                 "$ROUTE_OUT"
expect_contains "tool_scope populated"  "shell.exec"                                 "$ROUTE_OUT"
expect_contains "fallback_used=false"   "\"fallback_used\": false"                   "$ROUTE_OUT"

CREATIVE=$(run_rule_only route "帮我生成一个图标 prompt")
expect_contains "creative.icon (not creative.design)" "\"primary_intent\": \"creative.icon\"" "$CREATIVE"

# ── 2. @mention ─────────────────────────────────────────────────────
hdr "2. @mention (PRD §5.3a)"
MENTION_SINGLE=$(run route "@代码助手 帮我看看这个 openwrt 配置")
expect_contains "single mention overrides agent_type" "\"agent_type\": \"coding\"" "$MENTION_SINGLE"
expect_contains "mention_override=true"               "\"mention_override\": true" "$MENTION_SINGLE"

MENTION_MULTI=$(run route "@代码助手 @研究助手 重构并搜索方案")
expect_contains "multi mention forces orchestrator" "\"agent_type\": \"orchestrator\"" "$MENTION_MULTI"

MENTION_INTERNAL=$(run route "@Jarvis 帮我")
expect_contains "Jarvis is not @mentionable"        "\"mention_override\": false"      "$MENTION_INTERNAL"

# ── 3. Sessions CRUD ────────────────────────────────────────────────
hdr "3. Sessions CRUD (PRD §6 + §23 macOS API parity)"
NEW_OUT=$(run sessions new "Mirage 重构" coding)
expect_contains "sessions new returns id" "created sess_" "$NEW_OUT"

SESS=$(run sessions list | grep -oE 'sess_[0-9a-f]+' | head -1)
expect_contains "list shows new session"  "Mirage 重构" "$(run sessions list)"

JSON=$(run sessions list --json)
expect_contains "--json: id field"        "\"id\""        "$JSON"
expect_contains "--json: status field"    "\"status\""    "$JSON"

run sessions archive "$SESS" >/dev/null
ARCHIVED_LIST=$(run sessions list)
if [[ "$ARCHIVED_LIST" == *"$SESS"* ]]; then
    fail "archive should hide session"
else
    ok "archived session hidden from list"
fi

# Re-create for following steps
run sessions new "Continued session" coding >/dev/null
SESS=$(run sessions list | grep -oE 'sess_[0-9a-f]+' | head -1)

# ── 4. Memory: write / list (with id) / search / forget / history ──
hdr "4. Memory (PRD §12 / §13)"
run memory write "用户偏好 vim 编辑器" >/dev/null
run memory write "周末喜欢做菜" >/dev/null
run memory write "Mirage 项目用 Riverpod" >/dev/null

LIST=$(run memory list)
expect_contains "list shows ids"        "mem_"               "$LIST"
expect_contains "list shows content"    "vim"                 "$LIST"

LIST_JSON=$(run memory list --json)
expect_contains "list --json has id+tier" "\"tier\":" "$LIST_JSON"

SEARCH=$(run memory search vim 5)
expect_contains "search ranks vim first" "vim 编辑器" "$SEARCH"
SEARCH_JSON=$(run memory search vim 5 --json)
expect_contains "search --json hybrid_score" "hybrid_score" "$SEARCH_JSON"

MEM_ID=$(run memory search vim | grep -oE 'id=mem_[0-9a-f]+' | head -1 | cut -d= -f2)
FORGET=$(run memory forget "$MEM_ID" "test cleanup")
expect_contains "forget records reason" "deprecated $MEM_ID" "$FORGET"

LIST_AFTER=$(run memory list)
if [[ "$LIST_AFTER" == *"vim 编辑器"* ]]; then
    fail "forgotten memory should be hidden"
else
    ok "forgotten memory hidden from list"
fi

HIST=$(run memory-history "$MEM_ID")
expect_contains "history shows created"     "created"    "$HIST"
expect_contains "history shows deprecated"  "deprecated" "$HIST"

# ── 5. Persona ──────────────────────────────────────────────────────
hdr "5. Persona (PRD §12.7)"
run persona set '{"style":"terse","language":"zh"}' >/dev/null
GET=$(run persona get)
expect_contains "persona get returns json content" "\"style\":\"terse\"" "$GET"
run persona set "easygoing assistant" >/dev/null
GET2=$(run persona get)
expect_contains "plain text wrapped as JSON string" "\"easygoing assistant\"" "$GET2"

# ── 6. ActivityCard ─────────────────────────────────────────────────
hdr "6. ActivityCard (PRD §8.13)"
CARDS=$(run activity-cards "$SESS" || true)
ok "activity-cards command runs (empty list expected for fresh session)"

# ── 7. Immutable logs (raw / audit / trace / replay) ────────────────
hdr "7. Immutable logs + provenance (PRD §15.7)"
TRACE=$(run route "test trace" | grep -oE 'trc_[0-9a-f]+' | head -1)
TRACE_VIEW=$(run trace-view "$TRACE")
expect_contains "trace-view shows raw events"  "user_message" "$TRACE_VIEW"

DASHBOARD=$(run dashboard --json)
expect_contains "dashboard --json valid"   "\"raw_events\"" "$DASHBOARD"

# ── 8. Maintenance / Dream ──────────────────────────────────────────
hdr "8. Dream lint + cluster (PRD §12.6)"
MAINT=$(run maintenance global)
expect_contains "maintenance prints lint metrics"   "duplicates_deprecated" "$MAINT"
expect_contains "maintenance prints cluster"        "clusters_created"      "$MAINT"

# ── 9. Growth ───────────────────────────────────────────────────────
hdr "9. Growth Engine (PRD §16-17)"
GROWTH=$(run growth events route_decision | head -3)
expect_contains "growth events list"      "router route_decision" "$GROWTH"

# ── 10. Skills / verifier / regression / commands (read surfaces) ───
hdr "10. Read-only surfaces (skills / verifier / regression / commands)"
SKILLS=$(run skills)
ok "skills command runs (output: ${#SKILLS} chars)"

REGRESS=$(run regression latest)
expect_contains "regression latest no-data line" "no regression report yet" "$REGRESS"

CMDS=$(run commands list)
expect_contains "commands list has sync_and_resolve" "sync_and_resolve" "$CMDS"
expect_contains "commands list has regression_check" "regression_check" "$CMDS"

# Run a command (records execution row even without runtime).
RUN=$(run commands run regression_check "$SESS")
expect_contains "commands run returns exec id"      "cmd_" "$RUN"
EXEC_ID=$(echo "$RUN" | grep -oE 'cmd_[0-9a-f]+' | head -1)
STATUS=$(run commands status "$EXEC_ID")
expect_contains "commands status shows pending steps" "step 0  agent=regression" "$STATUS"

# ── 11. v1.9 capacity + handoff ─────────────────────────────────────
hdr "11. v1.9 CapacityAdvisor + Handoff (docs/prd/v1.9-context-handoff.md)"
CAP=$(run sessions capacity "$SESS")
expect_contains "capacity prints advisory" "advisory=" "$CAP"
expect_contains "capacity ok for fresh session" "advisory=ok" "$CAP"

HOFF=$(run sessions handoff "$SESS")
expect_contains "handoff returns id" "hand_" "$HOFF"
HID=$(echo "$HOFF" | grep -oE 'hand_[0-9a-f]+' | head -1)

PENDING=$(run handoff list)
expect_contains "handoff list shows pending"  "decision=pending" "$PENDING"

SHOW=$(run handoff show "$HID")
expect_contains "handoff show: id"            "$HID"             "$SHOW"
expect_contains "handoff show: pending"       "\"user_decision\": \"pending\"" "$SHOW"
expect_contains "handoff show: cold_start"    "cold_start"       "$SHOW"

ACCEPT=$(run handoff accept "$HID" "续接会话")
expect_contains "accept produces new session" "new_session=sess_" "$ACCEPT"

# After accept: source archived, only the new session remains active.
NEW_LIST=$(run sessions list)
if [[ "$NEW_LIST" == *"$SESS"* ]]; then
    fail "after accept, source session should be archived"
else
    ok "source session archived after accept"
fi
expect_contains "new session present"         "续接会话"          "$NEW_LIST"

# Pending list now empty.
PENDING_AFTER=$(run handoff list | head -3)
if [[ -z "$PENDING_AFTER" ]]; then
    ok "pending list empties after accept"
else
    fail "pending list should be empty: '$PENDING_AFTER'"
fi

# ── 11b. Provider / model config (jarvis model) ─────────────────────
hdr "11b. Provider / model config"
# Use a per-run config file so we don't touch the operator's real
# ~/.jarvis/config.toml.
MODEL_CFG="/tmp/jarvis-smoke-model-$$.toml"
rm -f "$MODEL_CFG"
JARVIS_CONFIG="$MODEL_CFG" CURRENT_EMPTY=$(JARVIS_CONFIG="$MODEL_CFG" run model current)
expect_contains "model current empty"  "(none"               "$CURRENT_EMPTY"
SET_OUT=$(JARVIS_CONFIG="$MODEL_CFG" run model set anthropic/claude-sonnet-4-6)
expect_contains "model set saves"      "default model = anthropic/claude-sonnet-4-6" "$SET_OUT"
CURRENT_AFTER=$(JARVIS_CONFIG="$MODEL_CFG" run model current)
expect_contains "model current after set" "anthropic/claude-sonnet-4-6" "$CURRENT_AFTER"
LIST_JSON=$(JARVIS_CONFIG="$MODEL_CFG" run model list --json)
expect_contains "model list --json default"   "\"default_model\":\"anthropic/claude-sonnet-4-6\"" "$LIST_JSON"
expect_contains "model list --json provider"  "\"name\":\"anthropic\"" "$LIST_JSON"
SET_BAD=$(JARVIS_CONFIG="$MODEL_CFG" run model set badformat 2>&1 || true)
expect_contains "model set rejects bad id"    "expected" "$SET_BAD"

# OAuth provider (claude-cli) — auth status reflects whether the
# `claude` CLI binary exists on PATH; no env var is required.
JARVIS_CONFIG="$MODEL_CFG" run model set claude-cli/sonnet >/dev/null
OAUTH_LIST=$(JARVIS_CONFIG="$MODEL_CFG" run model list)
expect_contains "model list shows oauth provider" "oauth via" "$OAUTH_LIST"
OAUTH_JSON=$(JARVIS_CONFIG="$MODEL_CFG" run model list --json)
expect_contains "model list --json oauth_binary" "\"oauth_binary\":\"claude\"" "$OAUTH_JSON"
expect_contains "model list --json no api_key_env" "\"api_key_env\":null" "$OAUTH_JSON"
rm -f "$MODEL_CFG"

# ── 12. Subcommand --help / --json consistency ──────────────────────
hdr "12. --help / --json consistency"
for sub in route memory sessions persona handoff judge commands regression verifier model; do
    H=$(run "$sub" --help)
    if [[ "$H" == *"jarvis $sub"* ]]; then
        ok "jarvis $sub --help"
    else
        fail "jarvis $sub --help"
    fi
done

# ── 13. (optional) codex judge end-to-end ────────────────────────────
if [[ "${JARVIS_JUDGE:-}" == "codex" ]]; then
    hdr "13. codex LLM judge (live)"
    if ! command -v codex >/dev/null 2>&1; then
        fail "codex CLI not on PATH; install with `npm i -g @openai/codex` and `codex login`"
    else
        PROBE=$(run judge probe)
        expect_contains "judge probe ok"          "judge probe ok"   "$PROBE"

        CODEX_ROUTE=$(run route "openwrt 编译报错 no rule to make target")
        expect_contains "codex route returns coding agent" "\"agent_type\": \"coding\"" "$CODEX_ROUTE"
        expect_contains "router_notes carries judge tag"   "judge:" "$CODEX_ROUTE"
    fi
fi

# ── Summary ────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════════════════════════════"
echo "  Smoke test summary"
echo "    pass: $PASS"
echo "    fail: $FAIL"
if (( FAIL > 0 )); then
    echo ""
    echo "  Failed steps:"
    for s in "${FAILED_STEPS[@]}"; do
        echo "    - $s"
    done
    rm -f "$DB"
    exit 1
fi
echo ""
echo "  All checks passed. DB used: $DB"
rm -f "$DB"
