#!/usr/bin/env bash
# Package release binaries built with `cargo build --release --target <target>` into dist/.
#
#   packaging/package.sh <target> <version> [dist-dir]
#
#   Linux   → gitcraft-<version>-linux-<arch>.tar.gz  (binaries, icon, desktop entry, install.sh)
#   macOS   → GitCraft-<version>-macos-<arch>.zip     (GitCraft.app, gitcraft-cli)
#   Windows → gitcraft-<version>-windows-<arch>.zip   (gitcraft.exe, gitcraft-cli.exe)
#
# Every package carries the licences, NOTICE and README.
set -euo pipefail

TARGET="${1:?usage: package.sh <target> <version> [dist-dir]}"
VERSION="${2:?usage: package.sh <target> <version> [dist-dir]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${3:-$ROOT/dist}"
BIN="$ROOT/target/$TARGET/release"
mkdir -p "$DIST"
DIST="$(cd "$DIST" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

case "$TARGET" in
    x86_64-*) ARCH=x86_64 ;;
    aarch64-*) ARCH=arm64 ;;
    *) ARCH="${TARGET%%-*}" ;;
esac

docs() {
    cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/NOTICE" "$ROOT/README.md" "$1/"
}

case "$TARGET" in
    *-linux-*)
        NAME="gitcraft-$VERSION-linux-$ARCH"
        D="$WORK/$NAME"
        mkdir -p "$D"
        install -m755 "$BIN/gitcraft" "$BIN/gitcraft-cli" "$D/"
        install -m644 "$ROOT/assets/gitcraft.png" "$ROOT/packaging/linux/gitcraft.desktop" "$D/"
        install -m755 "$ROOT/packaging/linux/install.sh" "$D/"
        docs "$D"
        tar -C "$WORK" -czf "$DIST/$NAME.tar.gz" "$NAME"
        echo "$DIST/$NAME.tar.gz"
        ;;
    *-apple-darwin)
        NAME="GitCraft-$VERSION-macos-$ARCH"
        D="$WORK/$NAME"
        APP="$D/GitCraft.app"
        mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
        install -m755 "$BIN/gitcraft" "$APP/Contents/MacOS/gitcraft"
        install -m755 "$BIN/gitcraft-cli" "$D/gitcraft-cli"
        sed "s|@VERSION@|$VERSION|g" "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"
        # The app icon, made from the 256 px PNG.
        SET="$WORK/gitcraft.iconset"
        mkdir -p "$SET"
        for s in 16 32 128 256; do
            sips -z "$s" "$s" "$ROOT/assets/gitcraft.png" --out "$SET/icon_${s}x${s}.png" >/dev/null
            d=$((s * 2))
            if [[ $d -le 256 ]]; then
                sips -z "$d" "$d" "$ROOT/assets/gitcraft.png" --out "$SET/icon_${s}x${s}@2x.png" >/dev/null
            fi
        done
        iconutil -c icns "$SET" -o "$APP/Contents/Resources/gitcraft.icns"
        # Ad-hoc signature: not notarised, but a consistent bundle Gatekeeper can be told to open.
        codesign --force --deep --sign - "$APP"
        docs "$D"
        (cd "$WORK" && ditto -c -k --keepParent "$NAME" "$DIST/$NAME.zip")
        echo "$DIST/$NAME.zip"
        ;;
    *-windows-*)
        NAME="gitcraft-$VERSION-windows-$ARCH"
        D="$WORK/$NAME"
        mkdir -p "$D"
        cp "$BIN/gitcraft.exe" "$BIN/gitcraft-cli.exe" "$D/"
        docs "$D"
        (cd "$WORK" && 7z a -tzip -bso0 "$DIST/$NAME.zip" "$NAME")
        echo "$DIST/$NAME.zip"
        ;;
    *)
        echo "unsupported target: $TARGET" >&2
        exit 1
        ;;
esac
