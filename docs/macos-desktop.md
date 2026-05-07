# Jarvis macOS 桌面端规划

## 目标

让 Jarvis 在 macOS 上以"原生应用"形态出现，覆盖 PRD §8.13 协作面板和
§8.17 commands.json 一键操作的全部交互能力。后台运行时（Router /
Orchestrator / 子 Agent / Memory / Dream）继续由现有 Rust workspace
承担——桌面端**只做 UI + 状态可视化 + 用户输入路由**。

设计原则：

1. 后台和 UI 严格分离。UI 死了，后台继续跑；后台升级，UI 不需要重启。
2. UI 不直接读 SQLite。所有数据走 `jarvis-api` 的 HTTP / SSE。
3. macOS 体验优先：状态栏图标、Spotlight 风格全局唤起、协作面板贴近系统观感。
4. 单一可执行文件 + 一个 dock 图标即可使用，零配置 install。

## 技术选型对比

| 选项 | 优势 | 劣势 | 选不选 |
|---|---|---|---|
| **SwiftUI 原生** | 原生体验最好，Sequoia/Tahoe 私有动画/手势全用得上；和系统通知/全局热键/菜单栏集成最干净 | 不复用 Rust 类型；要维护 Swift 端的协议层；CI 必须 macOS-only | ✅ 主要方向 |
| **Tauri（Rust + WebView）** | 直接复用 jarvis-* crate；前端可用 React/Solid/Svelte | macOS 系统集成不如原生；WebView 在 mini-app 形态下偏笨重；菜单栏图标需要系统接口绕路 | 备选（次推荐） |
| **egui（immediate-mode Rust）** | 100% Rust；零 web 栈；图省事 | 不是原生 Cocoa 控件，长期看视觉违和；窗口管理/全局热键得自己写 | 否 |
| **Electron** | 生态成熟 | 包体大 / 内存占用 / 平台违和 | 否 |

**结论**：**SwiftUI 原生**为主，**Tauri**作为可选 fallback（用于 Windows /
Linux 的二阶段铺开）。先做 macOS 原生。

## 架构

```
┌──────────────────────────────────────────────────────────┐
│                  macOS 桌面端 (Swift / SwiftUI)            │
│                                                          │
│   ┌──────────────┐   ┌───────────────┐  ┌─────────────┐  │
│   │ Menubar Icon │   │ Composer 主窗口 │  │ Spotlight 唤起 │  │
│   └──────┬───────┘   └───────┬────────┘  └──────┬──────┘  │
│          ▼                   ▼                   ▼        │
│   ┌────────────────────────────────────────────────────┐ │
│   │      JarvisClient (Swift)                          │ │
│   │  - REST  : route / memory / sessions / walkthrough │ │
│   │  - SSE   : sessions/{id}/stream                    │ │
│   │  - Polls : /dashboard/metrics                      │ │
│   └─────────────────┬──────────────────────────────────┘ │
└─────────────────────┼────────────────────────────────────┘
                      │ HTTP/1.1 over loopback
                      ▼
┌──────────────────────────────────────────────────────────┐
│               jarvis-api (Rust, 127.0.0.1:7777)          │
│   Router · Orchestrator · Memory · Growth · Dream …      │
│   SQLite 在 ~/Library/Application Support/Jarvis/db      │
└──────────────────────────────────────────────────────────┘
```

后端启动方式：

- 桌面端首次启动检测 7777 端口；没人监听就 fork 一个嵌入的
  `jarvisd` （= 当前 `jarvis serve` + 启动参数）。
- `jarvisd` 注册成 `LaunchAgent`，登录时自动启动；卸载时清理。

## 模块拆分

```
JarvisMac/
  App/
    JarvisApp.swift            @main + scene 选择
    AppDelegate.swift          系统事件 / launchd 注册
  Core/
    JarvisClient.swift         URLSession 封装；POST /router/input 等
    StreamClient.swift         EventSource over URLSession.dataTask
    Models/                    Codable 镜像 jarvis-core 的类型
      RouteDecision.swift
      Session.swift
      Memory.swift
      WalkthroughDoc.swift
      ActivityCard.swift
  Features/
    Composer/                  主输入框 + RouteDecision 预览
    Sessions/                  会话列表 + 时间线
    CollaborationPanel/        协作面板 (§8.13.4 渲染规则)
    Memory/                    Memory 浏览 / 编辑 / 历史
    Dashboard/                 头部 tile + 拓扑/指标图
    CommandsBar/               commands.json 一键按钮（§8.17）
  Services/
    DaemonSupervisor.swift     检测 / 启动 / 重启 jarvisd
    HotkeyService.swift        全局快捷键（GlobalShortcut/CGEventTap）
    NotificationService.swift  WaitingUser ActivityCard → 系统通知
    ApplePersistence.swift     Keychain 存 ANTHROPIC_API_KEY
  Resources/
    Assets.xcassets
    Localizable.strings
JarvisMacTests/
  JarvisClientTests.swift
  StreamClientTests.swift
  ModelsRoundTripTests.swift
```

## 后端需要补的 API（已有 / 待加）

✅ 已就绪（v1.0 已实现）

