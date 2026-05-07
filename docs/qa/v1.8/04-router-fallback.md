# 04 — Router 降级策略

**PRD 章节**：§5.5 Router 降级 · §9.10.6 降级响应模式

**结论**：✅ 通过

## 验收点

- [x] §5.5 LLM judge 失败（subprocess error / 超时 / 解析错误）→ 路由继续，`fallback_used=true`
- [x] §5.5 router_notes 标注 fallback 原因
- [x] §5.5 不阻塞主流程：依然返回完整 RouteDecision
- [x] §5.5 Growth Engine 不可用时主流程降级（事件异步排队 / 跳过）
- [x] §9.10.6 ControlPlane 在 task_plane 不可用时仍走规则路径

## 实现机制

```rust
// crates/jarvis-router/src/router.rs::route_with_judge
match judge.judge(...).await {
    Some(out) if out.confidence > decision.confidence && !decision.mention_override => {
        // judge 接管：覆盖 agent_type / topic / session_action / confidence
    }
    None => {
        decision.fallback_used = true;
        decision.router_notes = format!("{} | judge unavailable, rule-only fallback", ...);
    }
    _ => {}
}
```

## 单测覆盖

`crates/jarvis-router/src/tests.rs`：

| 用例 | 行为 |
|---|---|
| `route_with_judge_uses_judge_when_more_confident` | judge confidence 高于规则层 → judge 接管 |
| `route_with_judge_marks_fallback_when_judge_returns_none` | judge 返回 None → fallback_used=true |
| `route_with_judge_respects_mention_override` | mention_override=true 时 judge 不能覆盖 agent_type |

`crates/jarvis-codex/src/tests.rs`：

| 用例 | 行为 |
|---|---|
| `judge_returns_none_on_missing_binary` | codex binary 不存在 → None |
| `judge_returns_none_on_nonzero_exit` | exit code != 0 → None |
| `judge_returns_none_on_garbage_output` | schema 解析失败 → None |
| `judge_times_out_and_returns_none` | timeout 触发 → None + kill_on_drop 杀子进程 |

全部 default-suite 通过，无 ignored。

## 实测：judge 强制失败时的输出

通过将 `CODEX_BINARY=/nonexistent/codex` 强制 codex 缺失：

```bash
$ JARVIS_JUDGE=codex CODEX_BINARY=/nonexistent/codex \
    jarvis route "openwrt 编译报错"
```

```json
"agent_type": "devops",          ← 退回规则层
"confidence": 0.8,                ← 规则层 confidence
"router_notes": "rule hint: devops.networking (w=0.4) | judge unavailable, rule-only fallback",
"fallback_used": true             ← 显式标记
```

✓ `fallback_used=true`、`router_notes` 含 fallback 原因、agent_type 退回规则层结果。

## §9.11 Steer 频率保护（同属降级）

`crates/jarvis-orchestrator/src/steer.rs`（库层）—— 60s 内同一 sub_task_id 超过 3 次 Steer 拒绝。CLI 还没有 steer 子命令，详见 [14-steer.md](./14-steer.md)。

## §27.1 路由错误纠正

PRD §27.1 要求保留 trace + 用户纠正进入 routing example。当前：
- ✅ 完整 trace 链（raw_event_log + audit_log + provenance）— 见 [24-immutable-log.md](./24-immutable-log.md)
- 🟡 用户纠正 → routing_example artifact：Growth Engine 已有 routing_example 类型支持（`crates/jarvis-growth/src/artifact.rs`），但用户纠正→事件转换的具体链路未打通到 CLI（v0.2 阶段）
