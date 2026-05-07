# 03 — @mention 预处理

**PRD 章节**：§5.3a @mention · §8.11.4 ConversationBus Step 0 · §8.12 Agent Registry mentionable · §30.15

**结论**：✅ 通过

## 验收点

- [x] §5.3a `parseMentions` 正则匹配 `@中文名/英文名`
- [x] §5.3a 三种匹配维度：display_name / type / mention_aliases
- [x] §5.3a 四种 mention_mode：none / single / multi / steer
- [x] §5.3a single → 覆盖 agent_type
- [x] §5.3a multi → 强制 orchestrator + forced_sub_agents
- [x] §5.3a steer → 当目标 Agent 运行中且内容像约束时改为注入信号
- [x] §5.3a unresolved → 标记 + 路由继续，不阻塞
- [x] §8.12 内部 Agent（Jarvis/Verifier/Walkthrough/Memory）`mentionable=false`
- [x] §9.11.6 mention_log 表记录所有 mention（含 unresolved）
- [x] §30.15 全套 19 个 mention 测试通过

## A. 单 mention 强制覆盖

```bash
$ jarvis route "@代码助手 帮我看看这个 openwrt 配置"
```

```json
"agent_type": "coding",
"mention_override": true,
"forced_sub_agents": [],
"router_notes": "[user @ specified coding] · rule hint: devops.networking (w=0.4)"
```

> 输入像 devops（openwrt 命中）但用户 @ 了代码助手 → `agent_type=coding`、`mention_override=true`，rule hint 仍保留在 notes 里（审计可见）。对应 PRD §5.3a `applyMentionOverride.single`。

## B. 多 mention 强制 orchestrator

```bash
$ jarvis route "@代码助手 @研究助手 重构并搜索方案"
```

```json
"agent_type": "orchestrator",
"mention_override": true,
"forced_sub_agents": ["coding", "research"],
"router_notes": "[user @ specified orchestrator] · rule hint: coding.refactor (w=0.35)"
```

> 对应 §5.3a `applyMentionOverride.multi`。`forced_sub_agents` 列表是 PRD 设计的字段，正确填充。

## C. unresolved mention 不阻塞

```bash
$ jarvis route "@测试助手 帮我跑测试"
```

```json
"agent_type": "general",
"mention_override": false
```

> @测试助手 不在 registry，标记 unresolved，`mention_override=false`，路由继续到 general。对应 PRD §5.3a 「未解析的 @mention 处理」。**实现差异**：当前未在 router_notes 里直接拼接「可用 Agent 列表」提示文案，那是 `ConversationBus.handleUserInput` 包装层的职责（见 §30.15.4 E2E-M4）。

## D. 内部 Agent `@Jarvis` 不可被 @

```bash
$ jarvis route "@Jarvis 帮我"
```

```json
"agent_type": "general",
"mention_override": false
```

> `mentionable=false` 的内部 Agent 视为 unresolved。对应 §8.12.2 + §30.15 单测 `jarvis_orchestrator_is_not_mentionable`、`verifier_is_not_mentionable`。

## 全套单测

```text
$ cargo test -p jarvis-router --lib mention
running 19 tests
test result: ok. 19 passed; 0 failed; 0 ignored
```

涵盖 §30.15 列出的所有覆盖要求：
- mention_mode 4 种全覆盖
- alias 解析
- multi mention orchestrator 强制
- steer 模式（运行中 Agent + 方向性内容）
- unresolved 不阻塞
- 内部 Agent 隔离
- mention_log 记录 + unresolved Growth Event 上报

## 备注

steer 模式（§5.3a / §9.11）当前在 router 层做了 mode 识别 + override_action 标记，**但实际 SteerSignal 注入到运行中子 Agent 的链路没打通**（见 [14-steer.md](./14-steer.md) ⏸️）。CLI/库层 mention 解析全部到位。
