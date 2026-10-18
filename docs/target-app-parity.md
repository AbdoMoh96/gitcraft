# Target app parity assessment

Status: M1 · Revision 1 (2026-10-17)

Measured: **91 of 124** catalogued features (73.4%), see `docs/parity-checklist.md`.

## Strong
- The daily loop: graph → select WIP → stage (file, hunk, lines) → commit → push, all with shortcuts.
- Branch work from the left panel and the graph: checkout, create anywhere, merge, rebase, fast-forward,
  rename, delete (local and remote), upstreams.
- Commit operations: cherry-pick, revert, reset (three modes), detached checkout, tags.
- Conflicts: detection, banner, take ours / theirs, mark resolved, continue / skip / abort, and a
  side-by-side merge tool picking hunks or single lines into an editable output.
- Repository tabs, reopened at start.
- Undo / redo for the operations people most want back.
- Automation: every action is a command, reachable from CLI, control channel and MCP.

## Missing (ranked in `docs/gaps.md`)
- Interactive rebase (reorder, squash, drop). Editing any commit message on the current branch works.
- Workspaces; profiles and author identity editing.
- Submodules, worktrees, Git LFS, Git Flow, hooks list, patches.
- Hosting integrations (pull requests, issues, CI status), cloud features.
- Diff niceties: syntax highlighting, word-level highlights, ignore whitespace, in-app editing.
- Graph niceties: solo / hide branches, column settings, compare two commits.
