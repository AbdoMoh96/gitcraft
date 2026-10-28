# Gaps: the ranked work list

Status: M1 · Revision 1 (2026-10-17)

Highest value first. Each item names the catalog ids it fills (`docs/parity-checklist.md`).

1. ~~Interactive rebase~~ — landed (`rebase.interactive`, `commit.squash`, `commit.drop`, `commit.moveUp`,
   `commit.moveDown`): the todo list is written by GitCraft and handed to git through
   `GIT_SEQUENCE_EDITOR`; new messages are applied by `exec` amend lines. Next: rebasing across merges.
2. ~~Merge tool output editor~~ — landed (`conflict.edit`). Next: syntax colours in its panes, a
   base column for `diff3`, editing a picked line in place.
3. ~~Repository tabs~~ — landed (`ui.tabs`). Drag to reorder landed. Next: workspaces (`workspace.open`).
4. ~~Diff quality~~ — landed: `view.ignoreWhitespace` (`-w`), `view.wordDiff` (word-level LCS between a
   removed line and the added line replacing it), `view.syntax` (GitCraft's own per-line highlighter,
   `crates/ui-egui/src/highlight.rs`), image diffs, `file.content`. Next: multi-line comment state.
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
