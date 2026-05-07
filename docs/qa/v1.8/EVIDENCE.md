# v1.8 PRD QA — 终端 Transcript 证据汇总

每个 feature 一段真实捕获的命令 + 输出（CLI 直接跑或 cargo test --nocapture）。
所有命令在分支 `claude/llm-e2e-testing-PLrap` HEAD commit 下跑出，
临时数据库：`/tmp/qa-evidence/jarvis.db`。

---


## [01.A] 规则路由（devops 命中 openwrt）

**命令：**

```bash
jarvis route "openwrt 编译报错 no rule to make target"
```

**输出：**

```
{
  "trace_id": "trc_dd998d5f0933",
  "task_id": "task_17eed26c5b50",
  "primary_intent": "devops.networking",
  "secondary_intents": [],
  "domain": "devops",
  "topic": "openwrt 编译报错 no rule",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "devops",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.8,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "read_file",
      "read_config",
      "list_dir",
      "inspect_system",
      "logread",
      "web.search",
      "shell.exec"
    ],
    "blocked_tools": [],
    "requires_confirmation": [
      "shell.exec",
      "modify_config",
      "restart_service",
      "modify_firewall",
      "router_config"
    ],
    "max_tool_calls": 40,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: devops.networking (w=0.4)",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [01.B] 规则路由（creative.icon 修复点）

**命令：**

```bash
jarvis route "帮我生成一个图标 prompt"
```

**输出：**

```
{
  "trace_id": "trc_24af8783a9b5",
  "task_id": "task_3f99c6add377",
  "primary_intent": "creative.icon",
  "secondary_intents": [],
  "domain": "creative",
  "topic": "帮我生成一个图标 prompt",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "creative",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.675,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "web.search",
      "image_gen.prompt",
      "read_file",
      "create_note"
    ],
    "blocked_tools": [],
    "requires_confirmation": [],
    "max_tool_calls": 15,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: creative.icon (w=0.35)",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [01.C] 规则路由（memory.update 高权重）

**命令：**

```bash
jarvis route "记住我不喜欢用 class component"
```

**输出：**

```
{
  "trace_id": "trc_9b532e39720f",
  "task_id": "task_9285fe7acc74",
  "primary_intent": "memory.update",
  "secondary_intents": [],
  "domain": "memory",
  "topic": "记住我不喜欢用 class compon",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "orchestrator",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.9,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": true,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [],
    "blocked_tools": [],
    "requires_confirmation": [],
    "max_tool_calls": 0,
    "max_parallel_tools": 0
  },
  "router_notes": "rule hint: memory.update (w=0.8)",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [02] LLM judge（codex 真实调用）

**命令：**

```bash
JARVIS_JUDGE=codex jarvis route "openwrt 编译报错 no rule to make target"
```

**输出：**

```
{
  "trace_id": "trc_b515369698ce",
  "task_id": "task_2d9694090a9f",
  "primary_intent": "debug OpenWrt build error: no rule to make target",
  "secondary_intents": [
    "identify missing make target or dependency",
    "guide build-system troubleshooting"
  ],
  "domain": "software_engineering",
  "topic": "OpenWrt compilation / Makefile build failure",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "coding",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.86,
  "clarification_needed": true,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "read_file",
      "read_config",
      "list_dir",
      "inspect_system",
      "logread",
      "web.search",
      "shell.exec"
    ],
    "blocked_tools": [],
    "requires_confirmation": [
      "shell.exec",
      "modify_config",
      "restart_service",
      "modify_firewall",
      "router_config"
    ],
    "max_tool_calls": 40,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: devops.networking (w=0.4) | judge: trace_id: trc_b515369698ce; task_id: task_2d9694090a9f. User reports an OpenWrt compile error with 'no rule to make target'. Route to coding because this is primarily a build/debugging issue involving Makefiles and source tree state; devops/networking is secondary due to OpenWrt context. Clarification needed for the exact error line, target path, package being built, OpenWrt version/branch, and recent changes.",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [03.A] @单 mention 强制覆盖

**命令：**

```bash
jarvis route "@代码助手 帮我看看这个 openwrt 配置"
```

**输出：**

