# 12 — commands.json 快捷操作层

**PRD 章节**：§8.17 commands.json · §1.1 v1.1 借鉴 Rob Broomy commands

**结论**：⏸️ 库层骨架已就位，CLI 触发链未打通

## 验收点

- [x] §8.17.2 commands.json 数据结构（id / label / icon / description / steps）
- [x] §21 command_executions / parallel_explorations 表
- [x] CommandRunner 库函数（按 steps 顺序或 parallel 派发）
- [ ] §8.17.4 Parallel Explore 多 worktree 并行 + CompareAgent 打分（库 API 部分有，无端到端测试）
- [ ] §23.7 POST /commands/run / GET /commands/execution/:id（无 HTTP）
- [ ] CLI: `jarvis commands run <id>` 无（等价 verb 缺失）

## 库层

`crates/jarvis-orchestrator/src/commands.rs` 包含：

- 解析 commands.json 配置
- CommandRunner 按 steps 派发（顺序 / parallel）
- 内置示例 commands：sync_and_resolve / walkthrough_and_pr / regression_check / unblock_agent / parallel_explore / ask_bottleneck

## §8.17.4 Parallel Explore

PRD 要求：
- 同时启动 N 个 Agent，独立 worktree
- 各自生成 WalkthroughDoc
- CompareAgent 多维度打分
- 用户选最优，其余 worktree 直接丢弃

`crates/jarvis-orchestrator/src/commands.rs` 的 ParallelExplorationStore 提供了 DB 层支持（parallel_explorations 表），CompareAgent 的具体打分实现需要 v0.3+ 才会跑通。

## CLI 缺口

| PRD endpoint | 状态 |
|---|---|
| POST /commands/run | ❌ |
| GET /commands/execution/:id | ❌ |
| GET /commands/exploration/:id | ❌ |
| POST /commands/exploration/:id/select | ❌ |

CLI 同样无对应 verb。

## v0.3 路线图

PRD §24.3 明确 commands.json 是 v0.3 阶段任务（"完整 6 个指令"）。当前 v1.8 实现状态：库层骨架到位，pipeline 集成 + CLI/API 入口待补。**预计与 macOS 桌面 m1 一并补齐**（commands 按钮在 GUI 上更有意义）。
