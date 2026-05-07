# 11 — 视觉回归测试 / RegressionRunner

**PRD 章节**：§8.16 视觉回归测试 · §16.7 Regression Runner 回放

**结论**：🟡 库层 + 单测全过；CLI / API 入口缺失

## 验收点（库层）

- [x] §8.16.2 加载所有 status=approved 的 WalkthroughDoc
- [x] §8.16.2 并行派发 VerifierTask，独立 worktree
- [x] §8.16.2 RegressionAnalyzer 区分 expected_change vs potential_bug
- [x] §8.16.2 跳过 unapproved walkthrough
- [x] §8.16.3 RegressionReport（total / passed / expected_changes / potential_bugs / items）
- [x] §8.16.4 智能差异分析：差异在已修改文件 → expected_change；差异在未修改文件 → potential_bug
- [x] §21 regression_reports 表

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib regression
running 4 tests
test tests::regression_pass_when_no_changes ... ok
test tests::regression_classifies_touched_failure_as_expected_change ... ok
test tests::regression_skips_unapproved_walkthrough ... ok
test tests::regression_classifies_untouched_failure_as_potential_bug ... ok
test result: ok. 4 passed; 0 failed
```

直接对应 §8.16.2/8.16.4 的所有判定路径。

## §16.7 Skill Regression Runner

PRD §16.7 列出的回放规则同样实现在 `crates/jarvis-growth/src/promotion.rs` 里：

- 至少 3 条成功 trace
- mock tool runtime 回放（不触真实工具）
- 回放成功率 ≥ 80%
- 回放失败 → 停留 candidate

测试：`crates/jarvis-growth/src/tests.rs` 多个 promotion-gate 用例覆盖。

## CLI 缺口

PRD §23.4 列了 `POST /regression/run` 和 `GET /regression/report/:id`，CLI 也没有 `jarvis regression run` / `regression report`。

`crates/jarvis-orchestrator/src/regression.rs::RegressionRunner::run` 库 API 已就绪，但没人调用；预期 v0.4 + commands.json 的 "发版前回归检查" 按钮触发（PRD §8.17.2）。

## ❌ 标注

INDEX 把这一项列为 🟡（库层就绪未上 CLI），不是 ❌（未实现）。原因：核心 RegressionAnalyzer 已写完且测试覆盖，差的是 user-facing 入口。