```
{
  "trace_id": "trc_27cdda5d4b8e",
  "task_id": "task_ba27725bf9b8",
  "primary_intent": "devops.networking",
  "secondary_intents": [],
  "domain": "devops",
  "topic": "帮我看看这个 openwrt 配置",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "coding",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.8,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "read_file",
      "list_dir",
      "write_file",
      "create_file",
      "web.search"
    ],
    "blocked_tools": [],
    "requires_confirmation": [
      "delete_file",
      "shell.exec"
    ],
    "max_tool_calls": 30,
    "max_parallel_tools": 3
  },
  "router_notes": "[user @ specified coding] · rule hint: devops.networking (w=0.4)",
  "mention_override": true,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [03.B] @多 mention → orchestrator + forced_sub_agents

**命令：**

```bash
jarvis route "@代码助手 @研究助手 重构并搜索方案"
```

**输出：**

```
{
  "trace_id": "trc_0b53a72bc62c",
  "task_id": "task_30dfc35f6223",
  "primary_intent": "coding.refactor",
  "secondary_intents": [],
  "domain": "coding",
  "topic": "重构并搜索方案",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "orchestrator",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.87500006,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [],
    "blocked_tools": [],
    "requires_confirmation": [],
    "max_tool_calls": 0,
    "max_parallel_tools": 0
  },
  "router_notes": "[user @ specified orchestrator] · rule hint: coding.refactor (w=0.35)",
  "mention_override": true,
  "forced_sub_agents": [
    "coding",
    "research"
  ],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [03.C] unresolved mention 路由继续

**命令：**

```bash
jarvis route "@测试助手 帮我跑测试"
```

**输出：**

```
{
  "trace_id": "trc_c6a8cfc34df2",
  "task_id": "task_65184e0a4860",
  "primary_intent": "chat",
  "secondary_intents": [],
  "domain": "chat",
  "topic": "帮我跑测试",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "general",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.3,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [],
    "blocked_tools": [],
    "requires_confirmation": [],
    "max_tool_calls": 0,
    "max_parallel_tools": 0
  },
  "router_notes": "no rule layer hits — routed via fallback",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [03.D] 内部 Agent @Jarvis 不可被 @

**命令：**

```bash
jarvis route "@Jarvis 帮我"
```

**输出：**

```
{
  "trace_id": "trc_5916d3bb035d",
  "task_id": "task_23882796dce1",
  "primary_intent": "chat",
  "secondary_intents": [],
  "domain": "chat",
  "topic": "帮我",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "general",
  "preferred_runtime_mode": "in_process",
  "confidence": 0.3,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [],
    "blocked_tools": [],
    "requires_confirmation": [],
    "max_tool_calls": 0,
    "max_parallel_tools": 0
  },
  "router_notes": "no rule layer hits — routed via fallback",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [04] judge 不可达 → fallback_used=true

**命令：**

```bash
JARVIS_JUDGE=codex CODEX_BINARY=/nonexistent/codex jarvis route "openwrt 编译报错"
```

**输出：**

```
[2m2026-05-07T04:08:25.231504Z[0m [33m WARN[0m [2mcodex[0m[2m:[0m judge failed: io: No such file or directory (os error 2)
{
  "trace_id": "trc_19d4239401eb",
  "task_id": "task_a395a9be4fb2",
  "primary_intent": "devops.networking",
  "secondary_intents": [],
  "domain": "devops",
  "topic": "openwrt 编译报错",
  "session_action": "create_new",
  "target_session_id": null,
  "agent_type": "devops",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.8,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "read_file",
      "read_config",
      "list_dir",
      "inspect_system",
      "logread",
      "web.search",
      "shell.exec"
    ],
    "blocked_tools": [],
    "requires_confirmation": [
      "shell.exec",
      "modify_config",
      "restart_service",
      "modify_firewall",
      "router_config"
    ],
    "max_tool_calls": 40,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: devops.networking (w=0.4) | judge unavailable, rule-only fallback",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": true
}
```


## [05] explicit_reference 命中已有 session

**命令：**

```bash
jarvis route "继续 OpenWrt 排查"
```

**输出：**

```
{
  "trace_id": "trc_aae47db0f72a",
  "task_id": "task_23c8f9813e85",
  "primary_intent": "devops.networking",
  "secondary_intents": [],
  "domain": "devops",
  "topic": "继续 OpenWrt 排查",
  "session_action": "continue_existing",
  "target_session_id": "sess_79f431689cb5",
  "agent_type": "devops",
  "preferred_runtime_mode": "worker_process",
  "confidence": 0.7,
  "clarification_needed": false,
  "memory_read": true,
  "memory_write": false,
  "skill_candidates": [],
  "tool_scope": {
    "allowed_tools": [
      "read_file",
      "read_config",
      "list_dir",
      "inspect_system",
      "logread",
      "web.search",
      "shell.exec"
    ],
    "blocked_tools": [],
    "requires_confirmation": [
      "shell.exec",
      "modify_config",
      "restart_service",
      "modify_firewall",
      "router_config"
    ],
    "max_tool_calls": 40,
    "max_parallel_tools": 2
  },
  "router_notes": "rule hint: devops.networking (w=0.4)",
  "mention_override": false,
  "forced_sub_agents": [],
  "override_action": null,
  "steer_target_sub_task_id": null,
  "steer_content": null,
  "fallback_used": false
}
```


