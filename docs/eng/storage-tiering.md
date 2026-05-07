# Storage Tiering — Hot / Warm / Cold

**PRD 章节**：§15.7.6 分级存储 · §27.11 原始日志存储增长风险

**状态**：📐 design only — 实现是 v1.0 产品化阶段任务

---

## 0. 为什么要分级

不可变日志三表（raw_event_log / memory_change_log / session_snapshots）只追加不删，
v0.1-v0.4 单机 sqlite 单库挂着够用，但长跑场景体量爆增：

```
保守估算：每天 100 turns × 平均 20 raw_events/turn × 平均 1 KB/event
       = 2 MB/day
       = 700 MB/year
单机够 ≈ 5 年，多用户 / 多设备共享后撑不到 1 年。
```

PRD §15.7.6 给的方案：

| 阶段 | 时间窗 | 存储 | 查询语义 |
|---|---|---|---|
| **Hot** | 0-30 天 | SQLite 主库 | 全量在线查询，毫秒延迟 |
| **Warm** | 30-180 天 | zstd 压缩归档（同库 / 旁库） | 按需解压，秒级延迟 |
| **Cold** | >180 天 | 对象存储（S3 / NAS） | 按需取回，分钟级 |

`raw_content` 在 Warm/Cold 只保留 `safe_content`（已脱敏）。
`checksum` 永久保留（完整性验证）。
`memory_change_log` 和 `session_snapshots` 体量小，永久 Hot。

---

## 1. Schema 增量

```sql
-- Hot 行不变。Warm 行额外字段：
ALTER TABLE raw_event_log
  ADD COLUMN tier TEXT NOT NULL DEFAULT 'hot';   -- hot / warm / cold
ALTER TABLE raw_event_log
  ADD COLUMN warm_compressed_blob BLOB;          -- zstd(raw_content) when tier='warm'
ALTER TABLE raw_event_log
  ADD COLUMN cold_object_key TEXT;               -- s3://... when tier='cold'

CREATE INDEX idx_raw_event_tier_ts ON raw_event_log (tier, ts);
```

读取兼容层：

```rust
pub enum LoadedRawEvent {
    Hot(RawEvent),
    Warm(WarmEvent),    // safe_content + 压缩 blob 句柄
    Cold(ColdEvent),    // safe_content + object key
}

impl Db {
    pub fn load_event_full(&self, seq: i64) -> DbResult<RawEvent> {
        match self.load_event(seq)? {
            LoadedRawEvent::Hot(e) => Ok(e),
            LoadedRawEvent::Warm(w) => self.decompress_warm(w),
            LoadedRawEvent::Cold(c) => self.fetch_cold_blocking(c),  // long
        }
    }
}
```

---

## 2. 迁移 Job

每天定时（与 Dream Lint 共生），由 `crates/jarvis-control/src/scheduler.rs` 新增 job：

```rust
async fn run_tier_migration(db: &Db, cfg: &TierConfig) {
    // Step 1: hot → warm
    let cutoff_warm = Utc::now() - Duration::days(cfg.hot_retain_days);
    let candidates = raw_event_log::list_by_tier_older_than(db, "hot", cutoff_warm)?;
    for ev in candidates {
        let blob = zstd::encode(&ev.raw_content, zstd::DEFAULT_LEVEL)?;
        raw_event_log::transition_to_warm(db, ev.seq, blob)?;
        // raw_content 字段置 NULL，safe_content 保留
    }

    // Step 2: warm → cold
    let cutoff_cold = Utc::now() - Duration::days(cfg.warm_retain_days);
    let candidates = raw_event_log::list_by_tier_older_than(db, "warm", cutoff_cold)?;
    for ev in candidates {
        let key = format!("raw_event_log/{}.zst", ev.seq);
        cfg.object_store.put(&key, ev.warm_compressed_blob)?;
        raw_event_log::transition_to_cold(db, ev.seq, &key)?;
        // warm_compressed_blob 字段置 NULL
    }
}
```

```rust
pub struct TierConfig {
    pub hot_retain_days: i64,        // 默认 30
    pub warm_retain_days: i64,       // 默认 180
    pub object_store: Box<dyn ObjectStore>,
}

pub trait ObjectStore: Send + Sync {
    fn put(&self, key: &str, blob: Vec<u8>) -> Result<()>;
    fn get(&self, key: &str) -> Result<Vec<u8>>;
}
```

实现至少有两个：
- `LocalDirObjectStore`（NAS / 本地路径）
- `S3ObjectStore`（aws-sdk-s3）

---

## 3. checksum 永久保留

不可变日志的 PRD §15.7.2 trigger 必须保留：

