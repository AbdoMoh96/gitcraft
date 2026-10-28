# GitCraft roadmap

**Stage:** M1 complete — a working Git client for the daily loop.

| Number | Value |
|---|---|
| Feature parity | 92 / 125 (73.6%) |
| Commands | 81 engine + 13 UI |
| Tests | 78 (`cargo xtask stats`) |
| Lines of Rust | ~14,200 |

Milestones and the current focus: [`docs/roadmap.md`](docs/roadmap.md). Ranked work:
[`docs/gaps.md`](docs/gaps.md).

## Progress log
- **2026-10-28 — M2.** Interactive rebase (`rebase.interactive`): Graph › Edit History › Interactive
  Rebase from Here opens the commits up to HEAD, newest first; each can be picked, reworded (with a new
  message), squashed, fixed up or dropped, and moved up or down. GitCraft writes the todo list and git
  takes it through `GIT_SEQUENCE_EDITOR`; new messages are applied with `exec` amend lines, so git
  never opens an editor. Uncommitted changes are stashed around it (`--autostash`). Shortcuts:
  Squash into Parent (`commit.squash`), Drop Commit (`commit.drop`), Move Up / Down
  (`commit.moveUp` / `commit.moveDown`). All undoable (`reset --keep` back to the old tip).
- **2026-10-28 — M3.** Tab reordering: drag a tab along the strip and the others slide aside as it
  passes their middle (eased, frame-rate independent); release settles it into its slot.
  Ctrl+Shift+PageUp / PageDown move the shown tab; `ui.tabs` `move` (`index`, `to` or `by`) for
  agents. The new order is what reopens at the next start.
- **2026-10-18 — M2.** Conflict prevention (`conflict.predict`): after every reload a background
  check merges the current branch in memory (`git merge-tree --write-tree`, git 2.38+) with its
  upstream when behind and with the target branch (`origin/HEAD`, else main / master). When either
  would conflict, the toolbar shows "Conflicts with <branch>"; a click lists the files. Nothing on
  disk is touched; Preferences can turn it off.
- **2026-10-18 — M2.** Merge tool (`conflict.edit`): clicking a conflicted file opens the current
  (ours) and incoming (theirs) sides side by side, aligned conflict by conflict; a box per conflict
  takes that side's whole hunk and a box per line takes single lines, which go to the output below
  in the order picked (All Current / All Incoming / Reset; Edit for hand changes). Save writes the
  output and marks the file resolved. The view is merged again from the index stages
  (`git merge-file --object-id`), falling back to the work tree's markers.
- **2026-10-18 — M3.** Repository tabs (`ui.tabs`): opening, cloning or creating a repository
  while one is shown opens a tab (an open one is shown instead); each tab keeps its own session,
  undo, selection and line picks; background jobs finish in their own tab. `+` / Mod+T, Ctrl+Tab,
  Ctrl+Shift+Tab, middle-click or Mod+W closes. Tabs are reopened at the next start.
- **2026-10-18 — M1.** Toolbar: the fetch / pull / push menu is named Actions and sits after Push.
  Graph lanes are continuous lines (rows no longer have a gap between them), WIP line included.
- **2026-10-17 — M1.** Edit commit message (`commit.reword`, undoable): click a commit's message in
  the commit panel (or Graph › Edit Commit Message) to edit it; older commits are re-created with
  `commit-tree`, keeping trees, authors and dates, without touching the index or work tree.
  Gravatar author pictures on graph dots, the author column and commit details, cached on disk for a
  week (Preferences toggle); hovering a dot shows the author's card.
- **2026-10-17 — M1.** Drag and drop branches in the graph (`branch.dropOnto`): drag a label onto
  another branch's row and pick Fast-forward (when possible), Merge or Rebase from the menu; the
  branch being changed is checked out first; fast-forwarding a branch that isn't checked out is
  undoable.
- **2026-10-17 — M1.** Hovering a graph label that stands for several refs (`topic +1`) lists them
  all; a click checks one out. Results and errors slide in as notifications at the bottom left
  (titled with the command, errors stay longer, hover keeps one, click closes). Terminal button on
  the toolbar (`ui.terminal`): opens the desktop's terminal in the repository (custom command in
  Preferences).
- **2026-10-17 — M1.** Double-clicking a remote branch in the graph (or the left panel) checks it
  out: a new local branch tracks it, or the existing one is checked out and fast-forwarded to it
  (undoable); a diverged branch is checked out and the notification says to pull.
- **2026-10-17 — M1.** Double-clicking a remote branch now pulls it (`branch.pullRemote`, in the
  background): fetch, check out (tracking a new local branch, setting a missing upstream),
  fast-forward; when the two have diverged a dialog offers merge, rebase or reset to the remote.
- **2026-10-17 — M1.** Release workflow: pushing tag `v<version>` checks it against Cargo.toml,
  runs `cargo xtask ci`, builds Linux (tarball + installer), macOS (arm64 and x86_64 `.app`) and
  Windows (zip) packages and publishes a GitHub Release (`docs/releasing.md`).
- **2026-10-17 — M1.** Workspace in the WordCraft layout (control-key, git, graph, engine, mcp,
  ui-egui, apps, xtask). Clean-room inventory of the target (`docs/clean-room.md`). git layer with
  parsers for status v2, log, refs, diffs, blame, stashes, remotes and hunk/line patches; operations
  for staging, commits, branches, merges, rebases, cherry-picks, reverts, resets, tags, remotes,
  stashes and conflicts. Graph lane layout. Engine with 79 commands, undo/redo, feature catalog,
  hostile-params test. egui window: toolbar, refs panel, virtualised graph, commit panel, diff view
  (hunk / inline / split, line staging, blame), dialogs, command palette, light/dark themes,
  background network jobs, auto-reload. CLI with a text graph, MCP server, loopback control channel
  with key file. `cargo xtask ci` green.
