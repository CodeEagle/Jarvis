#!/usr/bin/env bash
# Jarvis one-line installer.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/CodeEagle/Jarvis/main/scripts/install.sh | bash
#
# Or with a non-default install dir:
#   curl -fsSL ... | JARVIS_INSTALL_DIR=$HOME/bin bash
#
# Behaviour:
#   1. Detect OS + arch (linux/x86_64, macos/arm64, macos/x86_64).
#   2. Query GitHub for the latest release tag.
#   3. Download the matching tarball from that release.
#   4. If no matching release exists (yet), fall back to
#      `git clone && cargo build --release` if Rust is installed,
#      otherwise instruct the user how to install Rust.
#   5. Install to $JARVIS_INSTALL_DIR (default: ~/.local/bin).
#   6. Print PATH guidance + a quick smoke test.

set -euo pipefail

REPO_OWNER="${JARVIS_REPO_OWNER:-CodeEagle}"
REPO_NAME="${JARVIS_REPO_NAME:-Jarvis}"
REPO="${REPO_OWNER}/${REPO_NAME}"
INSTALL_DIR="${JARVIS_INSTALL_DIR:-$HOME/.local/bin}"

c_red()    { printf "\033[31m%s\033[0m\n" "$*"; }
c_green()  { printf "\033[32m%s\033[0m\n" "$*"; }
c_yellow() { printf "\033[33m%s\033[0m\n" "$*"; }
c_dim()    { printf "\033[2m%s\033[0m\n" "$*"; }

step() { c_dim "→ $*"; }
ok()   { c_green "✓ $*"; }
warn() { c_yellow "⚠ $*"; }
die()  { c_red "✗ $*"; exit 1; }

# ── 1. Detect platform ─────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
    Linux-x86_64)   TARGET="x86_64-linux";  PRETTY="Linux x86_64" ;;
    Darwin-arm64)   TARGET="aarch64-macos"; PRETTY="macOS Apple Silicon" ;;
    Darwin-x86_64)  TARGET="x86_64-macos";  PRETTY="macOS Intel" ;;
    *) die "Unsupported platform: $OS-$ARCH (supported: Linux x86_64, macOS arm64/x86_64)" ;;
esac
ok "Detected $PRETTY"

mkdir -p "$INSTALL_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ── 2. Discover latest release tag ──────────────────────────────────
step "Looking up latest release on github.com/$REPO ..."
LATEST_TAG=""
if API_RESP="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null)"; then
    LATEST_TAG="$(printf '%s' "$API_RESP" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
fi

# ── 3. Try to download the prebuilt binary ──────────────────────────
INSTALLED_FROM=""
if [[ -n "$LATEST_TAG" ]]; then
    ARCHIVE="jarvis-${LATEST_TAG}-${TARGET}.tar.gz"
    URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$ARCHIVE"
    SHA_URL="${URL}.sha256"
    step "Trying release $LATEST_TAG ($ARCHIVE) ..."
    if curl -fsSL -o "$TMP/$ARCHIVE" "$URL" 2>/dev/null; then
        # SHA verification (best-effort; skip silently if absent).
        if curl -fsSL -o "$TMP/$ARCHIVE.sha256" "$SHA_URL" 2>/dev/null; then
            EXPECTED="$(awk '{print $1}' "$TMP/$ARCHIVE.sha256")"
            ACTUAL="$(shasum -a 256 "$TMP/$ARCHIVE" | awk '{print $1}')"
            if [[ "$EXPECTED" != "$ACTUAL" ]]; then
                die "SHA256 mismatch! expected $EXPECTED got $ACTUAL"
            fi
            ok "SHA256 verified"
        else
            warn "no .sha256 sidecar found; skipping integrity check"
        fi
        tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
        if [[ ! -f "$TMP/jarvis" ]]; then
            die "downloaded archive does not contain a 'jarvis' binary"
        fi
        install -m 755 "$TMP/jarvis" "$INSTALL_DIR/jarvis"
        INSTALLED_FROM="release $LATEST_TAG"
    else
        warn "no prebuilt for $TARGET in $LATEST_TAG"
    fi
else
    warn "no published releases found"
fi

# ── 4. Fallback: build from source ──────────────────────────────────
if [[ -z "$INSTALLED_FROM" ]]; then
    step "Falling back to build-from-source (HEAD on main)"
    if ! command -v cargo >/dev/null 2>&1; then
        cat >&2 <<EOF

$(c_red "Cannot build: Rust toolchain not found.")

Install Rust first:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Then re-run this installer.
EOF
        exit 1
    fi
    if ! command -v git >/dev/null 2>&1; then
        die "git not found; please install git and re-run"
    fi
    SRC="$TMP/src"
    git clone --depth 1 "https://github.com/$REPO.git" "$SRC" 2>&1 | sed 's/^/  /'
    (
        cd "$SRC"
        cargo build --release --locked -p jarvis-cli 2>&1 | tail -3 | sed 's/^/  /'
    )
    install -m 755 "$SRC/target/release/jarvis" "$INSTALL_DIR/jarvis"
    INSTALLED_FROM="source (HEAD)"
fi
ok "Installed jarvis from $INSTALLED_FROM → $INSTALL_DIR/jarvis"

# ── 5. PATH check ──────────────────────────────────────────────────
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo ""
        warn "$INSTALL_DIR is not in your PATH. Add to your shell rc:"
        if [[ -n "${ZSH_VERSION:-}" ]] || [[ "$SHELL" == */zsh ]]; then
            echo "    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc"
        else
            echo "    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
        fi
        ;;
esac

# ── 6. Quick smoke ─────────────────────────────────────────────────
echo ""
step "Smoke check"
"$INSTALL_DIR/jarvis" --help | head -3
echo ""
ok "Ready. Try:"
cat <<'EOF'
    jarvis route "openwrt 编译报错"
    jarvis sessions new "我的项目" coding
    jarvis memory write "用户偏好简洁回答"
    jarvis memory search 偏好 --json
    jarvis dashboard --json

  Codex LLM judge (optional):
    npm i -g @openai/codex && codex login
    JARVIS_JUDGE=codex jarvis judge probe
    JARVIS_JUDGE=codex jarvis route "openwrt 编译报错"

  Full feature acceptance (60+ checks):
    bash -c "$(curl -fsSL https://raw.githubusercontent.com/CodeEagle/Jarvis/main/scripts/smoke.sh)"
EOF