## [06.A] sessions new

**命令：**

```bash
jarvis sessions new "OpenWrt DNS 排查" devops; jarvis sessions new "Code Review Session" coding
```

**输出：**

```
created sess_d242ef0166b8 title="OpenWrt DNS 排查" domain=devops
created sess_79f431689cb5 title="Code Review Session" domain=coding
```


## [06.B] sessions list

**命令：**

```bash
jarvis sessions list
```

**输出：**

```
sess_79f431689cb5  Code Review Session  domain=coding  last_active=2026-05-07T04:08:25.258793720+00:00
sess_d242ef0166b8  OpenWrt DNS 排查  domain=devops  last_active=2026-05-07T04:08:25.245225681+00:00
```


## [06.C] sessions archive 幂等

**命令：**

```bash
jarvis sessions archive <sess_id>; jarvis sessions list; jarvis sessions archive <sess_id>
```

**输出：**

```
archived sess_79f431689cb5
sess_d242ef0166b8  OpenWrt DNS 排查  domain=devops  last_active=2026-05-07T04:08:25.245225681+00:00
session sess_79f431689cb5 already archived
```


## [08] activity-cards <session>（empty 因为该 session 未走 Orchestrator）

**命令：**

```bash
jarvis activity-cards <sess_id>
```

**输出：**

```
```


## [09] walkthrough lib tests --nocapture

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib walkthrough -- --nocapture
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.33s
     Running unittests src/lib.rs (target/debug/deps/jarvis_orchestrator-a1801a000f819e6f)

running 13 tests
test tests::walkthrough_auto_approves_low_risk_verified ... ok
test tests::walkthrough_disputed_auto_rejects ... ok
test tests::walkthrough_from_handoff_parses_section_headings ... ok
test tests::walkthrough_from_handoff_returns_none_when_missing ... ok
test tests::walkthrough_high_risk_requires_human ... ok
test tests::walkthrough_manual_approve_records_actor_and_timestamp ... ok
test tests::regression_skips_unapproved_walkthrough ... ok
test tests::pipeline_skips_walkthrough_when_subtask_fails ... ok
test tests::walkthrough_test_failure_blocks_auto_approve ... ok
test tests::walkthrough_too_many_files_blocks_auto_approve ... ok
test tests::pipeline_runs_subtask_and_auto_approves_walkthrough ... ok
test tests::walkthrough_store_round_trip_and_auto_review ... ok
test tests::walkthrough_manual_reject_records_reason_in_notes ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 56 filtered out; finished in 0.08s

```


## [10] verifier lib tests

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib verifier -- --nocapture
```

**输出：**

```
test tests::verifier_file_exists_fails_with_discrepancy ... ok
test tests::verifier_file_exists_passes_when_present ... ok
test tests::verifier_run_marks_verified_when_all_match ... ok
test tests::verifier_run_marks_disputed_when_files_missing ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 65 filtered out; finished in 0.02s
```


## [11] regression lib tests

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib regression -- --nocapture
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.27s
     Running unittests src/lib.rs (target/debug/deps/jarvis_orchestrator-a1801a000f819e6f)

running 4 tests
test tests::regression_pass_when_no_changes ... ok
test tests::regression_classifies_touched_failure_as_expected_change ... ok
test tests::regression_skips_unapproved_walkthrough ... ok
test tests::regression_classifies_untouched_failure_as_potential_bug ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 65 filtered out; finished in 0.04s

```


## [13] control-plane / SLA / watchdog 全 14 个测试

**命令：**

```bash
cargo test -p jarvis-control --lib
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.15s
     Running unittests src/lib.rs (target/debug/deps/jarvis_control-6e7114a6be0e82a5)

