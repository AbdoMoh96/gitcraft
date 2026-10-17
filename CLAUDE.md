# GitCraft — instructions for agents

GitCraft is a clean-room, open-source, Rust-native Git client targeting parity with the leading graphical Git client, and going further on speed, openness and agent control. It runs natively on Linux, macOS and Windows.

## Start every session here
1. Read `ROADMAP.md` (stage, progress log), then `docs/gaps.md` (ranked work list) and `docs/roadmap.md` (Current focus). `plan/` is gitignored for local notes.
2. Work autonomously. Stop to ask only when a decision is genuinely the owner's (licensing, publishing, pushing to new remotes, secrets).

## Never crash
A crash in a Git client can leave a repository mid-operation. **This outranks feature work.**
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`).
- **Errors are `Result<T, E>`** (`GitError` in `git`, `CmdError` in `engine`) and `?`. An unfinished feature returns an error or is disabled.
- **Input is hostile** (command params, git output, file names): `get()` instead of `[i]`, slice strings only at char boundaries, checked/saturating arithmetic, cap sizes. Git output parsers must never panic on junk (each parser has a `junk_never_panics` test).
- **Last-resort guard:** `Session::run` turns an escaped panic into an error and reloads. It's a safety net, not a licence.
- **Prove it:** `crates/engine/src/tests.rs::hostile_params_never_panic` runs every command with junk params in a real repository — keep it green; every crash fix lands with a regression test.
- Production crate roots carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`; new crates start with it. Tests may `unwrap` (`clippy.toml`).

## Git safety
- Every argument from a user or agent goes through `check_name` / `check_path` / `check_oid` (`crates/git/src/lib.rs`): no leading `-` (option injection), no NUL or line breaks. Paths go after `--` and as literal pathspecs (`Repo::run_paths`).
- Git never prompts: `GIT_TERMINAL_PROMPT=0`, editor `:`, no pager (`runner.rs`). Don't add a command that can block on a terminal.
- Destructive UI actions (discard, hard reset, force push, delete branch on a remote) ask first in the UI; programmatic calls never open dialogs.
- Undoable commands record an `UndoStep` (`crates/engine/src/cmd/edit.rs`) that checks the repository is still where it left it.

## Assets: original only
- Icons are drawn in code (`crates/ui-egui/src/icons.rs`). Colours come from `theme::Tokens`; the lane palette is our own.
- Never copy, trace, screenshot-crop or recreate icons, images, themes, sounds or any other asset from another Git client. Not even "temporarily".
- Every asset file needs a row in `ATTRIBUTION.md` (author, source, licence); `cargo xtask assets` fails otherwise.

## Clean room
- The incumbent may be **observed** black-box only: its public documentation, feature names, and running it with synthetic repositories. Screenshots of it stay under `plan/` (gitignored) and are never committed.
- Never unpack, read, decompile or copy anything inside its bundle (package metadata and file listings only — see `docs/clean-room.md`). Never copy its wording beyond feature names.
- Git behaviour comes from git itself and its documentation. Never copy GPL code (git's own sources are GPL-2.0: read the docs, not the code).

## Architecture (see `docs/architecture.md`)
| Layer | Crate | Job |
|---|---|---|
| L0 | `control-key` | control key file location |
| L1 | `git` | git CLI runner, porcelain parsers, patches, operations |
| L2 | `graph` | commit graph lane layout |
| L4 | `engine` | Session, undo, **command registry**, feature catalog |
| L5 | `mcp` | MCP server (headless or bridged to the app) |
| L6 | `ui-egui` | egui front end (swappable) |
| apps | `apps/gitcraft`, `apps/gitcraft-cli` | binaries |

- **Everything is a command.** User-visible behaviour = a `CommandSpec` in `crates/engine/src/cmd/*.rs` (id, label, UI location, shortcut, params doc, `enabled`, `run`, `slow` for network) + tests. UI-only commands (`ui.*`) live in `crates/ui-egui/src/lib.rs` (`GitApp::ui_command`).
- **Parity is measured:** `crates/engine/src/catalog.rs` lists the incumbent's features (names only) with command ids. `cargo xtask parity` writes `docs/parity-checklist.md`; `catalog::tests::parity_floor` only rises.
- **Layering** is enforced by `cargo xtask layers`. Nothing below L6 depends on egui/eframe/winit/rfd.
- **Rust only.** No wasm target: GitCraft runs the git program.

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers). Commit after every arc of work that builds, with a task id in the message (`M2.3: interactive rebase`).

## Running and looking at the app
- `cargo run --release -p gitcraft -- --sample --control 7990` (sample repository + control channel).
- Drive it with JSON lines on `127.0.0.1:7990`, each with the key from `~/.config/gitcraft/control-key.local-7990` (macOS `~/Library/Application Support/GitCraft/`): `{"id":1,"key":"…","method":"engine.execute","params":{"command":"stage.all"}}`, `{"id":2,"key":"…","method":"ui.screenshot"}`. See `docs/control-protocol.md`.
- **Headless screenshots** (no window): `cargo run -p gitcraft-ui-egui --example ui_shot -- out.png [--dark] [--size WxH] --cmd 'id={json}'`.
- **For UI work, look at the result** (screenshot, read the PNG).
- CLI: `gitcraft-cli status|log|run|commands|parity|mcp`. Use `GITCRAFT_PROFILE_DIR` for isolated runs.

## Roadmap
`ROADMAP.md` (stage, numbers, progress log), `docs/target-app-parity.md` (assessment), `docs/gaps.md` (ranked work list), `docs/roadmap.md` (milestones, Current focus), `docs/parity-checklist.md` (generated). When work lands, update the gap, the parity doc and the progress log.
