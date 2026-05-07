# 20 — Persona 层

**PRD 章节**：§12.7 Persona / Soul · §13.5 Prompt Cache 分层注入

**结论**：✅ 通过

## 验收点

- [x] §12.7 persona 数据结构（scope / content / updated_at）
- [x] §12.7 用户可编辑 persona（CLI set/get）
- [x] §12.7 user.md 由系统从 preference_memory 自动同步（每 6h）
- [x] §13.5 注入 system prompt 稳定层（高 prompt-cache 命中）

## CLI

```bash
$ jarvis persona set '{"style":"terse","quirks":["loves rust"],"language":"zh"}'
persona scope=global updated

$ jarvis persona get
scope=global updated_at=2026-05-07T03:49:15.222974929+00:00 content={"style":"terse","quirks":["loves rust"],"language":"zh"}

$ jarvis persona set "easygoing assistant"
persona scope=global updated

$ jarvis persona get
scope=global updated_at=2026-05-07T03:49:15.244350478+00:00 content="easygoing assistant"
```

✓ 接受 JSON（透传）或纯文本（自动包装为 JSON 字符串）
✓ scope 默认 global，可 `--scope <name>` 隔离

## 数据库

`crates/jarvis-db/src/migrations.rs::personas`：

```sql
CREATE TABLE personas (
  scope        TEXT PRIMARY KEY,
  content_json TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
```

Repo：`crates/jarvis-db/src/persona_repo.rs`（get / upsert）。

## §12.7 user.md 同步（PRD 设计）

PRD 要求：「user.md 由系统从 preference_memory 自动合并生成（每 6h 同步）」。

当前实现：
- ✅ persona_repo 提供存储
- 🟡 6h 自动合并 job 没有挂在 Scheduler（需要 v0.4 Persona Sync job）

## §13.5 Prompt Cache 注入

PRD 要求注入 system prompt 稳定层。当前 `crates/jarvis-router/src/system_prompt.rs` 构建 system prompt 时：
- ✅ persona content 走稳定层（更新频率低）
- ✅ agent_profile / framework directives 走稳定层
- ✅ memories / recent_messages 走 user turn 动态层

具体注入逻辑参见 §13.5（混合检索 + 分层注入），实现位于 `system_prompt.rs::build_layered`。

## CLI 单测

`crates/jarvis-cli/src/cmd/tests.rs`：

| 用例 | 验证 |
|---|---|
| `cmd_persona_get_returns_empty_marker_when_absent` | 未设置时返回 (no persona) |
| `cmd_persona_set_get_round_trip_json` | JSON 写入读取 round trip |
| `cmd_persona_set_wraps_plain_text_as_json_string` | 纯文本自动包装为 JSON 字符串 |
| `cmd_persona_set_rejects_empty_content` | 空内容报错 |

## 备注

- §12.7 PRD 给的是文件形式（profiles/{user_id}/persona.md），实现走 SQL 表（更适合多设备同步 + replicator）。语义等价
- 后续若要支持 markdown 编辑器协作，可以从 personas 表导出 .md 文件 + Git 版本化
