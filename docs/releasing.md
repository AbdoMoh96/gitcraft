# Releasing

Releases are built and published by `.github/workflows/release.yml` when a version tag is pushed.

## Steps

1. Set the new version in `Cargo.toml` (`[workspace.package] version = "0.2.0"`) and run
   `cargo xtask ci` (it also refreshes `Cargo.lock`).
2. Commit: `git commit -am "Release 0.2.0"` and push.
3. Tag and push the tag — the tag must be `v` + the version in `Cargo.toml`:

   ```bash
   git tag -a v0.2.0 -m "GitCraft 0.2.0"
   git push origin v0.2.0
   ```

4. The workflow checks the tag against `Cargo.toml`, runs `cargo xtask ci`, builds every platform,
   and creates the GitHub Release with generated notes. A version with a suffix (`0.2.0-beta.1`)
   is published as a pre-release.

To try the builds without publishing, run the workflow by hand (Actions › Release › Run workflow):
the packages are attached to the run as artifacts.

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
