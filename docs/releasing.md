# Releasing

Releases are built and published by `.github/workflows/release.yml` when a version tag is pushed.

## Steps

Every push to `main` runs the workflow. It reads the version from `Cargo.toml` and publishes
release `v<version>` only when that release doesn't exist yet; otherwise it stops after a quick
check (nothing is built).

1. Set the new version in `Cargo.toml` (`[workspace.package] version = "0.2.0"`) and run
   `cargo xtask ci` (it also refreshes `Cargo.lock`).
2. Commit and push to `main`.
3. The workflow runs `cargo xtask ci`, builds every platform, creates tag `v0.2.0` on that commit
   and publishes the GitHub Release with generated notes. A version with a suffix (`0.2.0-beta.1`)
   is published as a pre-release.

To try the builds without publishing, run the workflow by hand (Actions › Release › Run workflow):
the packages are attached to the run as artifacts.

The CI workflow (`ci.yml`) is paused: it runs only when started by hand (Actions › CI › Run
workflow). Its `on:` block says how to turn it back on.

## What is published

| Package | Contents |
|---|---|
| `gitcraft-<v>-linux-x86_64.tar.gz` | `gitcraft`, `gitcraft-cli`, icon, desktop entry, `install.sh` (installs to `~/.local`; `--uninstall` removes) |
| `GitCraft-<v>-macos-arm64.zip` | `GitCraft.app` (Apple Silicon) and `gitcraft-cli` |
| `GitCraft-<v>-macos-x86_64.zip` | `GitCraft.app` (Intel) and `gitcraft-cli` |
| `gitcraft-<v>-windows-x86_64.zip` | `gitcraft.exe`, `gitcraft-cli.exe` |
| `SHA256SUMS.txt` | checksums of the packages |

Every package carries `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE` and `README.md`. Packaging is
`packaging/package.sh <target> <version> [dist]`; it can be run locally after
`cargo build --release --target <target> -p gitcraft -p gitcraft-cli`.

## Not done yet

- **Signing.** macOS builds are ad-hoc signed, not notarised: the first launch needs right-click ›
  Open (or `xattr -dr com.apple.quarantine GitCraft.app`). Windows builds are unsigned, so
  SmartScreen warns. Both need certificates from the owner (Apple Developer ID, a code-signing
  certificate) stored as repository secrets.
- **Windows executable icon** (needs a resource compiled into the `.exe`).
- **Linux formats** beyond the tarball (AppImage, `.deb`, AUR).
