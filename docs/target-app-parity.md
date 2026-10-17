# Target app parity assessment

Status: M1 · Revision 1 (2026-10-17)

Measured: **86 of 123** catalogued features (69.9%), see `docs/parity-checklist.md`.

## Strong
- The daily loop: graph → select WIP → stage (file, hunk, lines) → commit → push, all with shortcuts.
- Branch work from the left panel and the graph: checkout, create anywhere, merge, rebase, fast-forward,
  rename, delete (local and remote), upstreams.
- Commit operations: cherry-pick, revert, reset (three modes), detached checkout, tags.
- Conflicts: detection, banner, take ours / theirs, mark resolved, continue / skip / abort.
- Undo / redo for the operations people most want back.
- Automation: every action is a command, reachable from CLI, control channel and MCP.

## Missing (ranked in `docs/gaps.md`)
- Interactive rebase (reorder, squash, drop). Editing any commit message on the current branch works.
- A three-way merge output editor (today: take a side or edit the file elsewhere, then mark resolved).
- Repository tabs and workspaces; profiles and author identity editing.
- Submodules, worktrees, Git LFS, Git Flow, hooks list, patches.
- Hosting integrations (pull requests, issues, CI status), cloud features.
- Diff niceties: syntax highlighting, word-level highlights, ignore whitespace, in-app editing.
- Graph niceties: solo / hide branches, column settings, compare two commits.
