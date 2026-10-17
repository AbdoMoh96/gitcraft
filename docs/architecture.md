# Architecture

Status: M1 · Revision 1 (2026-10-17)

## Layers

```
L6  ui-egui        the window: toolbar, refs panel, graph, commit panel, diff view, dialogs, palette
L5  mcp            MCP server: tools over a Backend (headless Session, or the app's control channel)
L4  engine         Session, selection, undo/redo, command registry, feature catalog
L2  graph          lane layout for the commit graph
L1  git            git CLI runner; parsers for status, log, refs, diff, blame, stash, remotes; patches; operations
L0  control-key    where the control key file lives
apps               gitcraft (eframe window + loopback control server), gitcraft-cli
```

A crate depends only on lower layers; only L6 may use a UI toolkit. `cargo xtask layers` enforces it.

## git (L1)

`Git` is the git program plus environment; `Repo` is an open work tree. Every read is a porcelain
command with a machine format, parsed here:

| Data | Command | Parser |
|---|---|---|
| status | `status --porcelain=v2 --branch -z --untracked-files=all` | `status::parse` |
| history | `log -z --date-order --format=<fields separated by 0x1f>` | `log::parse` |
| refs | `for-each-ref --format=<fields separated by 0x1f>` | `refs::parse` |
| diffs | `diff` / `diff --cached` / `diff --no-index` / `diff-tree -p` | `diff::parse` (files, hunks, numbered lines) |
| a commit's files | `diff-tree -r -z -M --name-status` | `diff::parse_name_status` |
| blame | `blame --porcelain` | `blame::parse` |
| stashes, remotes | `stash list -z`, `remote -v` | `stash::parse`, `remote::parse` |

Git runs with `LC_ALL=C`, no prompts (`GIT_TERMINAL_PROMPT=0`), no editor (`:`), no pager, no colour
and no optional locks. Paths from users are literal pathspecs (`Repo::run_paths`). Names are checked
(`check_name`: no leading `-`, no NUL or line breaks).

**Partial staging** (`patch.rs`): a hunk, or chosen lines of it, becomes a patch built from the parsed
diff; unchosen additions are dropped and unchosen deletions kept as context (the reverse when the patch
is applied with `--reverse`), and `git apply --cached --recount` applies it. Discarding is the same
patch applied in reverse to the work tree.

**Conflicts:** operations that can stop (merge, rebase, cherry-pick, revert, pull, stash apply/pop)
return an `Outcome` with `conflicts: true` instead of an error when files are left in conflict.

## graph (L2)

Commits arrive newest first in date order (children before parents). Each lane remembers the commit it
waits for. A commit takes the leftmost lane waiting for it (others waiting for it merge in), its first
parent continues the lane unless a lane to the left already waits for it, and further parents reuse a
waiting lane or open a free one. Lanes never move sideways, so a row is drawn as two halves of straight
or curved segments (`Row::up`, `Row::down`).

## engine (L4)

`Session` holds the open `Repo`, everything loaded from it (`RepoData`: status, refs, commits, graph,
stashes, remotes, indexes), the selection (graph row and the file in the diff view), view settings, the
commit message draft, undo/redo stacks and the activity log.

`Session::run(id, params)` looks up the `CommandSpec`, checks `enabled` (against a fresh status for
commands that change the repository, since files change outside GitCraft), runs it under a panic guard,
reloads when it changed the repository, and logs it. `slow` commands (network) are run by the app on a
background thread in a `Session::detached` that never reloads; the window keeps working.

`refresh_if_changed` compares a fresh status and ref list with what is loaded and reloads only when they
differ: the app calls it every few seconds while it has focus and when it regains focus.

## ui-egui (L6)

`GitApp` owns the `Session`, preferences, dialogs, the palette, background jobs and the control channel.
`GitApp::run` dispatches `ui.*` commands itself, runs `slow` commands in the background and the rest in
the session. Views read session state and act only through command ids. Rows in the graph, diff and
blame are virtualised (`ScrollArea::show_rows`).

## apps

`gitcraft` creates the window (eframe, wgpu), loads and saves `ui.json` preferences, and with
`--control PORT` runs the loopback control server: a random key written to the settings folder (mode
0600), compared in constant time, one JSON request per line. `gitcraft-cli` wraps the engine for
scripts and runs the MCP server.
