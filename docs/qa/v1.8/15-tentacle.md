# 15 — Tentacle 文件结构

**PRD 章节**：§10.4 Worktree 隔离 / Tentacle 文件 · §20.4 Tentacle Lock · §27.8 风险应对

**结论**：⏸️ 库层 + 单测齐全；与运行时 worker driver 的端到端集成未跑通

## 验收点（库层）

- [x] §10.4.1 CONTEXT.md 由 generator 自动写入
- [x] §10.4.2 todo.md checkbox 解析 + Watcher 轮询
- [x] §10.4.3 文件 vs DB 边界（文件是 primary，DB 是 index）
- [x] §20.4 Tentacle Lock 规则：CONTEXT 写保护 / HANDOFF 一次性写入 / NOTES 追加
- [x] §27.8 子 Agent 无权覆写 CONTEXT.md

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib tentacle
running 5 tests
test tests::tentacle_generator_creates_files ... ok
test tests::tentacle_handoff_is_one_shot ... ok
test tests::tentacle_context_is_write_protected_for_subagents ... ok
test tests::tentacle_notes_appends ... ok
test tests::tentacle_tick_marks_step_done ... ok
test result: ok. 5 passed; 0 failed
```

| 用例 | PRD 对照 |
|---|---|
| `tentacle_generator_creates_files` | §10.4 自动生成 .tentacle/ 目录 |
| `tentacle_context_is_write_protected_for_subagents` | §20.4 / §27.8 CONTEXT 写保护 |
| `tentacle_handoff_is_one_shot` | §20.4 HANDOFF 不可覆盖 |
| `tentacle_notes_appends` | §20.4 NOTES.md 追加模式 |
| `tentacle_tick_marks_step_done` | §10.4.2 checkbox 打勾后 todo.md 状态变更 |

## 库层 API

`crates/jarvis-orchestrator/src/tentacle.rs`：

```rust
pub struct TentacleGenerator { ... }
pub fn create(envelope: &SubTaskEnvelope, base: &Path) -> Result<TentaclePaths>;
pub fn append_notes(path: &Path, content: &str) -> Result<()>;
pub fn write_handoff(path: &Path, content: &str) -> Result<()>;  // one-shot
pub fn tick_step(todo: &Path, step_index: usize) -> Result<()>;
```

## ⏸️ 缺口

- **真子 Agent 启动 + tentacle 加载**：`crates/jarvis-orchestrator/src/dispatch.rs` 的 SubTaskDispatcher 在 simple 任务路径不创建 worktree（PRD §8.4a），medium/complex 任务的 worktree 路径库 API 已实现但在 InProcessDriver 模式下不展开
- **API**：PRD §23.8 的 5 个 tentacle endpoint 全部未接：
  - GET /tentacle/:sub_task_id/{context,todo,notes,handoff}
  - PATCH /tentacle/:sub_task_id/todo

## v0.2 路线图

PRD §24.2 把 Tentacle 文件系统列在 v0.2 阶段（与 Orchestrator 一起）。库层已就位，待 worker driver 跑通后端到端集成。
