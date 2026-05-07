# 28 — HTTP API

**PRD 章节**：§23 API 设计

**结论**：✅ 部分实现（dashboard / healthz / sessions / memories / SSE），剩余 endpoint 在 v0.2 / v0.3 路线图

## 验收点

- [x] §23.1 POST /router/input — Router 收到输入后**先写 raw_event_log**（见 [24-immutable-log.md](./24-immutable-log.md)）
- [x] §23 GET /healthz / dashboard / sessions / messages / memories / audit
- [x] §23 SSE 实时事件流（GET /events/stream）
- [ ] §23.2 Agent Runtime API（agents/start/run/cancel/...）— v0.2
- [ ] §23.3 Conversation API（ownership/interrupt/reply/steer）— v0.2/v0.3
- [ ] §23.4 Walkthrough & Verification API — v0.3
- [ ] §23.5 Growth API（events/artifacts/promote/reject/rollback）— v0.2
- [ ] §23.6 Memory & Dream API — v0.2
- [ ] §23.7 Commands API — v0.3
- [ ] §23.8 Tentacle 文件 API — v0.3
- [ ] §23.9 回溯查询 API — v0.4

## 单测

```text
$ cargo test -p jarvis-api --lib
running 10 tests
test result: ok. 10 passed; 0 failed
```

| 用例 | endpoint |
|---|---|
| `healthz_returns_ok_payload` | GET /healthz |
| `dashboard_metrics_returns_required_fields` | GET /dashboard |
| `dashboard_html_contains_known_tile_keys` | GET /dashboard.html |
| `recent_sessions_returns_array` | GET /sessions |
| `get_session_returns_404_when_missing` | GET /sessions/:id |
| `session_messages_returns_chronological` | GET /sessions/:id/messages |
| `list_memories_returns_empty_for_unused_scope` | GET /memory/:scope |
| `audit_returns_array` | GET /audit/:session_id |
| `sse_stream_emits_existing_raw_events_on_connect` | GET /events/stream（SSE） |
| `end_to_end_serve_and_client` | 启动 serve + client 调用 |

## CLI 启动

```bash
$ jarvis serve 127.0.0.1:7777
```

## §23.1 写入顺序保证

PRD §23.1 注释：「Router 收到用户输入后，在任何处理逻辑之前，首先将原始输入写入 raw_event_log」。

实现：`crates/jarvis-router/src/router.rs::route` 第一步 → raw_event_log::append（user_message）。被 `route_writes_raw_event_log_first` 单测覆盖（见 [24-immutable-log.md](./24-immutable-log.md)）。

## v0.2 / v0.3 路线图

PRD §24 列的所有 API 需要等到对应阶段：
- v0.2：Agent Runtime API + Memory API + Growth API
- v0.3：Conversation / Walkthrough / Commands / Tentacle API
- v0.4：Steer / 回溯查询 API

当前 v1.8 的 jarvis-api 主要满足 dashboard 可视化 + SSE 事件流 + 多设备同步基础。**预计随 macOS 桌面 m1 一并补**（macOS 客户端调用的就是这些 API）。
