<p align="center">
  <img alt="GitCraft icon" src="assets/gitcraft.png" width="128" height="128">
</p>

<h1 align="center">GitCraft</h1>

<p align="center">
  <b>A fast, open-source Git client in pure Rust: a clean-room take on the graphical Git client workflow.</b>
</p>

<p align="center">
  Commit graph, branches and remotes, staging by file, hunk or line, diffs (hunk, inline, split), blame,
  file history, merge / rebase / cherry-pick / revert with conflict handling, stashes, tags, undo and redo.
  Every action is a command you can also run from the CLI or hand to an agent over MCP.
</p>

<p align="center">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-7048e8?style=flat-square&logo=rust&logoColor=white">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-7048e8?style=flat-square">
  <img alt="Agent-drivable over MCP" src="https://img.shields.io/badge/agents-MCP%20%C2%B7%20CLI-5937c2?style=flat-square">
</p>

<p align="center">
  <a href="#what-works-today">What works today</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#for-agents-cli-and-mcp">For agents</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#clean-room">Clean room</a> ·
  <a href="#roadmap">Roadmap</a> ·
  <a href="#license">License</a>
</p>

## What works today

| Area | Features |
|---|---|
| **Repositories** | Open (folder, recent list, drag and drop), clone, init, close, reopen the last one, reload on outside changes |
| **Graph** | Lanes with curved merges, branch / remote / tag labels with the lane colour, author avatars, relative dates, WIP row, search with dimming, keyboard navigation, load more, file history mode |
| **Left panel** | Local branches with ahead/behind, remotes and their branches, stashes, tags, filter, context menus |
| **Commit panel** | Unstaged and staged files, stage / unstage / discard per file or all, conflict banner with Continue / Skip / Abort, summary with 72-character countdown, description, amend, commit |
| **Diff view** | Hunk, inline and split modes; stage, unstage or discard a hunk or picked lines; blame; file history |
| **Branches** | Create (anywhere), checkout (local or remote with tracking), rename, delete, set upstream, delete on remote, merge, fast-forward, rebase |
| **Commits** | Checkout (detached), cherry-pick, revert, reset soft / mixed / hard, tag (lightweight or annotated), copy SHA and message |
| **Remotes** | Fetch (all or one, prune), pull (merge, fast-forward only, rebase), push (sets upstream), force push with lease, add / remove remotes, push tags. Network commands run in the background |
| **Undo / redo** | Commit, amend, checkout, branch create / delete / rename, tag create / delete, reset |
| **Everywhere** | Command palette (Ctrl+P / ⌘P), keyboard shortcuts, light / dark / system theme, preferences |

Parity is measured against the incumbent's feature list: **86 of 123 features** today
([`docs/parity-checklist.md`](docs/parity-checklist.md)).

## Quick start

Needs Rust 1.95+ and `git` on `PATH` (or `GITCRAFT_GIT=/path/to/git`).

```sh
cargo run --release -p gitcraft                    # the app (opens the last repository)
cargo run --release -p gitcraft -- ~/src/project   # a repository
cargo run --release -p gitcraft -- --sample        # a throwaway sample repository
```

Build and install from source for the current user (Arch and other Linux desktops; no sudo):
`install/arch/install.sh` puts `gitcraft` and `gitcraft-cli` in `~/.local/bin` with a desktop entry
and icon; `install/arch/uninstall.sh [--purge]` removes them.

## For agents: CLI and MCP

```sh
gitcraft-cli status --repo .                      # branch, files, branches, tags, stashes (JSON)
gitcraft-cli log --repo . --limit 30              # the commit graph as text
gitcraft-cli run --repo . --cmd 'branch.create={"name":"topic"}' --cmd stage.all --cmd 'commit.create={"summary":"Work"}'
gitcraft-cli commands                             # all 79 commands with labels, locations and shortcuts
gitcraft-cli parity                               # feature parity
gitcraft-cli mcp --repo .                         # MCP server on stdio (headless)
gitcraft-cli mcp --connect 127.0.0.1:7990         # MCP bridged to a running app: gitcraft --control 7990
```

The MCP server offers `status`, `log`, `show_commit`, `diff`, `stage`, `unstage`, `commit`, `checkout`,
`create_branch`, `fetch`, `pull`, `push`, `execute` (any command), `batch`, `list_commands`, `parity` and,
with the app, `screenshot`. See [`docs/mcp.md`](docs/mcp.md) and [`docs/control-protocol.md`](docs/control-protocol.md).

## Architecture

| Layer | Crate | Job |
|---|---|---|
| L0 | `control-key` | where the control channel's key file lives |
| L1 | `git` | runs the `git` command line; parses status, log, refs, diffs, blame, stashes, remotes; builds hunk and line patches |
| L2 | `graph` | commit graph lane layout |
| L4 | `engine` | `Session`, selection, undo/redo, the **command registry**, the feature catalog |
| L5 | `mcp` | MCP server, headless or bridged to the app |
| L6 | `ui-egui` | the egui window (swappable) |
| apps | `apps/gitcraft`, `apps/gitcraft-cli` | binaries |

Everything is a command (`crates/engine/src/cmd/*.rs`): the toolbar, menus, shortcuts, palette, CLI,
control channel and MCP all dispatch by id. Details: [`docs/architecture.md`](docs/architecture.md).

## Clean room

GitCraft is an original implementation. The incumbent was studied only as a black box: its package
metadata and file listing, its public documentation and feature names. No part of its bundle was
unpacked, read, decompiled or copied, and none of its code, wording, icons, images or other assets are
used. Git behaviour comes from git itself and its public documentation. The method and what was
looked at are recorded in [`docs/clean-room.md`](docs/clean-room.md).

## Roadmap

[`ROADMAP.md`](ROADMAP.md) has the stage and progress log; [`docs/gaps.md`](docs/gaps.md) the ranked
work list; [`docs/roadmap.md`](docs/roadmap.md) the milestones.

## Development

```sh
cargo xtask ci        # fmt, clippy -D warnings, tests, assets, layers
cargo xtask parity    # regenerate docs/parity-checklist.md
cargo run -p gitcraft-ui-egui --example ui_shot -- shot.png --cmd 'graph.select={"oid":"wip"}'   # headless screenshot
```

## License

MIT OR Apache-2.0, at your option. GitCraft is not affiliated with or endorsed by the makers of any
other Git client; see [`NOTICE`](NOTICE).

## Releases

Pre-built packages for Linux, macOS and Windows are on the GitHub Releases page. Pushing to `main`
with a new version in `Cargo.toml` builds and publishes them: see [docs/releasing.md](docs/releasing.md).
