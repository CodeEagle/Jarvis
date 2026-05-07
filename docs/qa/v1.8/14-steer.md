# 14 — Steer 协议

**PRD 章节**：§9.11 Steer 协议 · §5.3a steer 模式 · §30.5 Steer 测试

**结论**：⏸️ 库层完整 + 单测全过；运行时注入链路（子 Agent 实际收到 Steer）未端到端打通

## 验收点（库层 / 单测）

- [x] §9.11.1 Steer vs 中断的语义区分
- [x] §9.11.2 SteerSignal 数据结构（scope direction/constraint/priority、inject_at 三档、状态机）
- [x] §9.11.3 注入流程：识别 → steer_queue → 立即确认用户 → 注入时机
- [x] §9.11.4 频率保护：60s 内最多 3 次（第 4 次拒绝）
- [x] §9.11.4 Steer 不允许覆盖 CONTEXT.md
- [x] §9.11.5 Codex Steer Adapter：只用 append 模式
- [x] §9.11.6 steer_signals 表 + Codex adapter
- [x] §15.7 Steer 写入 raw_event_log（审计）

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib steer
running 8 tests
test result: ok. 8 passed; 0 failed
```

| 用例 | PRD 对照 |
|---|---|
| `classify_steer_message` | §9.11.2 scope 分类（direction/constraint/priority） |
| `steer_admissibility_accepts_normal_constraint` | §9.11.4 合法约束 |
| `steer_admissibility_rejects_context_override_attempts` | §9.11.4 不允许覆盖 CONTEXT.md |
| `steer_admissibility_rejects_empty_content` | §9.11.2 内容非空 |
| `steer_first_three_accepted_then_throttled` | §9.11.4 60s 内最多 3 次 |
| `steer_status_transitions_pending_injected_acknowledged` | §9.11.2 状态机 |
| `steer_writes_to_raw_event_log` | §15.7 + §9.11.3 审计写入 |
| `steer_adapter_records_payload_in_append_mode` | §9.11.5 Codex adapter 只用 append 不 override |

## ⏸️ 缺口

- **运行时注入链路**：当前没有真正运行的子 Agent 在跑（§9 整个 Agent Runtime/Worker driver 还在 `crates/jarvis-orchestrator/src/sub_task.rs` 的 InProcessDriver 阶段），所以 `inject_at = next_step` 的实际触发没有 e2e 验证。`steer.rs` 库已能把 SteerSignal 写入 DB + 通知 adapter，但消费端是 in-process driver 的同步函数，没有"下一步前注入到 LLM 上下文"的真路径。
- **CLI**：`jarvis route` 路径已能识别 mention_mode=steer 并设置 `decision.override_action="steer"` + `steer_content`（[03-mention.md](./03-mention.md) 可验），但 `jarvis steer <session>` 命令缺失。
- **API**：PRD §23.2/23.3 列了 `POST /agents/:id/steer` 和 `POST /conversation/:id/steer`，HTTP 层没接。

## v0.3 路线图

PRD §24.3 v0.3 阶段才包含完整 Steer 协议（"SteerSignal + steer_queue + Codex adapter"）。当前 v1.8 已完成：
- ✅ adapter（codex append）
- ✅ steer_queue（DB 表 + 状态机）
- ✅ admissibility / throttle
- ✅ raw_event_log 审计
- ⏸️ "next_step 注入到运行子 Agent" — 需要 worker driver runtime