```
POST /router/input
GET  /sessions/recent
GET  /sessions/{id}
GET  /sessions/{id}/messages
GET  /sessions/{id}/stream    (SSE)
GET  /memory/{scope}
POST /memory
GET  /raw-log/{session}
GET  /audit/{session}
GET  /trace/{trace_id}
GET  /walkthrough/{session}
POST /walkthrough/{id}/approve
POST /walkthrough/{id}/reject
GET  /growth/events
GET  /growth/artifacts
GET  /dashboard/metrics
GET  /dashboard
POST /steer
POST /interrupt
POST /maintenance/lint
GET  /healthz
```

⬜ 待加（桌面端的需求驱动）

| 路径 | 用途 |
|---|---|
| `POST /sessions` | UI 显式新建 session（带 title / topic）|
| `DELETE /sessions/{id}` | 归档而非物理删除（status → archived）|
| `POST /commands/{id}/run` | 执行 commands.json 条目，返回 execution_id |
| `GET /commands/execution/{id}` | 跟踪 CommandExecution 进度 |
| `GET /activity-cards/{session}` | 协作面板数据源（已有 ActivityCardStore，需加 endpoint）|
| `POST /memory/{id}/forget` | 用户主动遗忘（status → deprecated + change_log）|
| `POST /persona` | 写入 persona.md（用户在 UI 里编辑）|
| `GET  /persona` | 读取 persona.md 当前内容 |

## 桌面端关键交互

### 1. 全局唤起（Compact 态，语音优先）

桌面端有两种形态：**Compact**（语音优先的悬浮窗，最轻量入口）和
**Expanded**（功能完整的主窗口，承载协作面板 / Memory / commands）。
Compact 是默认；满足触发条件或用户主动展开时切到 Expanded。

热键 `⌘ + Shift + J` 唤起 Compact，默认无 chrome 悬浮窗，最大宽
720pt，停在屏幕上半 1/3 居中。AI 助手的本体是"对话"，所以 Compact
默认就是语音——不该需要打字才能用它。

#### 1.1 三个子状态（语音模式）

```
   Idle                       Listening                  Replying
┌────────────┐            ┌────────────┐             ┌─────────────────┐
│            │            │            │             │ 💻 coding       │
│    🎙️      │ ─VAD start→│  ▁▃▆█▆▃▁  │ ─VAD stop→  │                 │
│            │            │ "帮我..."   │  router→    │ 🔊 (speaking…)  │
│            │            │            │  TTS→       │  ▁▂▁▃▂▁         │
└────────────┘            └────────────┘             └─────────────────┘
breathing mic            live waveform               TTS pulse + barge-in
```

- **Auto-VAD**：开口即录、停 600ms 即送 STT；长按麦克风图标改成
  push-to-talk，办公室环境用
- **Barge-in**：TTS 播放中检测到语音输入立刻打断当前回放，避免抢话
- **Replying** 中显示最近一条文本预览（≤3 行），滚动消失，不喧宾夺主

#### 1.2 文本兜底（无麦克风权限或用户偏好）

首次启动若用户拒绝麦克风权限、或在偏好里关闭语音：

```
┌──────────────────────────────────────┐
│ 💬 ____________________________  🎙️  │
└──────────────────────────────────────┘
   ↩ 发送 · ⌘↑ 展开 · 🎙️ 按住单次录入
```

文本输入框右侧的 🎙️ 按钮按住开始录、松开 STT 转文本填进输入框——
权限被拒的用户失去 hands-free，但单次语音录入仍可用；权限完全没给
则该按钮灰掉，hover 提示去 System Settings 启用。

#### 1.3 何时自动展开主窗口

Compact 默认不开主窗口；满足任一条件自动展开（保留 session 上下文）：

- `RouteDecision.agent_type == "orchestrator"`（多 Agent 协作）
- 任意 `ActivityCard.status == "waiting_user"`
- 流式回复包含 artifact / walkthrough 引用
- 语音输入里识别到 "打开窗口" / "show me" 等显式信号
- 用户主动按 `⌘↑`

主窗口和 Compact 是两个独立 `NSWindow`，共享同一个 `AppState`
`ObservableObject`；`session_id` 不变，转场不丢上下文。`⌘↓` / `Esc`
从 Expanded 收回 Compact。

收到 RouteDecision 后的分发逻辑：

- 单 Agent + 高置信度 → Compact 内直接 TTS 播报回复，不开窗
- clarification_needed → Compact 内 TTS 播报澄清问题，等下一轮输入
- 任一展开触发命中 → Expanded 主窗口接管，Compact 平滑收起

#### 1.4 STT / TTS 技术栈

| 层 | MVP（系统自带，零下载） | 长期可换（中文质量更好） |
|---|---|---|
| STT | `SFSpeechRecognizer` + `requiresOnDeviceRecognition = true`（macOS 13+） | `whisper.cpp` + `ggml-medium-zh` |
| TTS | `AVSpeechSynthesizer` + Premium 中文嗓音（`Tingting / Lili`，首次启动引导下载） | Piper（自然度更好但要打包模型） |
| VAD | `AVAudioEngine` + RMS 阈值 | webrtc-vad / Silero |

