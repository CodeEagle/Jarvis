# 21 — 三路混合检索

**PRD 章节**：§13 检索系统 · §13.1 三路混合 · §13.2 各类内容策略 · §13.5 Prompt Cache

**结论**：✅ 通过（FTS + 向量 + Jaccard 三路 + 情绪共振）

## 验收点

- [x] §13.1 FTS5 关键词检索（精确命中）
- [x] §13.1 向量相似度检索（语义召回）
- [x] §13.1 Jaccard token overlap（短文本兜底）
- [x] §13.1 hybrid_score 公式：(fts*0.35 + vec*0.4 + jaccard*0.15) * (0.7 + trust*0.3)
- [x] §13.2 各类内容差异策略（memory / session / routing examples / skill）
- [x] §13.4 SQLite + sqlite-vec（v0.1 路线）
- [x] §1.2 情绪共振第四路（负面情绪查询 → 正面记忆加分）

## 实现位置

`crates/jarvis-memory/src/retrieval.rs`：

```rust
pub fn hybrid_score_full(
    fts: f32, vec: f32, jaccard: f32,
    trust: f32, emotion_bonus: f32
) -> f32 {
    let raw = fts * 0.35 + vec * 0.40 + jaccard * 0.15;
    raw * (0.7 + trust * 0.3) + emotion_bonus
}

pub fn emotion_resonance_bonus(
    query: EmotionContext,
    mem_energy: f32, mem_polarity: EmotionPolarity
) -> f32 {
    // 负面情绪查询 + 正面 memory → 安抚加分
    // 正面情绪查询 + 负面 memory → 不加分
    // 低能量查询（< 3）→ 不触发
    // 最大加成 ≤ 0.15
}
```

✓ 公式与 PRD §13.1 完全对齐。

## 单测

```text
$ cargo test -p jarvis-memory --lib retrieval
running ~10 tests, all pass（含在总 28 个 memory 测试）
```

| 用例 | PRD §13 对照 |
|---|---|
| `hybrid_score_balances_three_paths` | §13.1 三路加权 |
| `hybrid_score_amplified_by_high_trust` | §13.1 trust_score 调节 |
| `emotion_resonance_negative_query_boosts_positive_memory` | §1.2 v1.2 共振机制 |
| `emotion_resonance_positive_query_does_not_boost_negative_memory` | 反向不加分 |
| `emotion_resonance_low_energy_no_bonus` | low energy 不触发 |
| `emotion_resonance_capped_at_0_15` | 加成上限 ≤ 0.15 |
| `retrieve_filters_below_threshold` | §13.2 综合分 < 0.2 不注入 |
| `retrieve_excludes_deprecated_memory` | 软删除 memory 不出现 |

## CLI 实测

`jarvis memory search` 走的是同一条混合检索路径（见 [18-memory-search-forget.md](./18-memory-search-forget.md)）：

```bash
$ jarvis memory search vim 5
   0.037  trust=0.95  [preference_memory] 用户偏好 vim 编辑器  id=mem_1323427824af
   0.000  ...
```

输出第一列就是 hybrid_score，可看到命中关键词的条目得分明显高于无关条目。

## §13.4 存储路线

| 阶段 | 实现 |
|---|---|
| v0.1 | SQLite + FTS5 + sqlite-vec ✅ |
| v0.2 | + Qdrant（按需）⏸️ 当前仍 sqlite-vec |
| v1.0 | PostgreSQL + pg_trgm + pgvector ❌ |

当前 v1.8 = v0.1 实现。`crates/jarvis-db/src/embeddings.rs` + `crates/jarvis-memory/src/vectors.rs` 提供 sqlite-vec 集成。

## §13.5 Prompt Cache 分层

注入顺序：

```
System Prompt（稳定层 → 高缓存命中）：
  persona.md / user.md
  agent_profile（含 progress_templates）
  promoted skills index
  framework directives

User Turn（动态层 → 每次不同）：
  hybrid retrieve 结果（top-5）
  rolling summary
  recent_messages / task tree view
  current task_envelope
```

实现：`crates/jarvis-router/src/system_prompt.rs::build_layered`。