running 14 tests
test tests::fallback_message_executing_mentions_running_agent_when_known ... ok
test tests::fallback_message_idle_short ... ok
test tests::fallback_message_unavailable_mentions_lightweight_mode ... ok
test tests::maintenance_jobs_run_lint_synchronously ... ok
test tests::replicator_drains_pending_rows_to_peer ... ok
test tests::control_plane_falls_back_when_budget_too_tight ... ok
test tests::control_plane_returns_resolved_for_simple_input ... ok
test tests::scheduler_config_defaults_24h_lint ... ok
test tests::control_plane_with_judge_takes_judge_outcome ... ok
test tests::watchdog_fresh_beat_is_healthy ... ok
test tests::watchdog_recovery_resets_stale ... ok
test tests::watchdog_marks_stale_then_dead_after_grace ... ok
test tests::watchdog_unknown_agent_is_dead ... ok
test tests::replicator_stops_on_peer_error_without_marking_delivered ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

```


## [14] steer lib tests（含 throttle / context-protect / append-mode）

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib steer
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/jarvis_orchestrator-a1801a000f819e6f)

running 8 tests
test tests::steer_admissibility_accepts_normal_constraint ... ok
test tests::steer_admissibility_rejects_context_override_attempts ... ok
test tests::steer_adapter_records_payload_in_append_mode ... ok
test tests::steer_admissibility_rejects_empty_content ... ok
test tests::classify_steer_message ... ok
test tests::steer_writes_to_raw_event_log ... ok
test tests::steer_status_transitions_pending_injected_acknowledged ... ok
test tests::steer_first_three_accepted_then_throttled ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 61 filtered out; finished in 0.09s

```


## [15] tentacle lib tests（CONTEXT 写保护 / HANDOFF 一次性）

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib tentacle
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.22s
     Running unittests src/lib.rs (target/debug/deps/jarvis_orchestrator-a1801a000f819e6f)

running 5 tests
test tests::tentacle_generator_creates_files ... ok
test tests::tentacle_notes_appends ... ok
test tests::tentacle_tick_marks_step_done ... ok
test tests::tentacle_context_is_write_protected_for_subagents ... ok
test tests::tentacle_handoff_is_one_shot ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 64 filtered out; finished in 0.00s

```


## [16] tool runtime 全测

**命令：**

```bash
cargo test -p jarvis-tools --lib
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.28s
     Running unittests src/lib.rs (target/debug/deps/jarvis_tools-42a7a0af84d5e101)

running 18 tests
test tests::allowed_tool_succeeds ... ok
test tests::blocked_tool_overrides_allow_list ... ok
test tests::denied_call_records_denied_audit_status ... ok
test tests::plugin_install_registers_its_tools ... ok
test tests::plugin_install_rejects_duplicate_id ... ok
test tests::plugin_unregister_removes_provenance ... ok
test tests::sandbox_allow_program_lets_it_through ... ok
test tests::sandbox_args_too_long_denied ... ok
test tests::sandbox_cwd_must_be_allowed_when_restricted ... ok
test tests::sandbox_default_denies_everything ... ok
test tests::sandbox_rejects_forbidden_pattern_in_args ... ok
test tests::confirmation_required_pauses_until_user_confirms ... ok
test tests::denied_call_still_audited ... ok
test tests::tool_call_creates_audit_log_entry ... ok
test tests::every_call_writes_to_raw_event_log ... ok
test tests::unknown_tool_is_unavailable ... ok
test tests::tool_outside_scope_denied ... ok
test tests::slow_tool_times_out ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s

```


## [17.A] memory write × 3

**命令：**

```bash
jarvis memory write 用户偏好 vim 编辑器; jarvis memory write 周末喜欢做菜; jarvis memory write Mirage 项目用 Riverpod
```

**输出：**

```
wrote mem_e74da9a48bda status=Approved trust=0.95
wrote mem_8e364554cd7d status=Approved trust=0.95
wrote mem_8ddc46b69652 status=Approved trust=0.95
```


## [17.B] memory list

**命令：**

```bash
jarvis memory list
```

**输出：**

```
[preference_memory] 用户偏好 vim 编辑器 trust=0.95
[preference_memory] 周末喜欢做菜 trust=0.95
[preference_memory] Mirage 项目用 Riverpod trust=0.95
```


## [18.A] memory search vim（hybrid score 排序）

**命令：**

```bash
jarvis memory search vim 5
```

**输出：**

```
   0.037  trust=0.95  [preference_memory] 用户偏好 vim 编辑器  id=mem_e74da9a48bda
   0.000  trust=0.95  [preference_memory] 周末喜欢做菜  id=mem_8e364554cd7d
   0.000  trust=0.95  [preference_memory] Mirage 项目用 Riverpod  id=mem_8ddc46b69652
