# Jarvis macOS — 视觉设计系统

姊妹文档：[`docs/macos-desktop.md`](../macos-desktop.md)（架构 / 模块 /
roadmap）。本文只管视觉与组件——色 / 字 / 间距 / 圆角 / 动效，以及
关键屏幕的 wireframe。

## 设计定位

**macOS HIG 骨架 + Claude 内容面**。

- 系统结构（菜单栏、`NSSplitView` 三栏、`NSToolbar`、sheet、菜单）
  完全照 HIG 走，避免"网页应用感"
- 内容承载层（卡片背景、字体、间距、强调色）走 Claude 风格——暖米白
  纸面、serif 标题、橙色仅做一级动作锚点、信息密度低
- 类比：Linear / Things / Notion 的路子。底是 macOS，皮是 Claude

不做的：

- ❌ 多彩 SF Symbols 满天飞（Claude 风格更克制）
- ❌ 重阴影 / 玻璃材质（用 1px 边框 + 8% 透明度替代）
- ❌ 弹性 spring 动效（除 ActivityCard 重排外不用）
- ❌ 渐变背景

## 1. 设计 tokens

### 1.1 色板

**Light**（主模式，纸面感）

| Token | 值 | 用途 |
|---|---|---|
| `bg/primary` | `#FAF9F5` | 主背景（Compact 悬浮窗、Expanded canvas） |
| `bg/secondary` | `#F5F4EE` | 嵌套面板（sidebar / collab panel 底） |
| `bg/tertiary` | `#EFEDE5` | hover / 选中底 |
| `fg/primary` | `#1A1817` | 主文字（带暖色的近黑） |
| `fg/secondary` | `#4A453E` | 二级文字 |
| `fg/muted` | `#6E6A60` | metadata / placeholder |
| `border/default` | `rgba(26,24,23,0.08)` | 默认 1px 边框 |
| `border/strong` | `rgba(26,24,23,0.16)` | 强调边框 / focus ring 收尾 |
| `accent/primary` | `#D97757` | Claude 橙；一级动作 / 品牌点缀 |
| `accent/hover` | `#C56745` | 一级动作 hover |
| `accent/muted` | `rgba(217,119,87,0.12)` | accent 背景态（badge / highlight） |

**Dark**（暗模式，暖暗调）

| Token | 值 | 用途 |
|---|---|---|
| `bg/primary` | `#1A1817` | 主背景 |
| `bg/secondary` | `#252220` | 嵌套面板 |
| `bg/tertiary` | `#302C29` | hover / 选中底 |
| `fg/primary` | `#F5F4EE` | 主文字 |
| `fg/secondary` | `#C5BFB3` | 二级文字 |
| `fg/muted` | `#A39E92` | metadata |
| `border/default` | `rgba(245,244,238,0.08)` | 默认边框 |
| `border/strong` | `rgba(245,244,238,0.16)` | 强调边框 |
| `accent/primary` | `#E58A6E` | accent 在暗底略亮 |
| `accent/hover` | `#EFA084` |  |
| `accent/muted` | `rgba(229,138,110,0.16)` |  |

**Status**（明暗共享语义，值不同）

| Token | Light | Dark | 用途 |
|---|---|---|---|
| `status/success` | `#5C8A5A` | `#7FAB7C` | success ActivityCard |
| `status/warning` | `#C99146` | `#E0AE6A` | waiting_user |
| `status/error` | `#B6463F` | `#D7615A` | error / failed |
| `status/running` | `#5C7AAA` | `#7B9BCB` | running |
| `status/pending` | `fg/muted` | `fg/muted` | 默认中性态 |

### 1.2 字体

| 阶 | 字号 / weight | 字体 | 用途 |
|---|---|---|---|
| Display | 28pt Semibold | SF Pro Rounded | 极少用——仅 onboarding 大标题 |
| H1 | 22pt Medium | **New York** (serif) | section 标题、Memory 浏览器条目主行 |
| H2 | 17pt Medium | New York | 卡片标题、对话回复发起人行 |
| Body | 15pt Regular | SF Pro | 主文 / 输入 / TTS preview |
| Body-tight | 13pt Regular | SF Pro | sidebar 条目、metadata |
| Caption | 11pt Medium | SF Pro | status pill、时间戳 |
| Code | 13pt Regular | SF Mono | trace / diagnostics / JSON 渲染 |

Serif 用 New York（系统自带）做品牌锚点——回复气泡发起人、Memory 条目
主行、空状态文案——其余 UI 文字仍 SF Pro。比例参照 Claude.ai 网页版。

