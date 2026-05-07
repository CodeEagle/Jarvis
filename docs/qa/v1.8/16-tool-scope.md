# 16 — Tool Runtime / ToolScope

**PRD 章节**：§11 Tool Runtime · §11.1 工具分级 · §11.2 ToolScope · §11.4 降级

**结论**：✅ 通过

## 验收点

- [x] §11.1 四级分级：read-only / safe-write / dangerous-write / external-action
- [x] §11.2 ToolScope 完整字段（allowed / blocked / requires_confirmation / max_tool_calls / max_parallel_tools）
- [x] §11.3 Agent 只能调用 ToolScope 内的工具
- [x] §11.3 危险操作必须确认
- [x] §11.4 工具超时返回结构化错误
- [x] §11.4 外部不可用 → unavailable

## CLI 验证（route 输出 ToolScope）

```bash
$ jarvis route "openwrt 编译报错"
"tool_scope": {
  "allowed_tools": ["read_file","read_config","list_dir","inspect_system","logread","web.search","shell.exec"],
  "blocked_tools": [],
  "requires_confirmation": ["shell.exec","modify_config","restart_service","modify_firewall","router_config"],
  "max_tool_calls": 40,
  "max_parallel_tools": 2
}
```

✓ §11.2 全字段呈现
✓ §11.3 危险操作（shell.exec / modify_config / router_config 等）放在 requires_confirmation
✓ §11.3 max_tool_calls / max_parallel_tools 设了上限

## §8.12 各 Agent 默认 ToolScope（与 PRD 对照）

| Agent | allowed_tools | requires_confirmation | max_tool_calls |
|---|---|---|---|
| coding | read_file / list_dir / write_file / create_file / web.search | delete_file / shell.exec | 30 |
| devops | read_file / read_config / list_dir / inspect_system / logread / web.search / shell.exec | shell.exec / modify_config / restart_service / modify_firewall / router_config | 40 |
| research | web.search / web.fetch / read_file （blocked: all_write） | （空） | 20 |
| creative | web.search / image_gen.prompt / read_file / create_note | （空） | 15 |

实现：`crates/jarvis-router/src/agent_registry.rs::AgentDefinition::default_tool_scope` 全部对齐 PRD §8.12.2。

## 单测

`crates/jarvis-tools/src/tests.rs`：18 个测试覆盖：
- allowed_tools 内允许调用
- 不在 allowed 内拒绝
- blocked_tools 即使在 allowed 也封禁
- requires_confirmation 状态机
- 超时 / unavailable 降级路径
- 所有调用写 raw_event_log

```text
$ cargo test -p jarvis-tools
running 18 tests
test result: ok. 18 passed; 0 failed
```

## §11.4 降级策略

| 故障 | 行为 |
|---|---|
| 超时 | 返回 ToolResult { status: "timeout", error: ... } |
| 权限拒绝 | 返回 ToolResult { status: "permission_denied" }，不抛 panic |
| 外部不可用 | status="unavailable"，Agent 可降级为 answer_only |
| 工具异常 | 结构化错误 + raw_event_log 记录 |

## 备注

`creative` agent 的 blocked_tools 在 PRD 写的是 `[all_write]`（一个语义占位符），实现里展开为具体 write 工具列表（safe-write + dangerous-write）。语义一致，命名细节略有出入。
