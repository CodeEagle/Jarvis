# 08 — ActivityCard / 协作面板

**PRD 章节**：§8.13 协作面板 · §21 activity_cards 表

**结论**：✅ 库层 + DB + CLI 列表全到位，🖥️ 真协作面板等 GUI

## 验收点

- [x] §8.13.2 ActivityCard 完整字段（agent_type / status / title / current_action / progress_text / interaction / artifacts / mention_override / interrupt_cost）
- [x] §8.13.4 状态枚举：pending / running / waiting_user / success / failed / suspended
- [x] §8.13.5 ActivityEvent → ActivityCard template 渲染
- [x] §8.13.7 activity_cards 表 + idx_activity_cards_session
- [x] v1.8 §8.13.3 mention_override 字段持久化
- [x] CLI: `jarvis activity-cards <session>` 列出该 session 卡片

## 库层

`crates/jarvis-orchestrator/src/activity_card.rs`：

```rust
pub struct ActivityCardStore { ... }
impl ActivityCardStore {
    pub fn create(&self, draft: CardDraft) -> ... ;
    pub fn upsert(&self, card: &ActivityCard) -> ... ;
    pub fn update_status(&self, id: &str, status: CardStatus) -> ... ;
    pub fn list_for_session(&self, session_id: &str) -> ... ;
}
```

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib activity_card
running 2 tests
test tests::activity_card_mention_override_persists ... ok
test tests::activity_card_lifecycle ... ok
test result: ok. 2 passed; 0 failed
```

`activity_card_lifecycle` 覆盖创建 → running → waiting_user → success 状态机。
`activity_card_mention_override_persists` 验证 v1.8 新增字段（@mention 触发的卡片标记）。

## CLI

```bash
$ jarvis activity-cards sess_8ad5b55e0789
(empty — 当前 session 还没产生 ActivityCard，因为 chat 没走 Orchestrator 链路)
```

CLI 路径：`crates/jarvis-cli/src/cmd.rs::cmd_activity_cards` 通过 `ActivityCardStore::list_for_session` 实现，每行格式：

```
<id>  agent=<type>  status=<state>  title=<truncated>  started_at=<iso>
```

单测 `cmd_activity_cards_lists_session_cards` 验证有数据时的渲染：

```
<id>  agent=coding  status=pending  title=fixing the bug  started_at=...
```

## §8.13 协作面板 GUI 部分

PRD §8.13.3 / §8.13.6 详细描述了协作面板的视觉呈现（折叠 / 展开 / waiting_user 置顶 / 颜色编码）。这部分需要 macOS 桌面端（m1 阶段）。CLI 已有数据通道，GUI 拿到 `list_for_session` 输出渲染即可。

## §8.13.5 ActivityEvent → ActivityCard template

每个 AgentDefinition 的 `progress_templates` 字段对齐 PRD §8.12.2：

| Agent | thinking | tool_call | milestone | waiting |
|---|---|---|---|---|
| coding | "正在分析 {{target}} 的结构..." | "{{action}} {{target}}" | "{{achievement}}" | "找到 {{count}} 种方案，等你选择" |
| devops | "正在分析 {{target}}..." | "{{action}}" | "{{achievement}}" | "需要你确认：{{topic}}" |
| research | "正在梳理 {{target}} 的相关资料..." | "搜索「{{target}}」，找到 {{count}} 条结果" | "{{achievement}}" | "整理完成，等你查看" |

实现：`crates/jarvis-router/src/agent_registry.rs::AgentDefinition::progress_templates`，与 PRD §8.12.2 完全对齐。
