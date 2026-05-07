# 27 — 并发与锁

**PRD 章节**：§20 并发与锁 · §20.3 Workspace Lock · §20.4 Tentacle 文件 Lock · §27.6 多 Agent 协作失败

**结论**：✅ 通过（Workspace Lock + Session 串行）

## 验收点

- [x] §20.1 同一 session 串行 / 不同 session 并行
- [x] §20.1 同一 workspace 写操作串行
- [x] §20.2 Lock 数据结构（resource / resource_id / owner / expires_at）
- [x] §20.3 Workspace Lock：同 workspace 互斥 / 不同 workspace 并行 / 过期自动释放
- [x] §20.4 Tentacle 文件 Lock：CONTEXT 写保护 / HANDOFF 一次性 / NOTES 追加（见 [15-tentacle.md](./15-tentacle.md)）

## 单测

`crates/jarvis-orchestrator/src/lib.rs` 包含 DurableWorkspaceLock + InProcessLockManager。

```text
$ cargo test -p jarvis-orchestrator --lib lock
test workspace_lock_blocks_same_resource_until_release ... ok
test workspace_lock_allows_different_resources_in_parallel ... ok
test workspace_lock_expires_automatically_after_ttl ... ok
test workspace_lock_durable_across_restart ... ok
test session_serializer_runs_tasks_in_fifo_order_per_session ... ok
test session_serializer_allows_parallel_across_sessions ... ok
```

| 用例 | PRD 对照 |
|---|---|
| `workspace_lock_blocks_same_resource_until_release` | §20.3 同 workspace 互斥 |
| `workspace_lock_allows_different_resources_in_parallel` | §20.3 不同 workspace 并行 |
| `workspace_lock_expires_automatically_after_ttl` | §20.3 expires_at 自动释放 |
| `workspace_lock_durable_across_restart` | 持久化 lock：进程重启后状态保留 |
| `session_serializer_runs_tasks_in_fifo_order_per_session` | §20.1 session 串行 |
| `session_serializer_allows_parallel_across_sessions` | §20.1 跨 session 并行 |

## §20.4 Tentacle Lock 规则（与 PRD 对齐）

| 文件 | Lock 行为 | 实现 |
|---|---|---|
| todo.md | 写 ≤ 5s；读不需要 | ✅ tick_step |
| NOTES.md | Agent 追加 ≤ 10s；Walkthrough 读不锁 | ✅ append_notes |
| HANDOFF.md | 一次性写入，已存在不允许覆盖 | ✅ write_handoff (one-shot) |
| CONTEXT.md | 写保护，子 Agent 无权 | ✅ tentacle_context_is_write_protected_for_subagents |

详见 [15-tentacle.md](./15-tentacle.md)。

## §27.6 多 Agent 协作失败

PRD 要求：
- PlannerAgent 超时 → 返回已生成 plan，降级单 Agent
- WorkerAgent 失败 → 标记失败子步骤，Router 决定重试或终止
- ResponseSynthesizer 冲突 → 以 ReviewerAgent 结论为准

实现现状：
- 🟡 InProcessDriver 层有 retry / failure 标记，单测 `worker_driver_failed_on_nonzero_exit` 等
- ⏸️ ResponseSynthesizer 的"冲突仲裁"机制未独立实现

## §22.3 工程目录对应

```text
src/
  locks/
    workspace-locker.ts
```

实际：`crates/jarvis-orchestrator/src/lib.rs::DurableWorkspaceLock`、InProcessLockManager。
