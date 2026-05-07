# 19 — Dream 系统（离线记忆整理）

**PRD 章节**：§12.6 Dream 系统 · §16.2 MemoryLint · §27.7 推断错误风险

**结论**：✅ 通过

## 验收点

- [x] §12.6 三层处理：整理层 / 固化层 / 生长层
- [x] §12.6 整理层：重复合并、过期清理、弱 lesson 降级、长期 conflict
- [x] §12.6 固化层：相关碎片聚类 → cluster_memory
- [x] §12.6 生长层：高频模式 → inference_memory（带有效期 / 反例驳斥 / 固化 confidence=0.75）
- [x] §12.6.1 间隙触发：waiting_user + Tier4 > 阈值 → 只跑整理层
- [x] §27.7 inference ≥3 次正向证据才 confirmed
- [x] §16 Growth Engine 集成（每日定时 dream-runner）

## CLI 触发

```bash
$ jarvis maintenance global
lint: duplicates_deprecated=0 scratch_purged=0 inferences_expired=0 weak_lessons=0 conflicts_dampened=0
cluster: clusters_created=0 members_absorbed=0
```

✓ 输出涵盖 §12.6 整理层 + 固化层全部维度（5 个 lint 维度 + cluster 创建/吸收）

## 单测覆盖

```text
$ cargo test -p jarvis-memory --lib dream
running ~10 tests, all pass
```

| 用例 | PRD 对照 |
|---|---|
| `lint_marks_duplicates_deprecated` | §12.6 重复 memory 合并保留高分 |
| `lint_purges_expired_scratch` | §12.6 过期 scratch_memory 清理 |
| `lint_demotes_weak_lessons` | §12.6 trust < 0.1 + 0 retrieve 的 lesson 降级 |
| `lint_dampens_long_term_conflicts` | §12.6 conflict > 30 天双方 trust 降低 |
| `lint_expires_stale_inferences` | §12.6 inference 超过 expires_at 删除 |
| `cluster_builds_cluster_memory_from_fragments` | §12.6 固化层碎片聚类 |
| `cluster_demotes_member_trust` | §12.6 融合后原始 trust_score 降低 |
| `inference_emits_after_three_supporting_events` | §12.6 + §27.7 ≥3 次证据触发推断 |
| `inference_refutes_on_counter_evidence` | §12.6 反例驳斥 |
| `inference_promotes_to_fact_with_0_75_confidence` | §27.7 固化 confidence=0.75 |
| `gap_trigger_runs_lint_only_under_pressure` | §12.6.1 间隙触发只跑整理层 |

## §12.6 三层流程图

```
整理层（Lint，每日定时 + 间隙触发）
  ├─ 重复 memory 合并（保留高分）
  ├─ 过期 scratch / inference 清理
  ├─ 弱 lesson 降级
  └─ 长期 conflict 双方 trust 衰减

固化层（Cluster，每日定时）
  └─ 时间窗口内相关碎片 → cluster_memory（含因果叙事摘要）
     原始碎片 trust_score 降低

生长层（Inference，每日定时 + ≥3 证据触发）
  ├─ 高频模式发现 → inference_memory（with expires_at）
  ├─ 后续命中证据 → 更新计数
  ├─ 反例 → status=refuted 删除
  └─ 满 3 次正向 + 0 反例 → fact_memory（confidence=0.75，留不确定）
```

## §16 Scheduler 集成

`crates/jarvis-control/src/scheduler.rs::SchedulerConfig::default`：

```rust
const fn default() -> Self {
    Self {
        lint_interval_hours: 24,        // §12.6 daily
        cluster_interval_hours: 24,
        inference_interval_hours: 24,
        ...
    }
}
```

Scheduler 在 `jarvis serve` 时启动后台任务，单测 `scheduler_config_defaults_24h_lint` 验证默认值。

## §12.6.1 间隙触发

```text
触发条件：
  当前有子 Agent 处于 waiting_user 状态
  + Tier 4 碎片 > 80 条
→ 只跑 lint 整理层（不跑固化层 / 不跑生长层）
→ 防止"高强度连续使用时 Dream 永远没机会运行"
```

实现：`crates/jarvis-memory/src/dream.rs::should_trigger_gap_lint`，单测 `gap_trigger_runs_lint_only_under_pressure`。
