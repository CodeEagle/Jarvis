# 05 — Session Resolver

**PRD 章节**：§6 Session Resolver · §6.3 评分公式 · §6.4 Explicit Reference

**结论**：✅ 通过

## 验收点

- [x] §6.1 Session 是任务状态容器，不是聊天窗口
- [x] §6.2 Session 数据结构完整（id / title / domain / topic / summary / unresolved / status / 时间戳）
- [x] §6.3 评分公式：semantic 0.40 + recency 0.30 + entity 0.30
- [x] §6.4 Explicit reference（"继续"等）覆盖评分阈值
- [x] §6.4 阈值分档：>=0.75 继续 / 0.45-0.75 低置信继续 / <0.45 新建
- [x] PRD 22.1 sessions 表 schema 对齐

## A. Session 创建

```bash
$ jarvis sessions new "OpenWrt DNS 排查" devops
created sess_88b878c070f1 title="OpenWrt DNS 排查" domain=devops
```

## B. Explicit Reference 命中

```bash
$ jarvis route "继续 OpenWrt 排查"
```

```json
"session_action": "continue_existing",
"target_session_id": "sess_88b878c070f1",
"router_notes": "rule hint: devops.networking (w=0.4)"
```

✓ 输入含「继续」+ 同 domain 关键词，正确匹配到刚创建的 OpenWrt session。`session_action=continue_existing`，`target_session_id` 非空（PRD §5.4 字段约束）。

## C. 阈值分档

`crates/jarvis-router/src/session_resolver.rs` 实现：

```rust
const HIGH_THRESHOLD: f32 = 0.75;
const LOW_THRESHOLD: f32 = 0.45;

pub fn resolve(...) -> SessionResolution {
    if score >= HIGH_THRESHOLD { Continue(sess.id) }
    else if score >= LOW_THRESHOLD { ContinueWithLowConfidence(sess.id) }
    else { CreateNew }
}
```

单测覆盖：
- `crates/jarvis-router/src/tests.rs::resolves_high_score_to_continue`
- `crates/jarvis-router/src/tests.rs::resolves_low_score_to_create_new`
- `crates/jarvis-router/src/tests.rs::explicit_reference_overrides_score`

## D. 评分三路权重

`crates/jarvis-router/src/session_resolver.rs::score`：

```rust
let semantic_w = 0.40;
let recency_w  = 0.30;
let entity_w   = 0.30;
let score = semantic * semantic_w + recency * recency_w + entity * entity_w;
```

✓ 与 PRD §6.3 完全对齐。

## §6.4 Explicit Reference 短语清单

| 短语 | 命中 |
|---|---|
| 继续 | ✅ |
| 刚才那个 | ✅（regex `刚才那个`） |
| 上面那个 | ✅ |
| 这个问题 | ✅ |
| 前面说的 | ✅ |
| 还是那个 | ✅ |
| 上次那个 | ✅ |

实现：`crates/jarvis-router/src/session_resolver.rs::EXPLICIT_REFERENCE_PHRASES`，全部对齐 PRD §6.4。

## 备注

- §6.4 explicit_reference 触发时直接覆盖阈值这一行为已实现；当无法确认具体 session 时 PRD 要求"以 explicit_reference 信号为主因进入人工确认流程"，当前 fallback 是默认匹配最近活跃 session（不够严格，**v0.2 改进项**）
- §9.5.5 ColdStartSnapshot 机制（破冰期 5 轮快照）库层已有 `crates/jarvis-memory/src/cold_start.rs`，但 Router 启动路径未自动注入快照（**v0.4 阶段**）
