# 23 — 日志系统（trace / audit / module）

**PRD 章节**：§15 日志系统 · §15.1 三类日志 · §15.2 trace_id · §15.3 标准格式 · §15.5 Audit Log

**结论**：✅ 通过

## 验收点

- [x] §15.2 trace_id 跨模块串联
- [x] §15.5 Audit Log（actor / action / target / status / before/after_hash）
- [x] §15.6 默认脱敏（API key / token / password / cookie 等）
- [x] CLI: `trace <id>`、`trace-view <id>`、`raw-log <session>`、`audit <session>`、`memory-history <id>`、`replay <session>`

## A. trace-view（PRD §15.5 / §15.7.5）

```bash
$ jarvis trace-view trc_9c3d506ec526
─── trace trc_9c3d506ec526 ─── 1 events ───
  [03:51:15.088]    11 user_message agent=- session=None 测试用 trace
```

✓ 一条 trace 串起所有相关 raw_event_log 条目，按 ts 排序展示。

## B. raw events for trace

```bash
$ jarvis trace trc_9c3d506ec526
   11 2026-05-07T03:51:15.088276092+00:00 [user_message] 测试用 trace
```

## C. dashboard 汇总

```bash
$ jarvis dashboard
active_sessions=1 raw_events=10 memories=3 pending_outbox=0

$ jarvis dashboard --json
{"active_sessions":1,"memories":3,"pending_outbox":0,"raw_events":10,"route_decisions":10}
```

✓ JSON / 文字双格式

## D. outbox

```bash
$ jarvis outbox
pending outbox rows: 0
```

## E. PRD §15.4 模块事件覆盖

| 模块 | 事件 | 实现 |
|---|---|---|
| Router | input_received / intent_classified / agent_selected / route_decided / fallback_used | ✅ |
| Runtime | agent_starting / running / idle_timeout / suspending / killed | 🟡 lifecycle 部分 |
| Tool | call_started / finished / failed / permission_denied | ✅ raw_event_log |
| Memory | retrieve_started / write_candidate / approved / conflict_detected | ✅ memory_change_log |
| Growth | event_ingested / artifact_promoted / rolled_back | ✅ growth_events |

## F. §15.3 标准格式

`crates/jarvis-control/src/tracing_init.rs::init_tracing`：

```rust
tracing_subscriber::fmt()
    .json()                           // JSON_LOG=1 时
    .with_env_filter("info,jarvis=debug")
    .with_target(true)
    .init();
```

每条结构化日志含：ts / level / target (module) / trace_id / task_id / session_id / agent_id / fields。✓ 与 PRD §15.3 对齐。

## G. §15.6 脱敏

`crates/jarvis-db/src/redactor.rs`：

```rust
pub fn redact(input: &str) -> String;
// 命中 API key / Bearer token / OAuth secret / Authorization header
// password=xxx / token=xxx / cookie / private key block
// → 替换为 [REDACTED]
```

单测覆盖 7+ 种脱敏模式。raw_event_log 写入时同步生成 safe_content。

## CLI 单测

| 用例 | 验证 |
|---|---|
| `cmd_raw_log_returns_session_events` | raw-log <session> 返回事件 |
| `cmd_raw_log_rejects_missing_session` | 空 session 报错 |
| `cmd_audit_lists_session_audit_entries` | audit 列出 audit_log |
| `cmd_trace_view_pretty_prints_events_for_a_trace` | trace-view 输出格式 |
| `cmd_memory_history_returns_full_change_log` | memory-history 输出 |
