#!/usr/bin/env bash
# Build GitCraft from source and install it for the current user (no sudo).
#
#   ./install.sh            build from $GITCRAFT_SRC (default: the checkout this script is in, else clone from GitHub)
#
# Installs:
#   ~/.local/bin/gitcraft, ~/.local/bin/gitcraft-cli
#   ~/.local/share/applications/gitcraft.desktop
#   ~/.local/share/icons/hicolor/256x256/apps/gitcraft.png
set -euo pipefail

REPO_URL="git@github.com:AbdoMoh96/gitcraft.git"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="${GITCRAFT_SRC:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || die "cargo not found; install Rust (rustup) first"
command -v git >/dev/null || die "git not found"

if [[ ! -f "$SRC/Cargo.toml" ]]; then
    SRC="${XDG_CACHE_HOME:-$HOME/.cache}/gitcraft-src"
    if [[ -d "$SRC/.git" ]]; then
        say "Updating source in $SRC"
        git -C "$SRC" pull --ff-only
    else
        say "Cloning $REPO_URL into $SRC"
        git clone "$REPO_URL" "$SRC"
    fi
fi

say "Building release binaries from $SRC"
cargo build --release --manifest-path "$SRC/Cargo.toml" -p gitcraft -p gitcraft-cli

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --manifest-path "$SRC/Cargo.toml" \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
TARGET_DIR="${TARGET_DIR:-$SRC/target}"

say "Installing binaries to $BIN_DIR"
install -Dm755 "$TARGET_DIR/release/gitcraft" "$BIN_DIR/gitcraft"
install -Dm755 "$TARGET_DIR/release/gitcraft-cli" "$BIN_DIR/gitcraft-cli"

say "Installing icon and desktop entry"
install -Dm644 "$SRC/assets/gitcraft.png" "$ICON_DIR/gitcraft.png"
mkdir -p "$APP_DIR"
cat > "$APP_DIR/gitcraft.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=GitCraft
GenericName=Git Client
Comment=Fast, open-source Git client
Exec=$BIN_DIR/gitcraft %F
Icon=gitcraft
Terminal=false
Categories=Development;RevisionControl;
StartupWMClass=gitcraft
DESKTOP

command -v update-desktop-database >/dev/null && update-desktop-database -q "$APP_DIR" || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) printf 'note: %s is not on your PATH\n' "$BIN_DIR" ;;
esac

say "Installed $("$BIN_DIR/gitcraft" --version)"
