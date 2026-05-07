# 06 — Sessions CRUD

**PRD 章节**：§6.1-6.2 Session · §21.1 sessions 表 · §23 macOS-desktop POST/DELETE /sessions 对齐

**结论**：✅ 通过

## 验收点

- [x] sessions new <title> [domain] 创建活跃 session
- [x] sessions list 列出 active 状态 session
- [x] sessions archive <id> 改为 Archived 状态（幂等）
- [x] archive 后 list 不再显示
- [x] sessions messages <id> 返回历史消息

## CLI 命令

```bash
$ jarvis sessions new "OpenWrt DNS 排查" devops
created sess_88b878c070f1 title="OpenWrt DNS 排查" domain=devops

$ jarvis sessions new "Code Review Session" coding
created sess_8ad5b55e0789 title="Code Review Session" domain=coding

$ jarvis sessions list
sess_8ad5b55e0789  Code Review Session  domain=coding  last_active=2026-05-07T03:38:31.968593486+00:00
sess_88b878c070f1  OpenWrt DNS 排查  domain=devops  last_active=2026-05-07T03:38:31.850047911+00:00

$ jarvis sessions archive sess_88b878c070f1
archived sess_88b878c070f1

$ jarvis sessions list      # 第一条已归档，list 不再显示
sess_8ad5b55e0789  Code Review Session  domain=coding  last_active=2026-05-07T03:38:31.968593486+00:00

$ jarvis sessions archive sess_88b878c070f1   # 幂等
session sess_88b878c070f1 already archived
```

## 单测对应

`crates/jarvis-cli/src/cmd/tests.rs`：

| 用例 | 验证 |
|---|---|
| `cmd_sessions_new_creates_active_session` | new 后立即 list 可见、domain 正确 |
| `cmd_sessions_new_rejects_empty_title` | title 为空报错 |
| `cmd_sessions_archive_hides_from_list` | archive 后从 list 消失 |
| `cmd_sessions_archive_unknown_id_errors` | unknown id 报错 |

## 与 PRD §23 macOS API 的对齐

| PRD endpoint | CLI 等价 | 状态 |
|---|---|---|
| POST /sessions | `sessions new` | ✅ |
| DELETE /sessions/{id} | `sessions archive` | ✅（archive 替代 hard delete，保留审计） |
| GET /sessions | `sessions list` | ✅ |
| GET /sessions/{id}/messages | `sessions messages <id>` | ✅ |

## 与 PRD §21.1 sessions 表对齐

```sql
CREATE TABLE sessions (
  id, title, domain, topic, summary, long_summary,
  active_entities_json, resolved_json, unresolved_json,
  status, created_at, updated_at, last_active_at
);
```

`crates/jarvis-db/src/migrations.rs::13` 完全一致。`session_repo::list_recent` 自动过滤 `status = 'active'`，archive 后从 list 隐藏由此实现。

## 备注

- 当前 `sessions delete <id>` 真删除未提供（PRD §23 macOS 想要的是 DELETE，但保留 audit 链路更安全 → archive 是正确选择，与 PRD §27 风险应对一致）
- session messages 已支持 `--limit`，按 created_at 排序
