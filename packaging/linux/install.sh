#!/usr/bin/env bash
# Install GitCraft from this release folder for the current user (no root needed).
#
#   ./install.sh              install into ~/.local
#   ./install.sh --uninstall  remove it again
#
# Installs ~/.local/bin/gitcraft and gitcraft-cli, the icon and a desktop entry.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$BIN_DIR/gitcraft" "$BIN_DIR/gitcraft-cli" "$APP_DIR/gitcraft.desktop" "$ICON_DIR/gitcraft.png"
    echo "GitCraft removed."
    exit 0
fi

command -v git >/dev/null || echo "warning: git is not installed; GitCraft needs it" >&2
install -Dm755 "$HERE/gitcraft" "$BIN_DIR/gitcraft"
install -Dm755 "$HERE/gitcraft-cli" "$BIN_DIR/gitcraft-cli"
install -Dm644 "$HERE/gitcraft.png" "$ICON_DIR/gitcraft.png"
mkdir -p "$APP_DIR"
sed "s|@BIN@|$BIN_DIR/gitcraft|" "$HERE/gitcraft.desktop" > "$APP_DIR/gitcraft.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
echo "GitCraft installed: $BIN_DIR/gitcraft"
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) echo "note: add $BIN_DIR to your PATH to run gitcraft from a terminal" ;; esac
