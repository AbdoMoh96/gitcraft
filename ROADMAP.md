# GitCraft roadmap

**Stage:** M1 complete — a working Git client for the daily loop.

| Number | Value |
|---|---|
| Feature parity | 86 / 123 (69.9%) |
| Commands | 79 engine + 12 UI |
| Tests | 58 (`cargo xtask stats`) |
| Lines of Rust | ~10,700 |

Milestones and the current focus: [`docs/roadmap.md`](docs/roadmap.md). Ranked work:
[`docs/gaps.md`](docs/gaps.md).

## Progress log
- **2026-10-17 — M1.** Workspace in the WordCraft layout (control-key, git, graph, engine, mcp,
  ui-egui, apps, xtask). Clean-room inventory of the target (`docs/clean-room.md`). git layer with
  parsers for status v2, log, refs, diffs, blame, stashes, remotes and hunk/line patches; operations
  for staging, commits, branches, merges, rebases, cherry-picks, reverts, resets, tags, remotes,
  stashes and conflicts. Graph lane layout. Engine with 79 commands, undo/redo, feature catalog,
  hostile-params test. egui window: toolbar, refs panel, virtualised graph, commit panel, diff view
  (hunk / inline / split, line staging, blame), dialogs, command palette, light/dark themes,
  background network jobs, auto-reload. CLI with a text graph, MCP server, loopback control channel
  with key file. `cargo xtask ci` green.
