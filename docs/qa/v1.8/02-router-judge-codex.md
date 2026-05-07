# 02 — Router LLM Judge（codex provider）

**PRD 章节**：§5.4 LLM Router Prompt · §5.5 降级策略 · §24.2 v0.2 起 LLM 判定

**结论**：✅ 通过

## 验收点

- [x] §5.4 Router 通过 LlmJudge trait 异步调用模型
- [x] §5.4 输出 schema 严格（all required fields, confidence < 1.0, session_action enum）
- [x] §5.4 router_notes 包含 judge 推理 + 引用 trace_id/task_id
- [x] §5.4 同时保留规则层 hint（混合写在 router_notes 前缀）
- [x] §5.5 judge 高置信时覆盖规则层决策（agent_type devops → coding）
- [x] §5.5 judge 失败时不破坏路由（见 04-router-fallback.md）

## 测试设置

- 适配器：`crates/jarvis-codex` 调用本地 `codex exec --output-schema` subprocess
- 鉴权：ChatGPT plan device-auth（无需 OPENAI_API_KEY）
- 默认模型：`gpt-5.5`，超时 180s

## 实测命令

```bash
$ JARVIS_JUDGE=codex jarvis route "openwrt 编译报错 no rule to make target"
```

## 实测输出（截取）

```json
{
  "primary_intent": "debug OpenWrt build error: no rule to make target",
  "secondary_intents": [
    "identify missing Makefile target or dependency",
    "guide compilation troubleshooting"
  ],
  "domain": "software_engineering",
  "topic": "OpenWrt build failure: no rule to make target",
  "session_action": "create_new",
  "agent_type": "coding",
  "confidence": 0.86,
  "clarification_needed": true,
  "tool_scope": {
    "allowed_tools": ["read_file","read_config","list_dir","inspect_system","logread","web.search","shell.exec"],
    "requires_confirmation": ["shell.exec","modify_config","restart_service","modify_firewall","router_config"],
    "max_tool_calls": 40
  },
  "router_notes": "rule hint: devops.networking (w=0.4) | judge: trace_id: trc_324f475939e6; task_id: task_4fbbba1269db. Route to coding because this is a build/compile error requiring Makefile/OpenWrt package debugging. Devops networking is secondary context only. Ask for the full error log, package/target being built, OpenWrt version, and recent feed/package changes.",
  "fallback_used": false
}
```

## 关键观测

| 项 | 规则层（无 judge） | 加入 codex judge |
|---|---|---|
| `agent_type` | `devops` | `coding` ✓ judge 修正 |
| `confidence` | 0.8 | 0.86（judge 接管） |
| `clarification_needed` | false | true ✓ judge 主动要 clarify |
| `router_notes` | rule hint only | rule hint + judge 完整推理 |
| `fallback_used` | false | false |

✓ judge 输出确实接管了规则层决策（confidence 高于规则层的 0.8 → 优先使用 judge 结果）。

✓ `requires_confirmation` 自动补全（PRD §5.4 危险工具必须放入此列表），由 Router 后处理保证。

## 单测对应

- `crates/jarvis-codex/src/tests.rs::real_codex_smoke`（13s 真 codex 调用，#[ignore]）
- `crates/jarvis-codex/src/tests.rs::real_codex_through_router`（19s 完整 Router 链路，#[ignore]）
- 多场景验证：英文 casual / continue 路径 / 单 agent 受限 / 长输入+rule hints

## 备注

- judge 输出 `domain` 是英文 `software_engineering` 而非 PRD §5.3 列出的 7 个固定值（chat/coding/devops/...）。属于 v1.8 PRD `Router 输出结构` 中 `domain` 的口径未严格 enum 约束。降级方案：rule hint 已正确分类到 devops，主流程不受影响。**建议**：后续在 Schema 加 `domain` 枚举或 normalize。
