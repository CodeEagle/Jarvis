# Jarvis macOS App — m1 (Swift / SwiftUI)

The lightweight desktop client for the Jarvis daemon. m1 covers a
single window with two tabs (Compose / Memory) calling the local
`jarvis-api` HTTP server. Voice mode, collaboration panel, command
bar, and Memory browser ship in m2–m4. Architectural plan lives in
[`docs/macos-desktop.md`](../../docs/macos-desktop.md); the visual
system in [`docs/design/macos-visual.md`](../../docs/design/macos-visual.md).

## One-liner install (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/CodeEagle/Jarvis/main/scripts/install-mac-app.sh | bash
```

Pulls the latest `nightly` build, drops `Jarvis.app` into
`/Applications` (or `~/Applications` if the former isn't writable),
strips the Gatekeeper quarantine, and launches it. Env overrides:

| var | default | meaning |
|---|---|---|
| `JARVIS_RELEASE_TAG` | `nightly` | release tag to pull from |
| `JARVIS_APP_INSTALL_DIR` | `/Applications` | install location |
| `JARVIS_LAUNCH_AFTER_INSTALL` | `1` | set to `0` to skip auto-open |

## Manual install (if you'd rather inspect first)

The app is built unsigned by the `macos-app` GitHub Actions workflow
on every push. Two ways to grab the .app:

**Stable URL** — every successful main build also publishes to a
rolling `nightly` GitHub Release:
<https://github.com/CodeEagle/Jarvis/releases/tag/nightly>

**Per-commit artifact** — for a specific feature-branch build,
**Actions** tab → run → Artifacts → `JarvisMac-<sha>.zip` (requires
GitHub auth).

### Bypass Gatekeeper (one-time)

Because the build isn't code-signed, macOS will refuse to open the
app on first launch ("Jarvis.app is damaged and can't be opened").
Strip the quarantine attribute once:

```sh
xattr -dr com.apple.quarantine /Applications/Jarvis.app
open /Applications/Jarvis.app
```

(The one-liner installer does this for you.)

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

- **Voice tab** (default) — press-and-hold the mic to speak; release
  sends the transcript to `POST /router/input` and renders the
  `RouteDecision` card. Falls back to a text input row at the bottom
  when mic / speech permissions aren't granted. STT-only — TTS
  arrives in m3 with auto-VAD and barge-in.
- **Memory tab** — lists memories from `GET /memory/global`; pull
  `Refresh` to reload.
- **Settings tab** — pick a default model from presets or type any
  `provider/model` id; paste your `ANTHROPIC_API_KEY` /
  `OPENAI_API_KEY` etc. without opening a terminal. Saving restarts
  the embedded daemon so the new env reaches it.

### Permissions

On first launch the Voice tab asks for **Microphone** and **Speech
Recognition** access. Both can be revisited in
*System Settings → Privacy & Security*. STT runs on-device — no audio
leaves the machine.

### Where data lives

| | path |
|---|---|
| SQLite | `~/Library/Application Support/Jarvis/jarvis.db` |
| Provider config | `~/.jarvis/config.toml` (managed by Settings) |
| Provider OAuth tokens | `~/.jarvis/auth/<provider>.json` (`jarvis model login`) |
| API keys | `~/Library/Application Support/Jarvis/secrets.json` (mode 0600) |

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