```sql
-- 即使 raw_content 被替换为 NULL，checksum 字段仍然不可改。
-- 已有 trigger prevent_raw_event_update：tier 字段是允许的，
-- 但 raw_content / checksum 只在 tier 转换时由 transition_to_warm 写。
-- 替代方案：把 trigger 改为只阻止 checksum / event_type / seq 的修改，
-- raw_content / safe_content / tier / cold_object_key 允许在 hot→warm→cold
-- 单调流水线下更新。
```

trigger 重写：

```sql
DROP TRIGGER IF EXISTS prevent_raw_event_update;

CREATE TRIGGER prevent_raw_event_critical_update
  BEFORE UPDATE ON raw_event_log
  WHEN OLD.checksum != NEW.checksum
    OR OLD.event_type != NEW.event_type
    OR OLD.seq != NEW.seq
    OR OLD.ts != NEW.ts
  BEGIN SELECT RAISE(ABORT, 'core fields immutable'); END;

CREATE TRIGGER prevent_raw_event_tier_regression
  BEFORE UPDATE ON raw_event_log
  WHEN (OLD.tier='cold')
    OR (OLD.tier='warm' AND NEW.tier='hot')
  BEGIN SELECT RAISE(ABORT, 'tier transitions are forward-only'); END;
```

---

## 4. 查询路径影响

`provenance::trace_events` / `provenance::replay_session_at` 调用方需要决定是否解压 warm / 取回 cold：

```rust
pub struct ReplayWindow {
    pub baseline: Option<SessionSnapshot>,
    pub events: Vec<RawEvent>,
}

pub fn replay_session_at(...) -> DbResult<ReplayWindow> {
    // Warm 自动解压（< 100 ms）；Cold 必须用 replay_session_at_async + opt-in
}
```

CLI 接口加 `--include-cold` flag：

```bash
$ jarvis replay sess_x 2025-01-01            # 只 hot + warm
$ jarvis replay sess_x 2025-01-01 --include-cold  # 触发 cold 取回
```

---

## 5. memory_change_log / session_snapshots 不分级

PRD §15.7.6：

> memory_change_log 和 session_snapshots 体量小，永久保留（不分级，回溯价值高）

实测体量：单 Memory 平均 2 KB；1000 个 Memory 一年 30 万次 change_log
→ 600 MB（~2x 原始 raw_event_log），仍然全量 hot 可接受。
若将来超限再单独治理。

---

## 6. 实现优先级

| 子任务 | v0.x | 工作量 |
|---|---|---|
| Schema 增量 + tier trigger 重写 | v1.0-α | 0.5 天 |
| Hot→Warm zstd 压缩 + LocalDirObjectStore | v1.0-α | 1 天 |
| Warm→Cold + S3ObjectStore | v1.0-β | 1.5 天 |
| Scheduler 集成 + TierConfig | v1.0-α | 0.5 天 |
| `--include-cold` 加入 CLI replay/trace-view | v1.0-β | 0.5 天 |
| 测试（迁移 + 解压 + 取回 + checksum 持续性） | v1.0-β | 1 天 |

总：~5 工作日，分两个发版增量。

---

## 7. 标记钩子

代码层加 TODO 锚点便于后续 PR 找到：

```rust
// crates/jarvis-db/src/raw_event_log.rs
// TODO §15.7.6 storage-tiering: enrich AppendEvent / RawEvent with
// tier=hot default; add list_by_tier_older_than / transition_to_warm
// / transition_to_cold helpers when tiering ships.

// crates/jarvis-control/src/scheduler.rs
// TODO §15.7.6 storage-tiering: hook a daily run_tier_migration job
// when TierConfig is wired. SchedulerConfig should grow a
// tier_migration_period + Option<TierConfig> field.

// crates/jarvis-db/src/migrations.rs
// TODO §15.7.6 storage-tiering: ALTER TABLE raw_event_log to add
// tier / warm_compressed_blob / cold_object_key columns. Replace
// prevent_raw_event_update with the two finer-grained triggers
// listed in docs/eng/storage-tiering.md §3.
```

---

## 8. 验证清单

```
✅ 现有 raw_event_log 测试不受 tier 字段加入影响（默认 'hot'）
✅ trigger 重写后 forward-only 转换允许，反向 / 篡改 checksum 仍然 ABORT
✅ Hot→Warm zstd 压缩比 ≥ 5×（中文 / 英文 raw_content 实测）
✅ Warm 解压在 100 ms 内（per-event）
✅ Cold 取回在 5 s 内（同区域 S3）
✅ Scheduler 默认 tier 迁移 off；显式开启时按 30/180 天配置生效
✅ replay / trace-view 默认不取 cold；--include-cold 显式取
✅ 单 cold 取回失败时返回结构化错误，不影响 hot/warm 部分
```
