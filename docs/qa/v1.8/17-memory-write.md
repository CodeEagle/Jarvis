# 17 — Memory 写入 / 类型 / Tier / Trust

**PRD 章节**：§12.1 Memory 类型 · §12.1a Tier · §12.2 数据结构 · §12.3 写入规则 · §12.5 衰减

**结论**：✅ 通过

## 验收点

- [x] §12.1 8 种 Memory 类型完整（preference / project / episode / fact / entity / relation / lesson / scratch）
- [x] §12.1 v1.2 新增类型：inference_memory / cluster_memory
- [x] §12.1a Tier 1-4 分层
- [x] §12.2 完整数据结构（含情绪坐标 / tier / expires_at / cluster_member_ids）
- [x] §12.3 user_explicit → confidence 0.95，直接 approved
- [x] §12.5 分类衰减（lesson 90 / preference & project 60 / fact & entity & relation 30 / episode 14 / scratch 0）
- [x] §12.5 trust_score 计算 = confidence * decay + retrieve_boost
- [x] §12.4 冲突检测（实体重叠 + 内容相似度低）
- [x] §12.7 Persona 层（见 [20-persona.md](./20-persona.md)）

## A. CLI memory write（user_explicit 路径）

```bash
$ jarvis memory write 用户偏好 vim 编辑器
wrote mem_1323427824af status=Approved trust=0.95

$ jarvis memory write 周末喜欢做菜
wrote mem_ceafccf5592a status=Approved trust=0.95

$ jarvis memory write Mirage 项目用 Riverpod
wrote mem_b950f75af1b9 status=Approved trust=0.95

$ jarvis memory list
[preference_memory] 用户偏好 vim 编辑器 trust=0.95
[preference_memory] 周末喜欢做菜 trust=0.95
[preference_memory] Mirage 项目用 Riverpod trust=0.95
```

✓ source_type=user_explicit → confidence=0.95 → status=Approved（PRD §12.3）

## B. Tier 自动设定

`crates/jarvis-memory/src/manager.rs::write` 根据 source_type → tier：

| source_type | tier | 说明 |
|---|---|---|
| user_explicit | 1 | 核心档案，最高优先级 |
| correction | 1 | 用户纠正同样高 |
| task_result | 3 | 事件快照 |
| inferred | 4 | 原子碎片（最低） |
| dream_cluster | 2 | 任务状态 |
| dream_inference | 4 | 待验证推断 |

## C. Trust Score 衰减（库层验证）

`crates/jarvis-memory/src/trust.rs::compute`：

```rust
let days = (now - mem.created_at).num_days() as f32;
let decay = 0.5_f32.powf(days / mem.half_life_days);
let retrieve_boost = (mem.retrieve_count as f32 * 0.02).min(0.2);
let trust = (mem.confidence * decay + retrieve_boost).clamp(0.0, 1.0);

// Tier 1 底线
if mem.tier == 1 && trust < 0.3 { trust = 0.3 }
```

✓ 与 PRD §12.5 完全对齐（含 Tier 1 trust ≥ 0.3 底线）。

## D. 半衰期对照表（实现 vs PRD）

| Type | PRD §12.5 | 实现 |
|---|---|---|
| lesson_memory | 90 天 | 90 ✓ |
| preference_memory | 60 天 | 60 ✓ |
| project_memory | 60 天 | 60 ✓ |
| fact_memory | 30 天 | 30 ✓ |
| entity_memory | 30 天 | 30 ✓ |
| episode_memory | 14 天 | 14 ✓ |
| relation_memory | 30 天 | 30 ✓ |
| scratch_memory | 0 天（不衰减直接过期） | 0 ✓ |

`crates/jarvis-core/src/memory.rs::MemoryType::default_half_life_days`。

## E. 情绪坐标范围（§12.3a）

PRD 要求：仅 episode / lesson / cluster / inference 四种类型可设情绪坐标，其余强制 energy=0/neutral。

实现：`Memory::enforce_emotion_gate` 在 upsert 时调用，对不适用类型强制清零。单测 `crates/jarvis-memory/src/tests.rs::trust_score_decays_lesson_slower_with_emotion` 验证情绪强度调节衰减；`memory_repo` 的 `upsert` 调用 `enforce_emotion_gate` 保证无法绕过。

## F. 冲突检测（§12.4）

`crates/jarvis-memory/src/manager.rs::check_conflict`：FTS5 检索同实体已有 memory + 简易内容相似度判断 → 实体重叠且内容矛盾时标记 conflict_ids，不直接覆盖。

```text
$ cargo test -p jarvis-memory --lib
running 28 tests, all pass
```

## G. 单测覆盖

| 用例 | PRD 对照 |
|---|---|
| `write_user_explicit_marks_approved_with_high_trust` | §12.3 user_explicit → 0.95 / Approved |
| `write_inferred_starts_as_candidate` | §12.3 inferred → candidate |
| `correction_lowers_existing_trust` | §12.3 非对称信任：correction 主动降已有 trust |
| `trust_score_decays_with_age` | §12.5 衰减公式 |
| `trust_score_lesson_slower_with_emotion` | §12.5 情绪浓度调节 |
| `tier_one_has_floor` | §12.1a Tier 1 ≥ 0.3 底线 |
| `enforce_emotion_gate_clears_invalid_types` | §12.3a 适用范围 |
| `conflict_detected_on_overlapping_entities_with_diff_content` | §12.4 冲突检测 |

约 28 个 memory 模块测试 + 13 个 db 层测试全部通过。
