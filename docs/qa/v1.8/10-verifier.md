# 10 — VerifierAgent

**PRD 章节**：§8.15 VerifierAgent · §1.1 v1.1 产出自验证

**结论**：🟡 库层 + 单测，无 CLI 入口

## 验收点（库层）

- [x] §8.15.1 独立验证：不信任 WalkthroughDoc 内容，通过实际 read_file/重跑测试验证
- [x] §8.15.2 VerifierCheck 数据结构（check_type / expected / actual / match / discrepancy）
- [x] §8.15.3 文件存在性验证 / 测试重跑 / lint 重跑 / diff 比对
- [x] §8.15.3 验证后更新 WalkthroughDoc.verification_status
- [x] §21 verifier_checks 表

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib verifier
running ~10 tests (含在总 69)
test verifier_file_exists_passes_when_present ... ok
test verifier_file_exists_fails_with_discrepancy ... ok
test verifier_test_run_compares_output ... ok
... 等等
```

## 库层 API

`crates/jarvis-orchestrator/src/verifier.rs`：

```rust
pub struct VerifierStore { ... }
pub fn execute_check(check: &mut VerifierCheck, base_dir: &Path, target: &str) -> bool;
pub fn run(checks: &[VerifierCheck], ...) -> VerificationReport;
```

## CLI 缺口

- 无 `jarvis verifier run <walkthrough_id>` 命令
- 无 `/walkthrough/:doc_id/verify` 接口
- VerifierAgent 由 pipeline 在 walkthrough 生成后**自动**派发，不是用户直接触发

## §8.15.4 工具权限

PRD 要求 VerifierAgent 只读：

```yaml
default_tool_scope:
  allowed_tools: [read_file, list_dir, shell.exec]   # shell.exec 仅用于跑测试和 lint
  blocked_tools: [all_write]
```

实现层面：当前 `crates/jarvis-router/src/agent_registry.rs` 没有 verifier agent 的 ToolScope 定义；该 Agent 是 pipeline 内部派发，不走 Router 路径，scope 由 pipeline 直接构造。**对齐风险**：未来如果 verifier 走 Router 路径，需要在 registry 加入定义并标记 `mentionable=false`。

## 备注

§30.3.2 PRD 测试规范要求"只读权限保证（写文件应被拒绝）"——库层 execute_check 函数只调用 std::fs::read* 和 std::process::Command（用于 shell.exec 测试运行），不调用任何 write API。从代码层面满足，但**未通过 sandbox 强制隔离**（生产环境是个加固项）。
