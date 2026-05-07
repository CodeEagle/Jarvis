# 24 — 不可变日志（raw_event_log / memory_change_log / session_snapshots）

**PRD 章节**：§15.7 不可变追加日志 · §15.7.2/3/4/5/6

**结论**：✅ 通过

## 验收点

- [x] §15.7.2 raw_event_log：触发器拒绝 UPDATE / DELETE
- [x] §15.7.2 raw_event_log：seq 单调递增，checksum 防篡改
- [x] §15.7.2 raw_event_log：safe_content 自动脱敏
- [x] §15.7.3 memory_change_log：每次 Memory 写入自动记录 before/after JSON
- [x] §15.7.3 memory_change_log：触发器阻止 UPDATE
- [x] §15.7.4 session_snapshots：触发器阻止 UPDATE，seq 单调递增
- [x] §15.7.5 时间点回放（replay）/ Memory 溯源（provenance）/ trace 执行验证
- [x] §23.1 Router 写 raw_event_log 在路由处理之前

## 单测（28 项 jarvis-db 全过）

```text
$ cargo test -p jarvis-db --lib
running 28 tests, all pass
```

| 用例 | PRD 对照 |
|---|---|
| `raw_event_log_blocks_update` | §15.7.2 触发器拒绝 UPDATE |
| `raw_event_log_blocks_delete` | §15.7.2 触发器拒绝 DELETE |
| `raw_event_seq_is_monotonic` | §15.7.2 seq 单调递增 |
| `raw_event_checksum_verifies` | §15.7.2 checksum 完整性 |
| `raw_event_log_auto_populates_safe_content_for_secrets` | §15.7.2 + §15.6 含 secret 时自动 redact |
| `raw_event_log_leaves_safe_content_none_when_clean` | 干净内容不需要 safe_content |
| `memory_write_records_change_log` | §15.7.3 写入即记 change_log |
| `memory_update_records_before_and_after` | §15.7.3 before / after JSON 快照 |
| `audit_log_blocks_update` | §15.7 同样保护 audit |
| `audit_log_round_trip` | audit_log CRUD |
| `audit_log_filters_by_session_and_actor` | §15.5 audit 查询 |
| `outbox_delete_blocked` | outbox 同样不可删 |
| `outbox_update_to_other_columns_blocked` | outbox 字段保护 |
| `outbox_seq_strictly_monotonic` | outbox seq |
| `session_snapshot_blocks_update` | §15.7.4 |
| `session_snapshot_seq_monotonic_per_session` | §15.7.4 seq |
| `provenance_replay_returns_baseline_and_subsequent_events` | §15.7.5 时间点回放 |
| `provenance_trace_events_returns_in_order` | §15.7.5 trace 执行验证 |
| `redactor_masks_anthropic_api_key` | §15.6 脱敏 |
| `redactor_masks_authorization_header` | §15.6 脱敏 |
| `redactor_masks_email_and_phone` | §15.6 脱敏 |
| `fact_memory_emotion_is_forced_neutral_on_write` | §12.3a + §15.7.3 |

## SQL 触发器（PRD §15.7.2 / §15.7.3 / §15.7.4 完全实现）

`crates/jarvis-db/src/migrations.rs`：

```sql
CREATE TRIGGER prevent_raw_event_update
  BEFORE UPDATE ON raw_event_log
  BEGIN SELECT RAISE(ABORT, 'raw_event_log is immutable'); END;

CREATE TRIGGER prevent_raw_event_delete
  BEFORE DELETE ON raw_event_log
  BEGIN SELECT RAISE(ABORT, 'raw_event_log is immutable'); END;

CREATE TRIGGER prevent_memory_log_update
  BEFORE UPDATE ON memory_change_log
  BEGIN SELECT RAISE(ABORT, 'memory_change_log is immutable'); END;

CREATE TRIGGER prevent_snapshot_update
  BEFORE UPDATE ON session_snapshots
  BEGIN SELECT RAISE(ABORT, 'session_snapshots is immutable'); END;
```

## §15.7.5 回溯查询能力

`crates/jarvis-db/src/provenance.rs`：

| API | PRD §15.7.5 能力 |
|---|---|
| `replay_session_at(session_id, ts)` | 时间点回放 |
| `trace_events(trace_id)` | trace 执行验证 |
| `memory_history(memory_id)` | Memory 溯源链 |

CLI 暴露：
- `jarvis replay <session_id> [iso]`
- `jarvis trace <trace_id>` / `jarvis trace-view <trace_id>`
- `jarvis memory-history <mem_id>`
- `jarvis audit <session_id>`

## §23.1 写入顺序保证（Router 在处理前先写 raw_event_log）

`crates/jarvis-router/src/router.rs::route` 第一步：

```rust
let event = raw_event_log::append(&self.db, AppendEvent {
    event_type: RawEventKind::UserMessage,
    session_id, trace_id: Some(&trace_id), ...
})?;
// 上面这一步必须先成功，后续才有 routing 逻辑
```

单测：`crates/jarvis-router/src/tests.rs::route_writes_raw_event_log_first` 验证 raw_event_log 写入优先于任何分类逻辑。

## §15.7.6 分级存储（Hot/Warm/Cold）

❌ 当前所有日志一律 Hot 存 SQLite，没有 Warm/Cold 分级。属于 v1.0 产品化阶段任务（PRD §27.11 风险应对）。
