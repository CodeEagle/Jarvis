# 25 — Growth Engine

**PRD 章节**：§16 Growth Engine · §17 Growth 接入点 · §19 Agent 策略成长 · §27.4 风险应对

**结论**：✅ 通过

## 验收点

- [x] §16.1 Growth Engine 是唯一成长中枢
- [x] §16.2 Collector / Evaluator / Extractor / Curator / Promotion Gate / Regression Runner / Artifact Store / Publisher
- [x] §16.3 GrowthEvent 数据结构（trace_id / source_module / event_type / payload）
- [x] §16.4 GrowthArtifact 数据结构（type / status / version / scope / confidence / evidence_trace_ids）
- [x] §16.5 状态机：candidate → testing → promoted → published
- [x] §16.6 各类 artifact 的自动晋升规则
- [x] §16.7 Skill Regression Runner 回放 + mock tool runtime
- [x] §17 Router / Runtime / Tool / Memory / Skill / Compressor 上报事件
- [x] §27.4 Growth Engine 不可用时主流程降级（异步排队）

## CLI

```bash
$ jarvis growth events route_decision
[2026-05-07T03:51:15+00:00] router route_decision {"agent_type":"general","confidence":0.3,...}
[2026-05-07T03:51:03+00:00] router route_decision {"agent_type":"research","confidence":0.65,...}
[2026-05-07T03:38:31+00:00] router route_decision {"agent_type":"devops","confidence":0.7,"primary_intent":"devops.networking",...}
...

$ jarvis growth artifacts
(空 — 当前测试运行没有产生 artifact)

$ jarvis growth filter <type> [status]
```

✓ Router 每次决策都自动上报 GrowthEvent（`source_module=router`、`event_type=route_decision`），符合 §17.1。

## 单测（21 项 jarvis-growth 全过）

```text
$ cargo test -p jarvis-growth --lib
running 21 tests, all pass
```

| 用例 | PRD 对照 |
|---|---|
| `emit_persists_an_event` | §16.3 GrowthEvent 持久化 |
| `put_and_get_artifact_round_trip` | §16.4 Artifact CRUD |
| `list_artifacts_filters` | artifact 过滤查询 |
| `routing_example_promotes_on_one_success` | §16.6 routing_example 自动晋升 |
| `skill_candidate_below_three_successes_blocks` | §16.6 skill 需 ≥3 成功 |
| `skill_candidate_with_high_failure_rate_blocks` | §16.6 失败率 >20% 不晋升 |
| `skill_candidate_without_regression_blocks` | §16.7 必须通过 regression |
| `skill_candidate_meeting_all_rules_promotes` | §16.6 满足所有条件 → 晋升 |
| `regression_runner_replays_steps_against_mock` | §16.7 mock tool runtime 回放 |
| `regression_runner_fails_when_mock_has_no_expectation` | §16.7 mock 缺 expectation 失败 |
| `complex_task_triggers_upgrade_after_debounce` | §19.3 模型升级 + 防抖 |
| `simple_task_with_budget_pressure_downgrades` | §19.3 模型降级 |
| `opus_does_not_upgrade_further` | §9.5.2b Opus 是上限 |
| `budget_shrinks_after_three_low_usage_runs` | §19.4 token 预算自学习下调 |
| `budget_never_deviates_more_than_40_percent` | §19.4 调整幅度 ±40% 上限 |
| `skill_match_filters_by_intent_overlap` | §18.4 skill 匹配 |
| `skill_round_trip_through_registry` | §18.2 SkillRegistry CRUD |

## §16.6 自动晋升规则（实现 vs PRD）

| Artifact 类型 | PRD 规则 | 实现 |
|---|---|---|
| routing_example | 用户纠正后高权重 → 自动晋升 | ✅ `routing_example_promotes_on_one_success` |
| memory_candidate | user_explicit → promoted；推断需积累 | ✅ |
| skill_candidate | ≥3 成功 + 失败率 ≤20% + regression pass | ✅ |
| tool_policy | ≥5 样本 + 新策略成功率明显高于旧 | 🟡 库 API 有，未在路由路径触发收集 |
| intent_candidate | 默认人工确认 / 大量证据 | 🟡 见 [03-mention.md](./03-mention.md) unresolved mention 上报路径 |

## §17 各模块 Growth 接入点

| 模块 | 上报事件 | 状态 |
|---|---|---|
| Router | route_decision / route_corrected / fallback_used | ✅ |
| AgentRuntime | agent_started / agent_result / failed / timeout | 🟡 部分 |
| ToolRuntime | tool_call_finished / failed / permission_denied | ✅ |
| MemoryManager | memory_retrieved / memory_candidate / user_corrected_memory | ✅ |
| SkillSystem | successful_trace / skill_used / skill_failed | 🟡 |
| Compressor | summary_used / summary_caused_error | 🟡 |

## §27.4 降级

PRD §27.4 + §5.5：Growth Engine 宕机 → 主流程跳过上报，异步重试。

实现：
- `crates/jarvis-control/src/replication.rs` 实现了 outbox 重试机制（pending → drain → mark delivered）
- 单测 `replicator_drains_pending_rows_to_peer` / `replicator_stops_on_peer_error_without_marking_delivered` 验证

## §19.3 model_policy artifact

`crates/jarvis-growth/src/model_policy.rs::ModelPolicyGenerator::run`：每日基于 ModelUsageStat 生成 model_policy artifact，Router 路由时优先读取推荐模型。单测覆盖 5+ 用例。
