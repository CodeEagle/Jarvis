#!/usr/bin/env bash
# Jarvis macOS app one-line installer.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/CodeEagle/Jarvis/main/scripts/install-mac-app.sh | bash
#
# Env overrides:
#   JARVIS_RELEASE_TAG=nightly      tag to download from (default: nightly)
#   JARVIS_APP_INSTALL_DIR=/path    where to place Jarvis.app
#                                   (default: /Applications, falls back
#                                    to ~/Applications if unwritable)
#   JARVIS_LAUNCH_AFTER_INSTALL=0   skip `open Jarvis.app` at the end
#   JARVIS_REPO_OWNER / _NAME       override repo coordinates
#
# Behaviour:
#   1. macOS 13+ check
#   2. Download JarvisMac.zip from the GitHub Release
#   3. Extract via `ditto -x -k` (preserves bundle metadata)
#   4. Quit any running Jarvis to avoid clobbering an in-use bundle
#   5. Move into INSTALL_DIR (sudo if needed; ~/Applications fallback)
#   6. `xattr -dr com.apple.quarantine` so Gatekeeper lets the
#      unsigned dev build run
#   7. Open the app

set -euo pipefail

REPO_OWNER="${JARVIS_REPO_OWNER:-CodeEagle}"
REPO_NAME="${JARVIS_REPO_NAME:-Jarvis}"
REPO="${REPO_OWNER}/${REPO_NAME}"
TAG="${JARVIS_RELEASE_TAG:-nightly}"
INSTALL_DIR="${JARVIS_APP_INSTALL_DIR:-/Applications}"
LAUNCH="${JARVIS_LAUNCH_AFTER_INSTALL:-1}"

c_red()    { printf "\033[31m%s\033[0m\n" "$*"; }
c_green()  { printf "\033[32m%s\033[0m\n" "$*"; }
c_yellow() { printf "\033[33m%s\033[0m\n" "$*"; }
c_dim()    { printf "\033[2m%s\033[0m\n" "$*"; }

step() { c_dim "→ $*"; }
ok()   { c_green "✓ $*"; }
warn() { c_yellow "⚠ $*"; }
die()  { c_red "✗ $*"; exit 1; }

# ── 1. Platform check ─────────────────────────────────────────────
if [[ "$(uname -s)" != "Darwin" ]]; then
    die "macOS only. For the CLI on Linux, use install.sh."
fi
MAJOR=$(sw_vers -productVersion | cut -d. -f1)
if [[ "$MAJOR" -lt 13 ]]; then
    die "macOS 13 Ventura or later required (have $(sw_vers -productVersion))."
fi
ok "macOS $(sw_vers -productVersion) · $(uname -m)"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/jarvis-install.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# ── 2. Download ──────────────────────────────────────────────────
URL="https://github.com/$REPO/releases/download/$TAG/JarvisMac.zip"
step "Downloading $URL"
if ! curl -fsSL --retry 3 -o "$TMP/JarvisMac.zip" "$URL"; then
    cat >&2 <<EOF

$(c_red "Could not download $URL")

Possible causes:
  - The '$TAG' release has not been published yet. Trigger a build
    from the Actions tab on github.com/$REPO, or push to main.
  - You're behind a network that blocks api.github.com.
  - The repo coords are wrong; override with
    JARVIS_REPO_OWNER=… JARVIS_REPO_NAME=… JARVIS_RELEASE_TAG=…

EOF
    exit 1
fi
ok "Downloaded $(du -h "$TMP/JarvisMac.zip" | awk '{print $1}')"

# ── 3. Extract ───────────────────────────────────────────────────
step "Extracting"
mkdir -p "$TMP/extract"
ditto -x -k "$TMP/JarvisMac.zip" "$TMP/extract"
APP_PATH=$(find "$TMP/extract" -maxdepth 2 -name '*.app' | head -1)
[[ -n "$APP_PATH" ]] || die "no .app bundle in archive"
APP_NAME=$(basename "$APP_PATH")
ok "Found $APP_NAME"

# ── 4. Quit running instance ─────────────────────────────────────
PROC_NAME="${APP_NAME%.app}"
if pgrep -x "$PROC_NAME" >/dev/null 2>&1; then
    warn "Jarvis is running — quitting first"
    osascript -e "tell application \"$PROC_NAME\" to quit" 2>/dev/null || true
    # Give AppleEvents a moment, then SIGTERM holdouts
    sleep 1
    pkill -x "$PROC_NAME" 2>/dev/null || true
fi

# ── 5. Install ───────────────────────────────────────────────────
# Try INSTALL_DIR; fall back to ~/Applications if unwritable and the
# user hasn't explicitly opted into sudo. Avoid surprising sudo
# prompts in piped-shell installs.
TARGET_DIR="$INSTALL_DIR"
if [[ ! -w "$TARGET_DIR" ]]; then
    if [[ "$INSTALL_DIR" == "/Applications" ]]; then
        FALLBACK="$HOME/Applications"
        warn "/Applications not writable; falling back to $FALLBACK"
        mkdir -p "$FALLBACK"
        TARGET_DIR="$FALLBACK"
    else
        die "Install dir $TARGET_DIR is not writable."
    fi
fi
step "Installing to $TARGET_DIR/$APP_NAME"
rm -rf "$TARGET_DIR/$APP_NAME"
mv "$APP_PATH" "$TARGET_DIR/"
ok "Installed"

# ── 6. Clear quarantine ──────────────────────────────────────────
step "Removing Gatekeeper quarantine"
xattr -dr com.apple.quarantine "$TARGET_DIR/$APP_NAME" 2>/dev/null || \
    warn "xattr -dr failed (no extended attrs?) — opening anyway"

# ── 7. Launch ────────────────────────────────────────────────────
echo ""
ok "Done. Try it:"
cat <<EOF
    open '$TARGET_DIR/$APP_NAME'

  Daemon stderr (subprocess that the .app spawns) goes to the macOS
  unified log:
    log stream --predicate 'subsystem == "ai.jarvis.mac"' --info

  Where data lives:
    ~/Library/Application Support/Jarvis/jarvis.db

EOF
if [[ "$LAUNCH" == "1" ]]; then
    open "$TARGET_DIR/$APP_NAME"
fi
