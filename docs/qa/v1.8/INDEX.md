# Jarvis v1.8 PRD QA — Index

> **📋 终端证据**：[**EVIDENCE.md**](./EVIDENCE.md) 包含本次 QA 真实捕获的 50+ 段命令 + 输出（CLI 跑 / `cargo test --nocapture`），1488 行原始 transcript。
> 每条 feature 文件描述实现状态 + 验收点；EVIDENCE.md 是命令实测原文。

每个 feature 一份独立 markdown，记录命令 + 真实 CLI 输出 + 验收点。所有 CLI 调用都跑在仓库分支 `claude/llm-e2e-testing-PLrap` 当前 commit 上，独立 in-memory / 临时 sqlite 数据库。

## 状态图例

| 标记 | 含义 |
|---|---|
| ✅ | 实现到位，CLI 或单测全绿，PRD 关键约束被覆盖 |
| 🟡 | 库层实现完整、单测覆盖，但目前没有暴露到 CLI / API |
| ⏸️ | 设计已落代码骨架，但功能尚未跑通（如 Steer 注入、Tentacle 文件流） |
| ❌ | 当前版本未实现，PRD 标记为后续阶段（v0.4 / v1.0） |
| 🖥️ | 需要 GUI / 桌面端才能完整验证（macOS m1 之后） |

## 总分布

- **总测项**：30 项 PRD 章节级 feature
- **✅ 已验证**：21 项
- **🟡 库层就绪未上 CLI**：4 项
- **⏸️ 骨架待打通**：3 项
- **❌ 未实现**：1 项
- **🖥️ 等 GUI**：1 项

## 测试套件总数

```
$ cargo test --workspace
test result: ok. 5 passed; 0 failed; 0 ignored;  ─ jarvis-anthropic
test result: ok. 10 passed; 0 failed; 0 ignored;  ─ jarvis-api
test result: ok. 34 passed; 0 failed; 0 ignored;  ─ jarvis-cli
test result: ok. 6 passed; 0 failed; 6 ignored;   ─ jarvis-codex（6 ignored = 真 codex e2e）
test result: ok. 14 passed; 0 failed; 0 ignored;  ─ jarvis-control
test result: ok. 13 passed; 0 failed; 0 ignored;  ─ jarvis-core
test result: ok. 28 passed; 0 failed; 0 ignored;  ─ jarvis-db
test result: ok. 21 passed; 0 failed; 0 ignored;  ─ jarvis-growth
test result: ok. 44 passed; 0 failed; 0 ignored;  ─ jarvis-memory
test result: ok. 3 passed; 0 failed; 0 ignored;   ─ jarvis-openai
test result: ok. 69 passed; 0 failed; 0 ignored;  ─ jarvis-orchestrator
test result: ok. 46 passed; 0 failed; 0 ignored;  ─ jarvis-router
test result: ok. 18 passed; 0 failed; 0 ignored;  ─ jarvis-tools
                                              ─────────────────────────
                            311 通过 / 0 失败 / 6 ignored
```

```
$ cargo test -p jarvis-codex --lib -- --ignored
running 6 tests
test tests::real_codex_smoke ... ok                         # 13s
test tests::real_codex_english_casual_input ... ok
test tests::real_codex_continue_existing_path ... ok
test tests::real_codex_long_input_with_rule_hints ... ok
test tests::real_codex_respects_allowed_agents_constraint ... ok
test tests::real_codex_through_router ... ok                # 19s
test result: ok. 6 passed; 0 failed (22.09s 并行)
```

## 详细清单

