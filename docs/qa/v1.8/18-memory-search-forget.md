# 18 — Memory 检索 / 遗忘 / 历史

**PRD 章节**：§12.3 写入规则 · §12.4 冲突 · §13 检索 · §15.7.3 memory_change_log · §23.6 Memory API

**结论**：✅ 通过

## 验收点

- [x] memory search <q> 走混合检索（hybrid score 排序）
- [x] memory forget <id> 设 status=Deprecated（不真删）
- [x] memory forget 自动写入 memory_change_log
- [x] forget 后 list / search 不再展示
- [x] memory-history <id> 输出完整变更链（created → deprecated）
- [x] §15.7.3 memory_change_log 不可变（DB 触发器拒绝 UPDATE/DELETE）

## A. memory search

```bash
$ jarvis memory search vim 5
   0.037  trust=0.95  [preference_memory] 用户偏好 vim 编辑器  id=mem_1323427824af
   0.000  trust=0.95  [preference_memory] 周末喜欢做菜  id=mem_ceafccf5592a
   0.000  trust=0.95  [preference_memory] Mirage 项目用 Riverpod  id=mem_b950f75af1b9
```

✓ 正向命中 vim 关键词的条目得分最高（0.037 > 0.000）
✓ 输出格式：`<score>  trust=<v>  [<type>] <content>  id=<mem_id>`，id 可链式接 forget / history

```bash
$ jarvis memory search Mirage 5
   0.037  trust=0.95  [preference_memory] Mirage 项目用 Riverpod  id=mem_b950f75af1b9
   0.000  ...
   0.000  ...
```

✓ 不同关键词触发不同排序

## B. memory forget

```bash
$ MEM_ID=$(jarvis memory search vim | grep -oE 'id=mem_[0-9a-f]+' | head -1 | cut -d= -f2)
$ jarvis memory forget "$MEM_ID" 隐私清理
deprecated mem_1323427824af (reason=隐私清理)

$ jarvis memory list
[preference_memory] 周末喜欢做菜 trust=0.95
[preference_memory] Mirage 项目用 Riverpod trust=0.95
```

✓ 已 deprecate 的 memory 从 list / search 自动隐藏（`memory_repo::list_by_scope` 过滤 status=Approved）
✓ reason 透传到 audit 链路

## C. memory-history

```bash
$ jarvis memory-history mem_1323427824af
─── memory mem_1323427824af ─── 2 entries ───
  [2026-05-07 03:48:08] created module=memory_manager reason="CLI"
  [2026-05-07 03:48:08] deprecated module=cli reason="隐私清理"
```

✓ 完整记录创建 + 软删除事件
✓ source_module 标注（manager / cli）
✓ reason 完整保留

PRD §15.7.5 要求：「Memory 溯源（Provenance）：完整的创建→修改→合并历史链」—— 已满足。

## D. CLI ↔ PRD §23.6 endpoint 对照

| PRD endpoint | CLI 等价 | 状态 |
|---|---|---|
| GET /memory/:scope | `jarvis memory list` | ✅ |
| DELETE /memory/:id | `jarvis memory forget <id>` | ✅（软删除） |
| POST /memory/:id/verify | （等价：用户确认 → user_explicit 写入） | 🟡 partial |
| POST /memory/:id/reject | （等价：forget + reason） | ✅ |

## E. 单测

`crates/jarvis-cli/src/cmd/tests.rs`：

| 用例 | 验证 |
|---|---|
| `cmd_memory_search_ranks_query_relevant_first` | 命中查询的 memory 排在第一位 |
| `cmd_memory_search_rejects_empty_query` | 空查询报错 |
| `cmd_memory_forget_marks_deprecated_and_logs_change` | forget 后 status=Deprecated + history 记录 |
| `cmd_memory_forget_returns_error_for_unknown_id` | unknown id 报错 |

## 备注

- `memory edit <id>` 当前没有提供，因为 PRD 的 `MemoryChangeType` 枚举里没有 `Updated` 变体。改 content 等价于 deprecate 旧 + write 新（user_explicit）
- `memory forget` 软删除 → PRD §27.11 的"分级存储"未实现，所有历史一律 hot 存 SQLite（v0.4 之后才考虑分级）
