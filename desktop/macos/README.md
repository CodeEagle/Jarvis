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

### Daemon (auto-managed, no terminal required)

The `.app` bundles a universal `jarvis` binary at
`Jarvis.app/Contents/MacOS/jarvis` and spawns `jarvis serve` on
launch. You don't need to install anything else or open a terminal.

How it picks an instance:

1. On launch, the app probes `GET http://127.0.0.1:7777/healthz`.
2. If something already responds → attaches to that daemon (won't
   spawn a duplicate).
3. Otherwise, spawns the bundled `jarvis serve` as a child process
   with `JARVIS_DB=~/Library/Application Support/Jarvis/jarvis.db`
   and `JARVIS_LOG=info` (unless you've already set them).

The header pill in the app shows the current state — `running`
(we own it), `attached` (someone else's daemon), `failed` (with a
tooltip for diagnostics). On app quit the bundled daemon receives
SIGTERM so SQLite can flush.

Daemon stderr is forwarded to the macOS unified log:

```sh
log stream --predicate 'subsystem == "ai.jarvis.mac"' --info
```

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
| **m1** | Single window · Compose / Memory · JarvisClient (route + memory) · embedded universal `jarvis` binary · DaemonSupervisor (auto-spawn / probe-attach / SIGTERM on quit) | **this commit** |
| m2 | Three-pane NSSplitView · ActivityCard · SSE stream · Compact text composer + global hotkey ⌘⇧J | next |
| m3 | Voice 三态 · STT/TTS (SFSpeech / AVSpeech) · CommandBar · Memory browser | |
| m4 | Menubar dropdown · Preferences · Auto-update · Dark mode polish | |

See `docs/macos-desktop.md` §"开发路线" for details.

## Caveats (m1)

- Build is unsigned. Gatekeeper workaround above.
- Single window only. ⌘⇧J global hotkey arrives in m2.
- Light mode only. Dark palette tokens are defined in
  `Theme/Tokens.swift` but not yet wired through.
- New York serif headers degrade to system serif on macOS versions
  that don't ship the font (rare; New York has shipped since macOS
  Big Sur).
- The bundled daemon writes to `~/Library/Application Support/Jarvis/`.
  If you also run `jarvis serve` manually, point it at the same path
  with `JARVIS_DB=~/Library/Application\ Support/Jarvis/jarvis.db`,
  otherwise you'll have two diverging databases.
- Universal binary (arm64 + x86_64) — Intel Macs supported but
  untested in CI (macos-14 runner is arm64).
