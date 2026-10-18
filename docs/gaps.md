# Gaps: the ranked work list

Status: M1 · Revision 1 (2026-10-17)

Highest value first. Each item names the catalog ids it fills (`docs/parity-checklist.md`).

1. **Interactive rebase** — `rebase.interactive`, `commit.squash`, `commit.drop` (`commit.reword` landed: it
   re-creates commits with `commit-tree`, no rebase needed): a
   todo list editor driven by `GIT_SEQUENCE_EDITOR` writing a prepared todo file.
2. ~~Merge tool output editor~~ — landed (`conflict.edit`). Next: syntax colours in its panes, a
   base column for `diff3`, editing a picked line in place.
3. ~~Repository tabs~~ — landed (`ui.tabs`). Next: drag to reorder, workspaces (`workspace.open`).
4. **Diff quality** — `view.ignoreWhitespace` (`-w`), `view.wordDiff` (intra-line highlights),
   `view.syntax` (a pure-Rust highlighter).
5. **Graph controls** — `graph.solo`, `graph.hide`, `graph.columns`, `diff.compare` (two selected commits).
6. **Identity and signing** — `config.identity` (user.name / email per repo), `commit.sign`,
   `commit.noVerify`, `commit.template`.
7. **Submodules and worktrees** — `submodule.list`, `worktree.list`.
8. **Tools** — `file.ignore`, `patch.create`, `patch.apply`, `lfs.track`, `gitflow.init`, `hooks.list`,
   `ui.terminal`, `file.edit`.
9. **Hosting integrations** — `pr.list`, `pr.create`, `issues.list`, `ci.list`, `integrations.connect`
   (needs a decision from the owner on accounts and tokens).
10. **Workspaces, profiles, cloud** — `workspace.open`, `profile.switch`, `cloudpatch.create`,
    `launchpad.open`.