```


## [18.B] memory search Mirage

**命令：**

```bash
jarvis memory search Mirage 5
```

**输出：**

```
   0.037  trust=0.95  [preference_memory] Mirage 项目用 Riverpod  id=mem_8ddc46b69652
   0.000  trust=0.95  [preference_memory] 用户偏好 vim 编辑器  id=mem_e74da9a48bda
   0.000  trust=0.95  [preference_memory] 周末喜欢做菜  id=mem_8e364554cd7d
```


## [18.C] memory forget + list 隐藏

**命令：**

```bash
jarvis memory forget <id> 隐私清理; jarvis memory list
```

**输出：**

```
deprecated mem_e74da9a48bda (reason=隐私清理)
[preference_memory] 周末喜欢做菜 trust=0.95
[preference_memory] Mirage 项目用 Riverpod trust=0.95
```


## [18.D] memory-history 完整变更链

**命令：**

```bash
jarvis memory-history <id>
```

**输出：**

```
─── memory mem_e74da9a48bda ─── 2 entries ───
  [2026-05-07 04:08:44] created module=memory_manager reason="CLI"
  [2026-05-07 04:08:44] deprecated module=cli reason="隐私清理"
```


## [19] dream maintenance lint+cluster

**命令：**

```bash
jarvis maintenance global
```

**输出：**

```
lint: duplicates_deprecated=0 scratch_purged=0 inferences_expired=0 weak_lessons=0 conflicts_dampened=0
cluster: clusters_created=0 members_absorbed=0
```


## [20.A] persona set + get（JSON）

**命令：**

```bash
jarvis persona set '{"style":"terse","quirks":["loves rust"],"language":"zh"}'; jarvis persona get
```

**输出：**

```
persona scope=global updated
scope=global updated_at=2026-05-07T04:08:44.306909788+00:00 content={"style":"terse","quirks":["loves rust"],"language":"zh"}
```


## [20.B] persona set + get（纯文本自动包装为 JSON 字符串）

**命令：**

```bash
jarvis persona set "easygoing assistant"; jarvis persona get
```

**输出：**

```
persona scope=global updated
scope=global updated_at=2026-05-07T04:08:44.328828778+00:00 content="easygoing assistant"
```


## [21] hybrid retrieval lib tests（FTS5+vec+jaccard+情绪）

**命令：**

```bash
cargo test -p jarvis-memory --lib retrieval
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/lib.rs (target/debug/deps/jarvis_memory-1e6ba09fc013ea22)

running 2 tests
test tests::fts5_retrieval_finds_keyword_match ... ok
test tests::retrieval_returns_top_match_for_token_overlap ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out; finished in 0.03s

```


## [22] compression / dynamic threshold lib tests

**命令：**

```bash
cargo test -p jarvis-memory --lib compression
```

**输出：**

```
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 44 filtered out; finished in 0.00s
```


## [23.A] trace-view <trace_id>

**命令：**

```bash
jarvis trace-view <trace_id>
```

**输出：**

```
─── trace trc_4408de12c350 ─── 1 events ───
  [04:08:44.349]    11 user_message agent=- session=None 测试 trace
```


## [23.B] trace <trace_id>（raw events）

**命令：**

```bash
jarvis trace <trace_id>
```

**输出：**

```
   11 2026-05-07T04:08:44.349934920+00:00 [user_message] 测试 trace
```


## [23.C] outbox

**命令：**

```bash
jarvis outbox
```

**输出：**

```
pending outbox rows: 0
```


## [23.D] dashboard / dashboard --json

**命令：**

```bash
jarvis dashboard; jarvis dashboard --json
```

**输出：**

```
active_sessions=1 raw_events=11 memories=3 pending_outbox=0
{"active_sessions":1,"memories":3,"pending_outbox":0,"raw_events":11,"route_decisions":11}
```


## [24] 不可变日志触发器拒绝 UPDATE/DELETE（jarvis-db 28 测试全过）

**命令：**

```bash
cargo test -p jarvis-db --lib
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/lib.rs (target/debug/deps/jarvis_db-3f9806d5d25fd655)

