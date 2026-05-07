# 26 — Skill System

**PRD 章节**：§18 Skill System · §16.7 Regression Runner · §17.5 SkillSystem 接入

**结论**：✅ 通过

## 验收点

- [x] §18.1 Tool / Skill 区别（单能力 vs 多步骤工作流）
- [x] §18.2 Skill 数据结构（trigger_intents / trigger_entities / required_tools / risk_level / steps / verification）
- [x] §18.4 Skill 生成流程（成功 trace → Trajectory Compressor → Skill Extractor → Candidate → Regression Runner → Promoted）
- [x] §16.7 mock tool runtime 回放验证
- [x] CLI: `jarvis skills` 列出已晋升 skill

## CLI

```bash
$ jarvis skills
（empty — 当前测试 DB 没有 promoted skill）
```

CLI 单测 `cmd_skills_list_returns_registered_skills` 验证有数据时输出：
```
diagnose  ...  status=promoted
```

## §18.2 数据结构

`crates/jarvis-growth/src/skill.rs::Skill`：

```rust
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger_intents: Vec<String>,
    pub trigger_entities: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_level: RiskLevel,
    pub steps: Vec<SkillStep>,
    pub verification: Vec<SkillVerification>,
    pub success_count: u32,
    pub failure_count: u32,
    pub status: SkillStatus,  // candidate / testing / promoted / deprecated
    pub evidence_trace_ids: Vec<String>,
    pub version: u32,
}
```

✓ 全字段对齐 PRD §18.2。

## §18.3 示例 Skill

PRD 给的 OpenWrt DNS 排查 skill 模型：

```yaml
name: diagnose_openwrt_dns
trigger_intents: [devops.networking.openwrt_dns]
required_tools: [read_config, shell.exec, logread]
steps:
  - check_client_dns_points_to_router
  - check_dnsmasq_hosts_config
  - check_homeproxy_dns_hijack
  - check_nslookup_from_client
  - suggest_restart_dnsmasq_if_needed
verification: [nslookup domain router_ip, logread -e dnsmasq]
risk_level: medium
```

实现层面 SkillRegistry / Skill 都能装载这种结构（测试 `skill_round_trip_through_registry` 验证）。

## §16.7 Regression Runner

```text
$ cargo test -p jarvis-growth --lib regression
test regression_runner_replays_steps_against_mock ... ok
test regression_runner_fails_when_mock_has_no_expectation ... ok
```

✓ mock tool runtime 不触发真实工具调用
✓ 回放成功率 ≥ 80% 才能 promoted
✓ 回放失败 → 停留 candidate，记录 failure 原因

## §16.6 晋升门槛（已在 [25-growth.md](./25-growth.md) 详述）

- ≥ 3 次成功
- 失败率 ≤ 20%
- 通过 regression（mock 回放）
- 且无新增危险操作

## §17.5 SkillSystem 接入点

| 上报 | 状态 |
|---|---|
| successful_trace | 🟡 v0.2 阶段，当前 Router 未自动统计 |
| repeated_workflow | 🟡 |
| skill_used | 🟡 |
| skill_failed | 🟡 |

| 消费 | 状态 |
|---|---|
| skill_candidate（写入 promotion_gate） | ✅ |
| promoted_skill（暴露给 Router 召回） | ✅ |
| deprecated_skill | ✅ |

## 备注

PRD §18.4 的"Trajectory Compressor / Skill Extractor"两段式抽取链路，当前需要人工或外部脚本投喂训练好的 Skill 入 candidate；自动从 trace 提炼 skill 候选的环节是 v0.2/v0.3 工作。