### 1.3 间距 / 圆角

```
spacing:  4 / 8 / 12 / 16 / 24 / 32 / 48
radius:   4 (chip) · 8 (button/input) · 12 (card) · 16 (sheet) · 24 (compact window)
```

布局原则：相邻同级元素 8 / 12，区块间 24，section 间 32。Compact 内
padding 16，Expanded 内 padding 24。Memory / commands 列表行高 ≥ 44
（HIG 触摸命中区）。

### 1.4 阴影

| 场景 | 值 |
|---|---|
| Flat 表面 | `none`（用 `border/default` 替代） |
| Compact 悬浮窗 | `0 8 24 0 rgba(0,0,0,0.13)` |
| Expanded 主窗口 | 系统默认（`NSWindow` 自带） |
| Sheet / Popover | 系统默认 |
| ActivityCard hover | `0 2 8 0 rgba(0,0,0,0.06)` |

### 1.5 动效

| 事件 | 时长 | 缓动 |
|---|---|---|
| 默认（hover / focus / 颜色过渡） | 200ms | ease-out |
| 模式切换（Compact ↔ Expanded） | 320ms | ease-out |
| Compact 状态切换（Idle → Listening → Replying） | 240ms | ease-in-out |
| Mic 呼吸（idle scale 1.00 ↔ 1.04） | 3000ms | ease-in-out infinite |
| TTS Replying 脉冲 | 1500ms | ease-in-out infinite |
| ActivityCard 重排 | spring(stiffness:120, damping:14) | — |
| 波形 amplitude | 60 fps 实时 | — |

Spring 仅 ActivityCard 重排用——其余场景一律 ease-out，避免 bouncy。

## 2. 组件库

| 组件 | 关键规格 |
|---|---|
| `Button.primary` | bg=`accent/primary` · text=`bg/primary` · h=32 · radius=8 · px=14 · 15pt Medium |
| `Button.secondary` | bg=transparent · border=1px `accent/primary` · text=`accent/primary` · 同尺寸 |
| `Button.ghost` | bg=transparent · text=`fg/primary` · 同尺寸 · hover bg=`bg/tertiary` |
| `Button.icon` | 32×32 · radius=8 · hover bg=`bg/tertiary` |
| `Button.pill` | h=28 · radius=14 · px=12 · 用于 commands.json 一键栏 |
| `Input` | bg=`bg/secondary` · border=1px `border/default` · h=32 · radius=8 · px=12 · 15pt |
| `Input.focus` | border=`accent/primary` @ 60% · 无 shadow |
| `Card` | bg=`bg/primary` · border=1px `border/default` · radius=12 · p=16 |
| `StatusPill` | h=20 · radius=10 · px=8 · 11pt Medium · bg=`status/x @ 12%` · text=`status/x` |
| `AgentBadge` | 24 圆 · border=1px `border/default` · 中心 emoji 14pt · 36pt 变体用于 Replying 头 |
| `ActivityCard` | Card 变体；header(badge+name+pill) / body(progress) / footer(actions if waiting) |
| `CommandBar` | toolbar 内水平 pill 列；每项 leading SF Symbol + 14pt label |
| `VoiceMic` | 64 圆 · 待机=`bg/secondary`+1px border · listening=`accent/muted` · replying=灰底 |
| `Waveform` | 7 bar · 4pt 宽 · 2pt gap · 高 8–48pt · listening=accent · replying=`fg/muted` |
| `Sidebar.row` | h=28 · px=12 · hover bg=`bg/tertiary` · 选中 bg=`accent/muted`+text=`accent/primary` |

按钮态总览：

```
Primary           Secondary          Ghost              Icon
┌───────────┐    ┌───────────┐     ┌───────────┐      ┌──┐
│  Approve  │    │  Approve  │     │  Approve  │      │ ⚙ │
└───────────┘    └───────────┘     └───────────┘      └──┘
 bg=accent       border=accent     transparent        32x32
```

StatusPill 总览：

```
running    waiting_user   success    error      pending
[●运行中]  [⏸等待用户]    [✓完成]    [✗失败]    [• 待定]
   蓝         琥珀          绿          红         灰
```

## 3. 关键屏幕 wireframe

### 3.1 Compact / 语音三态

