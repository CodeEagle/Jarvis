# 13 — Control Plane / Task Plane / Watchdog

**PRD 章节**：§9.10 主 Agent 永远响应 · §9.10.2 / 9.10.3 / 9.10.4 / 9.10.5 / 9.10.6

**结论**：✅ 通过

## 验收点

- [x] §9.10.2 Control Plane / Task Plane 分离（控制面常驻 + 任务面可重启）
- [x] §9.10.3 SLA：fallback_ack ≤ 2000ms（默认 2s）/ 自定义可调
- [x] §9.10.4 子 Agent Watchdog：heartbeat / 超时 / grace period
- [x] §9.10.5 主 Agent 自身崩溃自愈（重建 Task Tree + Artifact Registry）
- [x] §9.10.6 Task Plane 不可用 → 降级响应（轻量模式）
- [x] handle_user_input + handle_user_input_with_judge 两条路径

## 单测

```text
$ cargo test -p jarvis-control --lib
running 14 tests, all pass
```

| 用例 | PRD 对照 |
|---|---|
| `control_plane_returns_resolved_for_simple_input` | §9.10.2 normal flow |
| `control_plane_falls_back_when_budget_too_tight` | §9.10.3 fallback_ack 触发 |
| `control_plane_with_judge_takes_judge_outcome` | §9.10.2 judge 路径 |
| `fallback_message_executing_mentions_running_agent_when_known` | §9.10.3 fallback 文案含当前活跃 Agent |
| `fallback_message_unavailable_mentions_lightweight_mode` | §9.10.6 降级模式提示 |
| `fallback_message_idle_short` | §9.10.3 标准兜底文案 |
| `watchdog_fresh_beat_is_healthy` | §9.10.4 healthy 状态 |
| `watchdog_marks_stale_then_dead_after_grace` | §9.10.4 超时升级 stale → dead |
| `watchdog_recovery_resets_stale` | §9.10.4 heartbeat 恢复后状态复位 |
| `watchdog_unknown_agent_is_dead` | §9.10.4 unknown agent 默认 dead |
| `replicator_drains_pending_rows_to_peer` | 跨设备同步（v0.4 task） |
| `replicator_stops_on_peer_error_without_marking_delivered` | replicator 错误处理 |
| `scheduler_config_defaults_24h_lint` | §12.6 / §16 daily Dream lint 调度 |
| `maintenance_jobs_run_lint_synchronously` | maintenance 接口 |

## §9.10.3 SLA 默认值

`crates/jarvis-control/src/sla.rs::ResponseSla::defaults`：

```rust
const fn defaults() -> Self {
    Self {
        interrupt_ack: Duration::from_millis(500),
        progress_query: Duration::from_millis(800),
        sub_agent_reply: Duration::from_millis(1000),
        fallback_ack: Duration::from_millis(2000),
    }
}
```

✓ 与 PRD §9.10.3 全部对齐（500/800/1000/2000ms）。

## §9.10.4 WatchdogPolicy 默认值

`crates/jarvis-control/src/watchdog.rs`：

```rust
const DEFAULT: WatchdogPolicy = WatchdogPolicy {
    heartbeat_interval_ms: 5_000,
    heartbeat_timeout_ms: 20_000,
    grace_period_ms: 10_000,
    auto_retry: true,
    max_retries: 2,
};
```

✓ 与 PRD §9.10.4 完全对齐。

## §9.10.6 降级响应

`crates/jarvis-control/src/fallback.rs::fallback_message`：

```rust
match state {
    Stuck => "后台任务似乎遇到了问题，我来检查一下..."
    Executing => "正在后台处理，[当前活跃助手] 还在运行中"
    Unavailable => "当前系统处于轻量模式..."  // §9.10.6
}
```

## CLI 接口

`jarvis chat` 走 ControlPlane。带 `JARVIS_JUDGE=codex` 时 SLA 自动放宽到 180s（避免 codex 13s 调用被 2s 兜底触发，由 main.rs::chat_repl 处理）。

```bash
$ JARVIS_JUDGE=codex jarvis chat
Jarvis v0.1 — chat REPL [judge=codex]. Type :quit to exit.
> ...
```
