# 09 — WalkthroughAgent · 主动汇报

**PRD 章节**：§8.14 WalkthroughAgent · §1.1 v1.1 主动汇报范式

**结论**：✅ 通过

## 验收点

- [x] §8.14.2 触发条件：medium/complex 任务 + 含文件修改的 success
- [x] §8.14.3 WalkthroughDoc 数据结构（sections / verification_status / approval_status）
- [x] §8.14.3 四类 section：summary / change / test_result / risk
- [x] §8.14.4 优先读取 HANDOFF.md 作为输入
- [x] §8.14.6 自动审批条件：verified + 低风险 + 测试通过 + 文件 ≤5
- [x] §8.14.6 自动 reject：disputed / 高风险 / 测试失败
- [x] §21 walkthrough_docs 表（含 verification_status / approval_status / sections_json）
- [x] CLI: `walkthrough list / approve / reject`

## 单测

```text
$ cargo test -p jarvis-orchestrator --lib walkthrough
running 13 tests
test result: ok. 13 passed; 0 failed
```

| 用例 | PRD 对照 |
|---|---|
| `walkthrough_auto_approves_low_risk_verified` | §8.14.6 自动 approve 全条件满足 |
| `walkthrough_high_risk_requires_human` | §8.14.6 高风险 → 人工 |
| `walkthrough_test_failure_blocks_auto_approve` | §8.14.6 测试失败 → 不自动 |
| `walkthrough_too_many_files_blocks_auto_approve` | §8.14.6 文件 >5 → 不自动 |
| `walkthrough_disputed_auto_rejects` | §8.14.6 verification disputed → 自动 reject |
| `walkthrough_from_handoff_parses_section_headings` | §8.14.4 HANDOFF.md → sections |
| `walkthrough_from_handoff_returns_none_when_missing` | §8.14.4 无 HANDOFF.md → 不强行生成 |
| `walkthrough_manual_approve_records_actor_and_timestamp` | §8.14.3 approval_status / approved_at / approved_by |
| `walkthrough_manual_reject_records_reason_in_notes` | §8.14.3 拒绝记录原因 |
| `walkthrough_store_round_trip_and_auto_review` | DB roundtrip + auto-review 自动评估 |
| `pipeline_runs_subtask_and_auto_approves_walkthrough` | §8.14.4 pipeline 派发 → walkthrough 生成 → 自动审批 |
| `pipeline_skips_walkthrough_when_subtask_fails` | §8.14.2 失败子任务不生成 walkthrough |
| `regression_skips_unapproved_walkthrough` | §8.16 视觉回归只跑 approved |

## CLI

```bash
# 列出 session 内所有 walkthrough
$ jarvis walkthrough list <session_id>

# 人工 approve
$ jarvis walkthrough approve <doc_id> [actor]

# 人工 reject + 原因
$ jarvis walkthrough reject <doc_id> [actor] [reason]
```

CLI 单测：`cmd_walkthrough_list_and_approve_round_trip`、`cmd_walkthrough_reject_records_actor`。

## 数据库

`crates/jarvis-db/src/migrations.rs::409` walkthrough_docs 表，索引：
- `idx_walkthrough_session(session_id, generated_at DESC)` 列表查询
- `idx_walkthrough_approval(approval_status, verification_status)` 待审批列表

字段对齐 §8.14.3：sections_json (JSON of WalkthroughSection[]) / verification_status / verification_notes / verified_at / approval_status / approved_by / approved_at。

## 备注

- §8.14.5 的协作面板渲染（展开 walkthrough 卡片 / Approve / Reject 按钮）需要 GUI（🖥️）
- §8.14 自动审批的实际触发链：v0.3 阶段 pipeline 已实现（`pipeline_runs_subtask_and_auto_approves_walkthrough` 单测验证）