running 28 tests
test tests::audit_log_blocks_update ... ok
test tests::audit_log_round_trip ... ok
test tests::append_message_touches_session_last_active ... ok
test tests::audit_log_filters_by_session_and_actor ... ok
test tests::list_recent_returns_only_active_sessions ... ok
test tests::fact_memory_emotion_is_forced_neutral_on_write ... ok
test tests::memory_update_records_before_and_after ... ok
test tests::memory_write_records_change_log ... ok
test tests::list_for_session_returns_ordered_rows ... ok
test tests::outbox_delete_blocked ... ok
test tests::outbox_enqueue_and_drain ... ok
test tests::outbox_seq_strictly_monotonic ... ok
test tests::outbox_update_to_other_columns_blocked ... ok
test tests::provenance_replay_returns_baseline_and_subsequent_events ... ok
test tests::provenance_trace_events_returns_in_order ... ok
test tests::raw_event_checksum_verifies ... ok
test tests::raw_event_log_auto_populates_safe_content_for_secrets ... ok
test tests::raw_event_log_blocks_delete ... ok
test tests::redactor_masks_anthropic_api_key ... ok
test tests::redactor_masks_authorization_header ... ok
test tests::redactor_masks_email_and_phone ... ok
test tests::redactor_returns_empty_hits_for_clean_text ... ok
test tests::raw_event_log_blocks_update ... ok
test tests::raw_event_log_leaves_safe_content_none_when_clean ... ok
test tests::raw_event_seq_is_monotonic ... ok
test tests::session_snapshot_blocks_update ... ok
test tests::session_snapshot_seq_monotonic_per_session ... ok
test tests::session_upsert_and_get ... ok

test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s

```


## [25.A] growth events route_decision

**命令：**

```bash
jarvis growth events route_decision
```

**输出：**

```
[2026-05-07T04:08:44.421305931+00:00] router route_decision {"agent_type":"general","confidence":0.30000001192092896,"fallback_used":false,"mention_override":false,"primary_intent":"chat","session_action":"create_new"}
[2026-05-07T04:08:25.362561936+00:00] router route_decision {"agent_type":"devops","confidence":0.699999988079071,"fallback_used":false,"mention_override":false,"primary_intent":"devops.networking","session_action":"continue_existing"}
[2026-05-07T04:08:25.229836554+00:00] router route_decision {"agent_type":"devops","confidence":0.800000011920929,"fallback_used":false,"mention_override":false,"primary_intent":"devops.networking","session_action":"create_new"}
[2026-05-07T04:08:16.497621315+00:00] router route_decision {"agent_type":"general","confidence":0.30000001192092896,"fallback_used":false,"mention_override":false,"primary_intent":"chat","session_action":"create_new"}
[2026-05-07T04:08:16.404913802+00:00] router route_decision {"agent_type":"general","confidence":0.30000001192092896,"fallback_used":false,"mention_override":false,"primary_intent":"chat","session_action":"create_new"}
[2026-05-07T04:08:16.311081404+00:00] router route_decision {"agent_type":"orchestrator","confidence":0.8750000596046448,"fallback_used":false,"mention_override":true,"primary_intent":"coding.refactor","session_action":"create_new"}
[2026-05-07T04:08:16.219150181+00:00] router route_decision {"agent_type":"coding","confidence":0.800000011920929,"fallback_used":false,"mention_override":true,"primary_intent":"devops.networking","session_action":"create_new"}
[2026-05-07T04:07:55.113826660+00:00] router route_decision {"agent_type":"devops","confidence":0.800000011920929,"fallback_used":false,"mention_override":false,"primary_intent":"devops.networking","session_action":"create_new"}
[2026-05-07T04:07:55.028386093+00:00] router route_decision {"agent_type":"orchestrator","confidence":0.8999999761581421,"fallback_used":false,"mention_override":false,"primary_intent":"memory.update","session_action":"create_new"}
[2026-05-07T04:07:54.887847198+00:00] router route_decision {"agent_type":"creative","confidence":0.675000011920929,"fallback_used":false,"mention_override":false,"primary_intent":"creative.icon","session_action":"create_new"}
```


## [25.B] growth artifacts

**命令：**

```bash
jarvis growth artifacts
```

**输出：**

```
```


## [26] skills 列表

**命令：**

```bash
jarvis skills
```

**输出：**

```
```


## [27] workspace lock + session 串行

**命令：**

```bash
cargo test -p jarvis-orchestrator --lib lock
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.24s
     Running unittests src/lib.rs (target/debug/deps/jarvis_orchestrator-a1801a000f819e6f)

running 8 tests
test tests::walkthrough_test_failure_blocks_auto_approve ... ok
test tests::durable_lock_clear_stale_removes_old_files ... ok
test tests::walkthrough_too_many_files_blocks_auto_approve ... ok
test tests::durable_lock_releases_on_drop ... ok
test tests::workspace_lock_releases_on_drop ... ok
test tests::workspace_writer_blocks_readers ... ok
test tests::durable_lock_is_exclusive_across_acquires ... ok
test tests::workspace_writer_lock_excludes_other_writers ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 61 filtered out; finished in 0.00s

