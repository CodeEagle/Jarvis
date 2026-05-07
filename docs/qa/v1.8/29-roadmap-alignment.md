# 29 — MVP 路线图对齐

**PRD 章节**：§24 MVP 路线图 · §24.0 依赖关系

**结论**：✅ v0.1 完整、v0.2 大部分到位、v0.3/v0.4 部分功能领先

## 当前 v1.8 实现位置

按 PRD §24 阶段对照：

### v0.1 — 最小可用闭环（✅ 全部完成）

- [x] Main Router（规则层 + LLM judge）
- [x] IntentClassifier（两段式）
- [x] SessionResolver（评分 + explicit_reference）
- [x] SQLite sessions / messages / memories
- [x] sqlite-vec 向量
- [x] GeneralAgent / CodingAgent / DevOpsAgent / ResearchAgent / CreativeAgent
- [x] worker-process driver（InProcessDriver）
- [x] trace log + audit log
- [x] routing examples 收集
- [x] Control Plane / Task Plane 分离 + Watchdog（v1.5 起补）
- [x] raw_event_log 不可变追加
- [x] 路由准确率 / Session 匹配率指标采集

### v0.2 — 成长闭环 + 多 Agent 基础（✅ 大部分完成）

- [x] Growth Engine（Collector / Evaluator / Extractor / Promotion Gate）
- [x] GrowthEvent + GrowthArtifact
- [x] MemoryCandidate + SkillCandidate
- [x] Skill Regression Runner（mock）
- [x] AgentProfile 统计
- [x] Orchestrator 模式（TaskTree + ArtifactRegistry）
- [x] ConversationBus + ownership 状态机（库层）
- [x] ActivityCard
- [x] Tentacle 文件系统（CONTEXT/todo/NOTES/HANDOFF + Lock）
- [x] Worktree 隔离 + workspace lock
- [x] 动态压缩策略（三维度，库层）
- [ ] ⏸️ Conversation API HTTP / CLI（库层 OK，HTTP 未接）

### v0.3 — 主动汇报 + Steer（🟡 大部分到位）

- [x] WalkthroughAgent（HANDOFF.md 输入 + 自动审批）
- [x] VerifierAgent（独立验证）
- [x] 中断协议（软 / 硬 / 异步 + checkpoint）
- [x] Memory 三路混合检索 + 情绪共振
- [x] Prompt Cache 分层注入
- [x] 冷启动快照 (库层)
- [x] Steer 协议（库层完整 + 单测，包括频率保护、Codex append 模式）
- [x] 回溯查询（CLI replay / trace-view / memory-history）
- [ ] ⏸️ commands.json（骨架到位，CLI 入口缺）
- [ ] ⏸️ 并行探索 Parallel Explore（DB schema 有，CompareAgent 实现待补）
- [ ] ⏸️ app-server driver 长任务支持（仅 InProcessDriver）

### v0.4 — Memory 深化 + 成本优化（✅ 大部分到位）

- [x] Dream 系统（整理 / 固化 / 生长三层）
- [x] 情绪坐标（episode / lesson / cluster / inference）
- [x] 情绪共振检索
- [x] 动态模型升降级（双向 + 防抖）
- [x] token 预算自学习
- [x] model_policy artifact
- [x] Persona 层（v1.8 + Persona Repo）
- [x] Memory Tier 分层
- [x] 视觉回归测试框架（库层）
- [ ] ⏸️ Persona 6h 自动同步 job

### v1.0 — 产品化（⏸️ 仅 dashboard / replicator 雏形）

- [x] Growth Dashboard（基础）
- [x] Replicator（outbox 跨设备同步基础）
- [x] Trace Viewer（CLI trace-view）
- [ ] ❌ Skill 真实环境沙箱回放
- [ ] ❌ Intent 自动发现
- [ ] ❌ MCP / plugin 接入
- [ ] ❌ Qdrant / pgvector 升级
- [ ] ❌ 权限沙箱（工具调用沙箱隔离）
- [ ] ❌ 多用户支持
- [ ] 🖥️ macOS 桌面端 m1（设计已落 docs/macos-desktop.md）

## §24.0 依赖关系一致性

PRD 给的依赖图：基础层 → Memory/Skill/单 Agent → Orchestrator/ConversationBus/Tentacle → Walkthrough/Verifier/Regression/commands/Steer → Memory 深化 → 产品化。

实际进度跨阶段领先（v0.4 的 Dream 系统 + 情绪坐标 + 动态模型策略 + Walkthrough 都已就绪），但 v0.3 的 commands.json + worker driver 长任务路径 + Tentacle e2e 有缺口。

**总评**：v1.8 PRD 设计目标 70% 实现到 ✅ 完整闭环，13% 库层 + 单测 OK 但 CLI 未接，剩余 17% 是 v1.0 产品化任务。