所有 STT/TTS **一律本地推理**；不送任何音频到云端。`Info.plist`
必须含 `NSMicrophoneUsageDescription` +
`NSSpeechRecognitionUsageDescription`。首次拒绝后**永久走文本兜底**，
不再弹窗骚扰。

#### 1.5 隐私指示

Mic hot（正在录音或 VAD 监听）时菜单栏图标加一颗红点，与 macOS
系统麦克风指示灯保持一致。Idle 状态没有任何音频采集——VAD 只在
用户唤起 Compact 后才启动。

### 2. 协作面板渲染

数据源：`GET /sessions/{id}/stream`（SSE）+ `GET /activity-cards/{session}`。

落实 PRD §8.13.4 排列规则：`waiting_user` 卡片置顶；`running` 按
启动时间；`success` 折叠到底部。卡片状态机由 `ActivityCard.status`
驱动，Swift 侧用 `@State` + `withAnimation` 做平滑过渡。

### 3. 一键操作栏（commands.json）

主窗口顶部水平条，每个 CommandDefinition 渲染一颗 SF Symbol 图标
按钮：

- `🔀 拉主分支 + 解冲突`
- `📋 生成 Walkthrough + 创建 PR`
- `🧪 发版前回归检查`
- `⚡ 并行探索`
- `🎯 方向调整`（仅在有运行中的子 Agent 时点亮）

点击 → `POST /commands/{id}/run` → 拿 execution_id → 弹出
ProgressSheet 监听 `GET /commands/execution/{id}`。

### 4. 全屏 Memory 浏览器

- 左：scope 树（global / project / session）
- 中：memory 列表，按 trust_score 排序，过滤器（type / tier / 情绪极性）
- 右：选中 memory 的详情 + change_log 时间线
- 底部固定 toolbar：`Edit` / `Forget` / `Verify` / `Reject`

### 5. 状态栏图标

显示 `pending_outbox` 数量徽标 + 当前活跃 Agent 数。点开是 8-row
mini 仪表盘 + 「打开主窗口」/「打开 dashboard」/「Quit jarvisd」。

## 开发路线（建议 4 阶段）

### m1 — 玩具版（2 周）

- 单窗口 SwiftUI 应用 + JarvisClient 最小实现（route + memory list）
- 后端通过用户手动 `jarvis serve` 启动，不做 daemon supervisor
- 输出：能在 macOS 输入一句话、看到 RouteDecision 的 JSON 渲染

### m2 — 协作面板可用（2 周）

- SSE 客户端打通
- ActivityCard 渲染 + 状态机
- 添加缺失的 API（`/sessions` POST/DELETE、`/activity-cards/{session}`）
- 全局热键 + Composer 弹窗
- 输出：能用桌面端跑一次"重构 sync 模块"，看到子 Agent 协作面板

### m3 — 一键操作 + Memory 浏览器（3 周）

- commands.json 渲染 + 执行进度跟踪
- Memory 浏览器 + change_log 时间线
- Persona 编辑器（直接落到 ~/.jarvis/profiles/<user>/persona.md）
- DaemonSupervisor + LaunchAgent 注册
- 输出：日常工作可以全程不开终端

### m4 — 抛光 + 发布（2 周）

- 状态栏图标 + 通知中心集成
- 偏好窗口（API Key、热键、日志级别、JARVIS_DB 位置）
- 自动更新（Sparkle 或 GitHub Releases + ed25519 签名）
- 文档 + 截图 + 上 TestFlight / 公网下载

## 测试与 CI

- 单元测试：`JarvisClient` 用 mock URLProtocol 拦截 HTTP；
  `StreamClient` 用 in-memory NSStream pair。
- 端到端：GitHub Actions macOS runner 起 `jarvis serve` +
  XCUITest 跑核心路径（输入 → 看到 RouteDecision）。
- 后端契约由 `crates/jarvis-api` 的 OpenAPI（待生成）保证；Swift 侧
  从 OpenAPI codegen 出 Models 减少漂移。

## 开放问题

1. **多用户 / Touch ID 解锁敏感记忆**：第二阶段考虑；现在记忆都在
   本地 SQLite 文件里，由 macOS FileVault 保护已够。
2. **iOS 同步**：靠 `jarvis-control::Replicator` + outbox 实现，但要
   先决定一个对端协议（CRDT vs operational transform vs full replay）。
3. **离线 LLM**：Ollama / MLX 支持，可作为 `LlmJudge` 的第三个 adapter。
4. **快捷小工具**：是否在 macOS 提供 share extension（"发送选区给
   Jarvis"），值得评估。

## 与现有 Rust workspace 的关系

新建一个 `desktop/macos/` 目录（Xcode project + Swift package
manifest）。Rust workspace **不**改动；macOS 端只通过 HTTP 与
`jarvis-api` 通信。这样：

- Rust 端的 CI（`cargo test` / `cargo clippy`）保持现状
- Swift 端的 CI 完全独立，不影响后端
- 任何后端更新走 API 版本号，桌面端按需 bump JarvisClient 的
  契约