```


## [28] HTTP API 全测

**命令：**

```bash
cargo test -p jarvis-api --lib
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.40s
     Running unittests src/lib.rs (target/debug/deps/jarvis_api-df1ce857becfa7ab)

running 10 tests
test tests::dashboard_html_contains_known_tile_keys ... ok
test tests::audit_returns_array ... ok
test tests::dashboard_metrics_returns_required_fields ... ok
test tests::end_to_end_serve_and_client ... ok
test tests::get_session_returns_404_when_missing ... ok
test tests::healthz_returns_ok_payload ... ok
test tests::list_memories_returns_empty_for_unused_scope ... ok
test tests::session_messages_returns_chronological ... ok
test tests::recent_sessions_returns_array ... ok
test tests::sse_stream_emits_existing_raw_events_on_connect ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

```


## [Codex 真 e2e] 单测 ignored × 6 全过

**命令：**

```bash
cargo test -p jarvis-codex --lib -- --ignored
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/jarvis_codex-42fd984fb03875bc)

running 6 tests
test tests::real_codex_english_casual_input ... ok
test tests::real_codex_continue_existing_path ... ok
test tests::real_codex_respects_allowed_agents_constraint ... ok
test tests::real_codex_long_input_with_rule_hints ... ok
test tests::real_codex_through_router ... ok
test tests::real_codex_smoke ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 30.70s

```


## [Codex 真 e2e] real_codex_smoke --nocapture（13s 真 codex 输出）

**命令：**

```bash
cargo test -p jarvis-codex --lib real_codex_smoke -- --ignored --nocapture
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s
     Running unittests src/lib.rs (target/debug/deps/jarvis_codex-42fd984fb03875bc)

running 1 test
real codex outcome: JudgeOutcome {
    primary_intent: "排查 OpenWrt 编译错误 `no rule to make target`，需要定位 Makefile/依赖/路径/target 规则缺失等构建问题。",
    secondary_intents: [
        "收集完整报错日志、目标包名、OpenWrt 版本、执行的 make 命令和最近改动",
        "指导用户进行构建系统诊断与修复",
    ],
    domain: "software_engineering",
    topic: "OpenWrt build error: no rule to make target",
    session_action: CreateNew,
    agent_type: "coding",
    confidence: 0.86,
    clarification_needed: true,
    router_notes: "这是编译/构建系统故障排查，应路由到 coding。需要用户提供完整错误上下文，尤其是报错前后日志、执行命令、涉及的 package/Makefile 路径、OpenWrt 分支版本以及是否修改过 feeds 或 package。",
}
test tests::real_codex_smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 9.15s

```


## [Codex 真 e2e] real_codex_through_router --nocapture（19s 完整 Router 链路）

**命令：**

```bash
cargo test -p jarvis-codex --lib real_codex_through_router -- --ignored --nocapture
```

**输出：**

```
     Running unittests src/lib.rs (target/debug/deps/jarvis_codex-42fd984fb03875bc)

running 1 test
router+codex decision: RouteDecision {
    trace_id: "trc_466dbc3a8243",
    task_id: "task_dd1a2764dcec",
    primary_intent: "debug OpenWrt build error: no rule to make target",
    secondary_intents: [
        "identify likely Makefile/package dependency issue",
        "guide troubleshooting of firmware build environment",
    ],
    domain: "software_engineering",
    topic: "OpenWrt compilation error no rule to make target",
    session_action: CreateNew,
    target_session_id: None,
    agent_type: "coding",
    preferred_runtime_mode: WorkerProcess,
    confidence: 0.86,
    clarification_needed: true,
    memory_read: true,
    memory_write: false,
    skill_candidates: [],
    tool_scope: ToolScope {
        allowed_tools: [
            "read_file",
            "read_config",
            "list_dir",
            "inspect_system",
            "logread",
            "web.search",
            "shell.exec",
        ],
        blocked_tools: [],
        requires_confirmation: [
            "shell.exec",
            "modify_config",
            "restart_service",
            "modify_firewall",
            "router_config",
        ],
        max_tool_calls: 40,
        max_parallel_tools: 2,
    },
    router_notes: "rule hint: devops.networking (w=0.4) | judge: trace_id: trc_466dbc3a8243\ntask_id: task_dd1a2764dcec\nRoute to coding because the user reports a build/compile error. devops/networking is secondary due to OpenWrt context, but the immediate need is debugging build rules, Makefiles, feeds, or missing targets. Ask for the full error log, package name, OpenWrt version/branch, target device, and recent config/feed changes.",
    mention_override: false,
    forced_sub_agents: [],
    override_action: None,
    steer_target_sub_task_id: None,
    steer_content: None,
    fallback_used: false,
}
test tests::real_codex_through_router ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 10.82s

