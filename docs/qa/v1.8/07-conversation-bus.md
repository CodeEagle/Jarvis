# 07 — ConversationBus

**PRD 章节**：§8.11 ConversationBus / 会话所有权 / 消息路由

**结论**：🟡 库层完整 + 单测，CLI 未暴露

## 验收点（库层 / 单测）

- [x] §8.11.2 ConversationBus 数据结构（owner / sub_channels / pending_user_messages）
- [x] §8.11.3 Ownership 状态机（listening / executing / waiting_user）
- [x] §8.11.4 用户消息路由 5 个 Step（中断 > Steer > waiting_user > 进度查询 > 相关性）
- [x] §8.11.5 子 Agent UserInteractionRequest 流程
- [x] §8.11.6 三级中断协议（软 / 硬 / 异步）
- [x] §8.11.7 SubTaskCheckpoint 暂停 / 恢复
- [x] §8.11.10 数据库表 conversation_ownerships / sub_channels / sub_task_checkpoints / pending_user_messages

## 库层入口

- `crates/jarvis-orchestrator/src/conversation_bus.rs`
- `crates/jarvis-orchestrator/src/interrupt.rs`
- `crates/jarvis-orchestrator/src/checkpoint.rs`
- `crates/jarvis-db/src/migrations.rs::330+`（四张表）

## 单测覆盖

```text
$ cargo test -p jarvis-orchestrator --lib
running 69 tests
test result: ok. 69 passed; 0 failed
```

含 ownership 转让 / 软中断 checkpoint / 硬中断保留 Artifact / pending message 排队等。

## CLI / API 缺口

当前 `jarvis chat` REPL 调用的是 `ControlPlane::handle_user_input` → `Router::route`，**没有走 ConversationBus 完整链路**（多 Agent / 中断 / Steer 都不在 chat REPL 上）。

PRD §23.3 列出的 Conversation API：

| endpoint | 状态 |
|---|---|
| GET /conversation/:session_id/ownership | ❌ 无 |
| POST /conversation/:session_id/interrupt | ❌ 无 |
| GET /conversation/:session_id/activity | 🟡（activity-cards CLI 等价） |
| POST /conversation/:session_id/reply | ❌ 无 |
| POST /conversation/:session_id/steer | ❌ 无 |

**结论**：库层 + 测试到位，HTTP API + CLI 是 v0.2/v0.3 的工程任务（PRD §24.2/§24.3 路线图列在该阶段）。

## 备注

`crates/jarvis-api` 提供了 SSE + 基础 HTTP，但目前主要服务 dashboard，`/conversation/*` 路由还没接。等到 macOS 桌面 m1 阶段会一并补。