详见 [`macos-desktop.md` §1.1](../macos-desktop.md#11-三个子状态语音模式)。
不在此重复，但视觉规格如下：

- 窗口：520 × 200pt（idle / listening）；600 × 280pt（replying）
- radius=24，shadow=Compact 悬浮窗规格
- 米白纸面（`bg/primary`），无 NSToolbar
- Mic / Waveform 居中，下方 metadata 用 Caption 11pt `fg/muted`

### 3.2 Compact / 文本兜底

```
┌──────────────────────────────────────────────────────┐  ← radius=24
│                                                      │     shadow Compact
│   💬  ___________________________________________ 🎙️ │  ← Input.large
│                                                      │     右侧 IconButton
│                                ↩ 发送 · ⌘↑ 展开      │  ← Caption muted
└──────────────────────────────────────────────────────┘
```

- 单行 Input，font=15pt，placeholder=`fg/muted` "和 Jarvis 说点什么..."
- 右侧 32pt mic IconButton；按住录入；权限拒绝时 disabled 灰掉
- ⌘↑ 展开主窗口；Esc 关闭 Compact

### 3.3 Expanded / 主窗口（三栏）

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🟠 Jarvis                          ⌘K Search  🎙️                  ⚙ │  ← NSToolbar
├──────────────┬─────────────────────────────────────┬────────────────┤
│              │                                     │                │
│ Sessions     │  Composer                           │  🟠 coding     │  ← AgentBadge
│  · Mirage    │ ┌─────────────────────────────────┐ │  running       │     +StatusPill
│  · Sync 重构 │ │ 💬 ____________________________ │ │ ───────────── │
│  · Sketches  │ └─────────────────────────────────┘ │ • Reading sync.ts│
│              │                                     │ • Editing tests │
│ Memory       │  user · 14:02                       │                │
│  · global    │   帮我把 sync 模块重构成异步         │  🟢 verifier   │
│  · project   │                                     │  success       │
│              │  💻 coding · 14:02                   │ ───────────── │
│ Persona      │   先列出依赖关系...                  │                │
│              │   ▏▏▏ (流式)                         │  + Add Agent   │
│              │                                     │                │
│ Commands ↓   │                                     │                │
│ 🔀 拉主分支   │                                     │                │
│ 📋 Walkthrough│                                     │                │
│ 🧪 回归检查   │                                     │                │
└──────────────┴─────────────────────────────────────┴────────────────┘
   ← Sidebar           ← Canvas（对话流）            ← Collab Panel →
   240pt min           flex                          320pt min
   bg/secondary        bg/primary                    bg/secondary
```

- `NSSplitView` 横排三栏；左右栏可折叠（⌘1 / ⌘2）
- Toolbar 高度 38pt；含 logo + ⌘K 搜索胶囊 + 🎙️ 唤起 Compact + 设置
- Canvas 每条对话是一个 Card；user/agent 头用 H2 17pt serif，body 15pt
- Collab Panel 仅在子 Agent 运行时浮现；空时整栏可折叠隐藏
- 命令栏在 Sidebar 底部固定 disclosure section（不抢 toolbar 顶位）

### 3.4 Memory Browser

```
┌─────────────────────────────────────────────────────────────────────┐
│ Memory                                  + 写入   🔍 ____________    │  ← NSToolbar
├──────────────┬─────────────────────────────────────┬────────────────┤
│ Scope        │ Memories  ↓ trust_score · 类型 ▾    │  详情          │
│              │                                     │                │
│ ▸ global     │ ┌─────────────────────────────────┐ │ 用户偏好 vim   │
│   ▸ pref     │ │ 用户偏好 vim 编辑器              │ │ 编辑器          │
│   ▸ skill    │ │ pref · 0.92 · global             │ │                │
│              │ │ 创建于 3 天前                    │ │ id mem_abc...   │
│ ▸ project    │ └─────────────────────────────────┘ │ tier global     │
│ ▸ session    │                                     │ trust 0.92      │
│              │ ┌─────────────────────────────────┐ │ 类型 pref       │
│              │ │ 周末喜欢做菜                     │ │                │
│              │ │ pref · 0.71 · global             │ │ Change log:    │
│              │ │ 创建于 1 天前                    │ │ • created      │
│              │ └─────────────────────────────────┘ │ • verified by   │
│              │                                     │   user 2026-05  │
│              │                                     │                │
│              │                                     │ [Edit] [Forget] │
└──────────────┴─────────────────────────────────────┴────────────────┘
```

- 左：scope 树，`Sidebar.row` 规格；当前选中 accent muted
- 中：Card 列表；标题 serif H2，metadata Caption
- 右：详情面板；底部固定按钮组（Ghost / Secondary）

### 3.5 状态栏 dropdown

```
┌────────────────────────────────┐
│  Jarvis                        │  ← logo + 名称
│  3 agents · 0 pending          │  ← Caption
├────────────────────────────────┤
│  💻 coding             running │  ← ActivityCard 简化版
│  🔍 research          waiting  │
│  🟢 verifier          success  │
├────────────────────────────────┤
│  Open main window         ⌘⇧J  │
│  Open dashboard                │
│  Preferences…             ⌘,   │
├────────────────────────────────┤
│  Quit Jarvis                   │
└────────────────────────────────┘
```

- `NSStatusItem` 标准下拉；padding 8 / row h 32
- mic hot 时左上角 logo 边缘加 4pt 红点

### 3.6 Command 执行 ProgressSheet

```
┌────────────────────────────────────────────────────┐
│  📋 生成 Walkthrough + 创建 PR              ✕     │  ← Sheet header
├────────────────────────────────────────────────────┤
│                                                    │
│  ✓ 1/4  收集 session 上下文                        │
│  ⟳ 2/4  生成 Walkthrough doc                      │  ← spinner
│  ◯ 3/4  Verifier 自查                              │
│  ◯ 4/4  push + 开 PR                              │
│                                                    │
│  ▏▏▏▎▎▎▎▎▎▎▎▎▎▎▎▎▎▎▎▎▎ 50%                       │  ← progress bar
│                                                    │
│           [ Cancel ]   [ Open in Window ]          │
└────────────────────────────────────────────────────┘
```

- `.sheet` modal 挂在 Expanded 主窗口；宽 480pt
- 步骤行：圆 16pt 状态图标 + Body-tight 13pt 描述 + 状态色
- 底部 progress bar 用 accent 色，背景 `bg/tertiary`

## 4. 明暗模式映射

颜色 token 已在 §1.1 给出明暗成对值；**所有组件代码只引用 token 名称**，
不写死颜色。`@Environment(\.colorScheme)` 在 SwiftUI 里切换。

字体 / 间距 / 圆角 / 动效在两种模式下完全一致。阴影在暗模式下 alpha
减半（暗底阴影本来就不显眼，避免黑漆漆一坨）。

## 5. 与 m1–m4 milestones 的对应

参见 [`macos-desktop.md` §开发路线](../macos-desktop.md#开发路线建议-4-阶段)。
本文按 milestone 列各阶段必落的 token / 组件子集：

### m1（玩具版，2 周）

落：

- 全部 token（色 / 字 / 间距 / 圆角 / 动效）一次到位
- `Button.primary` / `Button.ghost` / `Input` / `Card`
- 单窗口 = 简化的 Expanded canvas（无 Sidebar / 无 Collab Panel）

### m2（协作面板可用，3 周；含 Compact 文本态）

落：

- `Sidebar.row` + 三栏 NSSplitView
- `StatusPill` / `AgentBadge` / `ActivityCard`
- Collab Panel
- Compact **文本态**（§3.2 wireframe）+ 全局热键 ⌘⇧J
- 模式切换动效（320ms）

### m3（语音 + 一键操作 + Memory，4 周）

落：

- `VoiceMic` / `Waveform` 组件
- Compact **语音三态**（§3.1）+ 麦克风权限流 + 文本兜底
- STT / TTS 接入（SFSpeechRecognizer + AVSpeechSynthesizer）
- `CommandBar` + Command 执行 Sheet（§3.6）
- Memory Browser（§3.4）

### m4（抛光 + 发布，2 周）

落：

- 状态栏 dropdown（§3.5）
- 偏好窗口（独立 sheet，复用全套组件）
- 暗模式打磨 / 阴影调整
- 动效细节（spring damping、TTS 脉冲节奏）

> **m1 不允许跳过 token 系统**——晚做 token 化的 UI 项目，后期换皮
> 都是在多处魔改，每个 milestone 重做一遍。一次落到位最便宜。

## 6. 不属于本文的事

- 图标素材：用 SF Symbols 6（Rounded weight）+ Claude logo 单一品牌符号
- 文案语调：另起一份 `docs/design/voice-and-tone.md`（待写）
- 国际化：本文按中英混排示例；m4 之前不做完整 i18n
- 可访问性：VoiceOver / 键盘导航 / 高对比度遵循 macOS HIG，不在视觉系统层面单独定义
