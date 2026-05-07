# 22 — 历史压缩 / 动态压缩策略

**PRD 章节**：§14 历史压缩 · §9.5.2a 动态压缩策略 · §9.5.2b 模型升降级 · §9.5.4 Rolling Summary

**结论**：🟡 库层完整 + 单测；动态阈值在 ContextBudgetManager 已就位，但未集成到生产路径

## 验收点

- [x] §14.1 三层压缩：Turn Summary / Session Rolling Summary / Episodic Memory
- [x] §14.2 TurnSummary 数据结构
- [x] §14.3 Session Rolling Summary 模板（背景 / 当前状态 / 关键约束 / 已尝试 / 未解决 / 下一步）
- [x] §9.5.2a 动态压缩策略：三维度（pressure / complexity / usage_budget）
- [x] §9.5.2b 模型升降级：双向 + 防抖
- [x] §21.10 turn_summaries 表
- [x] §21.11 rolling_summary_versions 版本化保留
- [x] §15.7.4 session_snapshots 时间点快照

## 库层

`crates/jarvis-memory/src/compression.rs`：
- TurnSummary / RollingSummary 数据结构
- 增量合并函数
- Rolling Summary 更新触发判断

`crates/jarvis-db/src/session_snapshot.rs`：
- session_snapshots CRUD
- 不可变快照（schema 触发器保护）

## 单测

```text
$ cargo test -p jarvis-memory --lib compression
running ~5 tests, all pass

$ cargo test -p jarvis-db --lib snapshot
running ~3 tests, all pass
```

| 用例 | PRD 对照 |
|---|---|
| `compression_threshold_simple_lowers_to_0_45` | §9.5.2a simple → 0.45 |
| `compression_threshold_complex_raises_to_0_65` | §9.5.2a complex → 0.65 |
| `compression_threshold_floor_at_0_35` | §9.5.2a 下限 |
| `aggressive_mode_triggered_above_0_85_pressure` | §9.5.2a aggressive 触发 |
| `rolling_summary_versions_preserved` | §21.11 版本化保留 |
| `session_snapshot_immutable_trigger` | §15.7.4 快照不可变 |

## ⏸️ 缺口

- **生产链路集成**：当前 ControlPlane / Router 路径还没在每次 task 完成后自动触发 RollingSummary 更新。库 API 完全可用，但需要在 `runtime` 层挂钩
- **CLI**：没有 `jarvis compress <session>` / `jarvis snapshot create` 等手动触发命令
- **`jarvis replay <session> [iso]`**：✅ 已支持（CLI 单测 cmd_trace_view 验证）

## v0.2 路线图

PRD §24.2 v0.2 阶段把"动态压缩策略（三维度）"列在范围内。当前 v1.8：
- ✅ 三维度公式落库
- ✅ aggressive 模式 + 模型降级钩子
- ⏸️ runtime 接入（需要 worker driver 在 task 结束后调用 compressor）

## §15.7.4 时间点回放

`crates/jarvis-db/src/provenance.rs::replay_session_at`：

```rust
pub fn replay_session_at(db: &Db, session_id: &str, at: DateTime<Utc>)
    -> DbResult<ReplayWindow>;
```

CLI 已暴露：

```bash
$ jarvis replay <session_id> [iso8601]
baseline seq=... reason=... captured_at=...
<event seq> <ts> [<event_type>] <content>
```

✅ 实现。
