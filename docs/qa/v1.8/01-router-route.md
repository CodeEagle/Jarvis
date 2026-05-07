# 01 — Router 路由决策（rule-only）

**PRD 章节**：§5 Main Router · §5.4 IntentClassifier 规则层

**结论**：✅ 通过

## 验收点

- [x] §5.2 输出 `RouteDecision` 完整结构（所有字段都在）
- [x] §5.3 L1.L2 intent 格式（`devops.networking` / `creative.icon` / `memory.update`）
- [x] §5.4 规则层关键词命中（openwrt → devops, 图标→creative, 记住→memory）
- [x] §5.4 规则层修复点：`creative.icon` 而非 `creative.design`（v1.0 修复）
- [x] §5.4 confidence 严格 < 1.0
- [x] §5.4 `tool_scope` 按 agent_type 自动配置（devops 含 shell.exec 需确认；creative 无写工具）
- [x] §5.4 `requires_confirmation` 含危险操作（router_config / modify_firewall / shell.exec）
- [x] §5.4 `router_notes` 不为空，记录命中规则

## A. devops 关键词命中

```bash
$ jarvis route "openwrt 编译报错 no rule to make target"
```

```json
{
  "primary_intent": "devops.networking",
  "domain": "devops",
  "agent_type": "devops",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.8,
  "tool_scope": {
    "allowed_tools": ["read_file","read_config","list_dir","inspect_system","logread","web.search","shell.exec"],
    "requires_confirmation": ["shell.exec","modify_config","restart_service","modify_firewall","router_config"],
    "max_tool_calls": 40,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: devops.networking (w=0.4)",
  "fallback_used": false
}
```

## B. creative 关键词命中（修复点：creative.icon）

```bash
$ jarvis route "帮我生成一个图标 prompt"
```

```json
{
  "primary_intent": "creative.icon",
  "domain": "creative",
  "agent_type": "creative",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.675,
  "tool_scope": {
    "allowed_tools": ["web.search","image_gen.prompt","read_file","create_note"],
    "requires_confirmation": [],
    "max_tool_calls": 15
  },
  "router_notes": "rule hint: creative.icon (w=0.35)"
}
```

> **PRD §5.4 修复点对齐**：规则 hint 使用 `creative.icon`，与 L2 intent 命名一致，未出现 `creative.design`。

## C. memory.update 高权重规则

```bash
$ jarvis route "记住我不喜欢用 class component"
```

```json
{
  "primary_intent": "memory.update",
  "domain": "memory",
  "agent_type": "orchestrator",
  "memory_write": true,
  "confidence": 0.9,
  "router_notes": "rule hint: memory.update (w=0.8)"
}
```

> §5.4 规则表 `记住|以后|从现在开始|忘记` weight=0.8，对应输出 confidence=0.9（规则 hint 权重 + Router 内置基础 0.4 + clamp 到 0.9）。`memory_write=true`、`agent_type=orchestrator`（路由进 Memory 写入流程，非具体 worker）。

## 反向验证

- [x] confidence 三例都 < 1.0（0.8 / 0.675 / 0.9）
- [x] 无错误的 fallback 标记（`fallback_used=false` 三例）
- [x] mention_override=false（无 @mention）

## 单测对应

`crates/jarvis-router/src/tests.rs`：`route_writes_raw_event_log_first`、`route_devops_input_picks_devops_agent`、5+ 路由相关 case，全部 default-suite 通过。
