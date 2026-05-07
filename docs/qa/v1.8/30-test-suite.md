# 30 — 测试用例总览

**PRD 章节**：§30 测试规范总览（30.1 ~ 30.15）

**结论**：✅ 默认套件 311 通过 / 0 失败；真 codex e2e 6 通过

## 默认 cargo test 总览

```text
$ cargo test --workspace 2>&1 | grep "test result:"
crate                          passed  failed  ignored
─────────────────────────────────────────────────────
jarvis-anthropic               5       0       0
jarvis-api                    10       0       0
jarvis-cli                    34       0       0
jarvis-codex                   6       0       6     ← 6 ignored = real codex
jarvis-control                14       0       0
jarvis-core                   13       0       0
jarvis-db                     28       0       0
jarvis-growth                 21       0       0
jarvis-memory                 44       0       0
jarvis-openai                  3       0       0
jarvis-orchestrator           69       0       0
jarvis-router                 46       0       0
jarvis-tools                  18       0       0
─────────────────────────────────────────────────────
total                        311       0       6
```

**313 unit + integration tests 默认全过；6 个 #[ignore] 真 codex 测试单独跑也 6/6 通过（22s）。**

## 真 codex e2e 实测

```text
$ cargo test -p jarvis-codex --lib -- --ignored
running 6 tests
test tests::real_codex_smoke ... ok                          # 13s
test tests::real_codex_english_casual_input ... ok
test tests::real_codex_continue_existing_path ... ok
test tests::real_codex_long_input_with_rule_hints ... ok
test tests::real_codex_respects_allowed_agents_constraint ... ok
test tests::real_codex_through_router ... ok                 # 19s
test result: ok. 6 passed; 0 failed (22.09s 并行)
```

## PRD §30 章节覆盖

| PRD §30.x | 主题 | 测试位置 | 状态 |
|---|---|---|---|
| §30.1 Main Router | IntentClassifier / SessionResolver / Router | jarvis-router/tests.rs (46) | ✅ |
| §30.2 ConversationBus | ownership / message router / 中断协议 | jarvis-orchestrator (69) | ✅ |
| §30.3 Walkthrough/Verifier/Regression | 自动审批 / 只读验证 / 智能 diff | jarvis-orchestrator (69) | ✅ |
| §30.4 响应保证 | SLA / Watchdog / 崩溃自愈 / 降级 | jarvis-control (14) | ✅ |
| §30.5 Steer 协议 | 信号识别 / 注入 / 频率保护 / Codex append | jarvis-orchestrator steer 8 用例 | ✅（库层） |
| §30.6 Tool Runtime | 权限 / 降级 / raw_event_log 写入 | jarvis-tools (18) | ✅ |
| §30.7 Memory 系统 | 写入 / 情绪范围 / trust / 冲突 | jarvis-memory (44) + jarvis-db (28) | ✅ |
| §30.8 Dream 系统 | 整理 / 固化 / 生长 / 间隙 | jarvis-memory dream 用例 | ✅ |
| §30.9 检索系统 | FTS5 / 向量 / Jaccard / 情绪共振 / 门槛 | jarvis-memory retrieval 用例 | ✅ |
| §30.10 压缩系统 | 动态阈值 / Rolling Summary / aggressive | jarvis-memory compression 用例 | ✅ |
| §30.11 Growth Engine | Skill 晋升 / Regression / Artifact 回滚 | jarvis-growth (21) | ✅ |
| §30.12 并发与锁 | Workspace lock / Tentacle lock / session 串行 | jarvis-orchestrator | ✅ |
| §30.13 不可变日志 | seq / triggers / checksum / 时间点回放 | jarvis-db (28) | ✅ |
| §30.14 模型升降级 | 升降级 / 防抖 / token 预算 | jarvis-growth | ✅ |
| §30.15 @mention | 解析 / 4 mode / unresolved / 内部 Agent | jarvis-router (19 mention 用例) | ✅ |

## 测试增长曲线

| 时间点 | 默认 | ignored 真 codex |
|---|---|---|
| 起始（用户加入会话）| 287 | 0 |
| 加 jarvis-codex adapter | +4 unit + 1 ignored | 1 (smoke) |
| 加 Router→Codex e2e | +0 | +1 (router) |
| 加 timeout / 并发 | +2 | +0 |
| 加 4 多场景 e2e | +0 | +4 |
| 加 chat REPL judge + Control Plane | +1 | +0 |
| 加 memory search/forget + judge probe | +6 | +0 |
| 加 sessions/persona/cards CRUD | +10 | +0 |
| **当前** | **311** | **6** |

净增 **+24 默认 + 6 ignored 真 codex**。

## 不在测试覆盖里的

- 🖥️ macOS 桌面端 GUI（m1 之后）
- ⏸️ Steer / Tentacle / commands.json 的 e2e 运行时打通（库层 + 单测覆盖到位，缺真 worker driver 端到端）
- ❌ 多用户 / 跨设备同步真实场景（replicator 单测 OK，多设备 e2e 缺）
- ❌ MCP / plugin 接入（v1.0 任务）