```


## [Judge probe] 真 codex 命令式可达性测试

**命令：**

```bash
JARVIS_JUDGE=codex jarvis judge probe
```

**输出：**

```
judge probe ok (6670ms): agent=general confidence=0.99 fallback_used=false notes=trace_id: trc-probe; task_id: t-probe
```


## [CLI 单测全集（jarvis-cli 34 个测试）]

**命令：**

```bash
cargo test -p jarvis-cli --lib
```

**输出：**

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.91s
     Running unittests src/lib.rs (target/debug/deps/jarvis_cli-887e64cba91f1be4)

running 34 tests
test cmd::tests::cmd_activity_cards_rejects_empty_session ... ok
test cmd::tests::cmd_audit_lists_session_audit_entries ... ok
test cmd::tests::cmd_judge_probe_reports_outcome_for_stub_judge ... ok
test cmd::tests::cmd_judge_probe_returns_err_when_adapter_unavailable ... ok
test cmd::tests::cmd_dashboard_summary_json_emits_valid_payload ... ok
test cmd::tests::cmd_activity_cards_lists_session_cards ... ok
test cmd::tests::cmd_memory_forget_marks_deprecated_and_logs_change ... ok
test cmd::tests::cmd_dashboard_summary_reports_seeded_state ... ok
test cmd::tests::cmd_memory_forget_returns_error_for_unknown_id ... ok
test cmd::tests::cmd_memory_history_returns_full_change_log ... ok
test cmd::tests::cmd_memory_write_rejects_empty_content ... ok
test cmd::tests::cmd_memory_write_and_list_round_trip ... ok
test cmd::tests::cmd_memory_search_rejects_empty_query ... ok
test cmd::tests::cmd_memory_search_ranks_query_relevant_first ... ok
test cmd::tests::cmd_persona_set_get_round_trip_json ... ok
test cmd::tests::cmd_persona_set_rejects_empty_content ... ok
test cmd::tests::cmd_outbox_pending_reports_zero_for_empty ... ok
test cmd::tests::cmd_persona_get_returns_empty_marker_when_absent ... ok
test cmd::tests::cmd_persona_set_wraps_plain_text_as_json_string ... ok
test cmd::tests::cmd_raw_log_rejects_missing_session ... ok
test cmd::tests::cmd_route_rejects_empty_input ... ok
test cmd::tests::cmd_session_messages_returns_chronological ... ok
test cmd::tests::cmd_sessions_archive_hides_from_list ... ok
test cmd::tests::cmd_sessions_archive_unknown_id_errors ... ok
test cmd::tests::cmd_sessions_list_returns_active_sessions ... ok
test cmd::tests::cmd_raw_log_returns_session_events ... ok
test cmd::tests::cmd_sessions_new_creates_active_session ... ok
test cmd::tests::cmd_sessions_new_rejects_empty_title ... ok
test cmd::tests::cmd_route_returns_pretty_json ... ok
test cmd::tests::cmd_skills_list_returns_registered_skills ... ok
test cmd::tests::cmd_route_with_judge_uses_judge_outcome ... ok
test cmd::tests::cmd_trace_view_pretty_prints_events_for_a_trace ... ok
test cmd::tests::cmd_walkthrough_list_and_approve_round_trip ... ok
test cmd::tests::cmd_walkthrough_reject_records_actor ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s

```


## [Codex adapter 全部单测（含 4 个 fake-codex unit + 6 个真 codex e2e ignored）]

**命令：**

```bash
cargo test -p jarvis-codex --lib（默认运行）
```

**输出：**

```
test tests::judge_returns_none_on_missing_binary ... ok
test tests::judge_parses_well_formed_response ... ok
test tests::judge_returns_none_on_garbage_output ... ok
test tests::real_codex_continue_existing_path ... ignored
test tests::real_codex_english_casual_input ... ignored
test tests::real_codex_long_input_with_rule_hints ... ignored
test tests::real_codex_respects_allowed_agents_constraint ... ignored
test tests::real_codex_smoke ... ignored
test tests::real_codex_through_router ... ignored
test tests::judge_returns_none_on_nonzero_exit ... ok
test tests::judge_handles_concurrent_calls ... ok
test tests::judge_times_out_and_returns_none ... ok

test result: ok. 6 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.52s

```


## [全 workspace 总计 311 通过 / 0 失败 / 6 ignored]

**命令：**

```bash
cargo test --workspace
```

**输出：**

```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

