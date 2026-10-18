#!/usr/bin/env bash
# Remove a GitCraft install made by install.sh.
#
#   ./uninstall.sh          remove binaries, desktop entry and icon (keeps settings)
#   ./uninstall.sh --purge  also remove ~/.config/gitcraft and the cloned source cache
set -euo pipefail

BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
PURGE=0
[[ "${1:-}" == "--purge" ]] && PURGE=1

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }

remove() {
    for f in "$@"; do
        if [[ -e "$f" ]]; then
            rm -rf -- "$f"
            echo "removed $f"
        fi
    done
}

say "Removing GitCraft"
remove "$BIN_DIR/gitcraft" "$BIN_DIR/gitcraft-cli" "$APP_DIR/gitcraft.desktop" "$ICON_DIR/gitcraft.png"

if (( PURGE )); then
    say "Purging settings and source cache"
    remove "${XDG_CONFIG_HOME:-$HOME/.config}/gitcraft" "${XDG_CACHE_HOME:-$HOME/.cache}/gitcraft-src"
fi

command -v update-desktop-database >/dev/null && update-desktop-database -q "$APP_DIR" || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true

say "Done"
