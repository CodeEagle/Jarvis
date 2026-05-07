# Jarvis macOS App — m1 (Swift / SwiftUI)

The lightweight desktop client for the Jarvis daemon. m1 covers a
single window with two tabs (Compose / Memory) calling the local
`jarvis-api` HTTP server. Voice mode, collaboration panel, command
bar, and Memory browser ship in m2–m4. Architectural plan lives in
[`docs/macos-desktop.md`](../../docs/macos-desktop.md); the visual
system in [`docs/design/macos-visual.md`](../../docs/design/macos-visual.md).

## Try it (no Mac dev environment needed)

The app is built unsigned by GitHub Actions on every push. To grab a
fresh build:

1. Open the repo's **Actions** tab → workflow **macos-app**
2. Pick the latest successful run on `main` (or the
   `claude/router-rule-layer-yCIVE` branch)
3. Scroll to **Artifacts**, download `JarvisMac-<sha>.zip`
4. Unzip → you'll have `Jarvis.app`

### Bypass the Gatekeeper warning (one-time)

Because the build isn't code-signed, macOS will refuse to open the
app on first launch ("Jarvis.app is damaged and can't be opened").
Strip the quarantine attribute once:

```sh
xattr -dr com.apple.quarantine ~/Downloads/Jarvis.app
open ~/Downloads/Jarvis.app
```

### Run the daemon

The app expects `jarvis-api` running on `127.0.0.1:7777`. Start it
in a terminal:

```sh
cargo run --release -p jarvis-cli -- serve
# or, after `cargo install --path crates/jarvis-cli`:
jarvis serve
```

Without the daemon, the Compose tab shows a transport error and the
Memory tab shows "无法连接".

## What you can do in m1

- **Compose tab** — type a message, ⌘↩ or click Send → calls
  `POST /router/input`, renders the resulting `RouteDecision` in a
  card (agent, intent, confidence, router notes, trace id)
- **Memory tab** — lists memories from `GET /memory/global`; pull
  `Refresh` to reload

## Build locally on a Mac

```sh
brew install xcodegen
cd desktop/macos
xcodegen generate
open Jarvis.xcodeproj
# ⌘R to run
```

Requirements: macOS 13+ on the build machine; the resulting `.app`
also targets macOS 13+.

## Roadmap

| Milestone | Highlights | Status |
|---|---|---|
| **m1** | Single window · Compose / Memory · JarvisClient (route + memory) | **this commit** |
| m2 | Three-pane NSSplitView · ActivityCard · SSE stream · Compact text composer + global hotkey ⌘⇧J | next |
| m3 | Voice 三态 · STT/TTS (SFSpeech / AVSpeech) · CommandBar · Memory browser · DaemonSupervisor | |
| m4 | Menubar dropdown · Preferences · Auto-update · Dark mode polish | |

See `docs/macos-desktop.md` §"开发路线" for details.

## Caveats (m1)

- Build is unsigned. Gatekeeper workaround above.
- No daemon supervisor — start `jarvis serve` manually for now.
- Single window only. ⌘⇧J global hotkey arrives in m2.
- Light mode only. Dark palette tokens are defined in
  `Theme/Tokens.swift` but not yet wired through.
- New York serif headers degrade to system serif on macOS versions
  that don't ship the font (rare; New York has shipped since macOS
  Big Sur).
- SwiftUI sources have not been built locally in this commit (the
  authoring environment is Linux). First CI run will surface any
  compile errors; iterate from there.