| PRD 章节 | Feature | 状态 | 证据 |
|---|---|---|---|
| §5 Router 职责 / RouteDecision | 路由决策（rule-only） | ✅ | [01-router-route.md](./01-router-route.md) |
| §5.4 Router 规则层 + LLM 判定 | LLM judge（codex） | ✅ | [02-router-judge-codex.md](./02-router-judge-codex.md) |
| §5.3a @mention 预处理 | @mention 解析 + override | ✅ | [03-mention.md](./03-mention.md) |
| §5.5 Router 降级策略 | judge 不可用 → fallback 标记 | ✅ | [04-router-fallback.md](./04-router-fallback.md) |
| §6 Session Resolver | explicit_reference / 评分 | ✅ | [05-session-resolver.md](./05-session-resolver.md) |
| §6.2 Session CRUD | new / archive / list | ✅ | [06-sessions-crud.md](./06-sessions-crud.md) |
| §8.11 ConversationBus | ownership / 消息路由 | 🟡 | [07-conversation-bus.md](./07-conversation-bus.md) |
| §8.13 ActivityCard | 协作面板存储 | ✅ | [08-activity-cards.md](./08-activity-cards.md) |
| §8.14 WalkthroughAgent | 文档生成 + 自动审批 | ✅ | [09-walkthrough.md](./09-walkthrough.md) |
| §8.15 VerifierAgent | 独立验证 | 🟡 | [10-verifier.md](./10-verifier.md) |
| §8.16 视觉回归测试 | RegressionReport | 🟡 | [11-regression.md](./11-regression.md) |
| §8.17 commands.json | 快捷操作 / 并行探索 | ⏸️ | [12-commands.md](./12-commands.md) |
| §9.10 主 Agent 永远响应 | SLA / 兜底 / Watchdog | ✅ | [13-control-plane.md](./13-control-plane.md) |
| §9.11 Steer 协议 | SteerSignal 注入 | ⏸️ | [14-steer.md](./14-steer.md) |
| §10.4 Tentacle 文件 | CONTEXT/todo/NOTES/HANDOFF | ⏸️ | [15-tentacle.md](./15-tentacle.md) |
| §11 Tool Runtime | scope / 权限分级 | ✅ | [16-tool-scope.md](./16-tool-scope.md) |
| §12 Memory 系统 | type / tier / trust / write | ✅ | [17-memory-write.md](./17-memory-write.md) |
| §12 Memory 检索 / 遗忘 | search / forget / history | ✅ | [18-memory-search-forget.md](./18-memory-search-forget.md) |
| §12.6 Dream 系统 | 整理 / 固化 / 生长 | ✅ | [19-dream.md](./19-dream.md) |
| §12.7 Persona | persona.md / user.md | ✅ | [20-persona.md](./20-persona.md) |
| §13 混合检索 | FTS5 + 向量 + Jaccard + 情绪 | ✅ | [21-hybrid-retrieval.md](./21-hybrid-retrieval.md) |
| §14 历史压缩 | Rolling Summary / 压缩策略 | 🟡 | [22-compression.md](./22-compression.md) |
| §15 日志 | trace / audit / module log | ✅ | [23-logging.md](./23-logging.md) |
| §15.7 不可变日志 | raw_event_log / memory_change_log | ✅ | [24-immutable-log.md](./24-immutable-log.md) |
| §16 Growth Engine | events / artifacts / promotion | ✅ | [25-growth.md](./25-growth.md) |
| §18 Skill System | registry / promotion gate | ✅ | [26-skills.md](./26-skills.md) |
| §20 并发与锁 | workspace lock / session 串行 | ✅ | [27-locking.md](./27-locking.md) |
| §23 API 设计 | HTTP API（serve） | ✅ | [28-api.md](./28-api.md) |
| §24 MVP 路线图 | v0.1-v1.0 阶段对齐 | ✅ | [29-roadmap-alignment.md](./29-roadmap-alignment.md) |
| 全套用例 | 默认 + ignored 真 LLM | ✅ | [30-test-suite.md](./30-test-suite.md) |

## 总结

**通过率**：21/30 = **70%** 完整闭环；加上 4 项库层就绪 = **83.3%** PRD 已落地。

**剩余 5 项（17%）**：
- ⏸️ Steer / Tentacle / commands.json 三件 v0.3 级目标，骨架代码已就位但 e2e 未打通
- ❌ 视觉回归 RegressionRunner CLI 入口缺失（库 API 已有）
- 🖥️ macOS 桌面端协作面板（GUI 才能验）

详细每项见对应 markdown。
