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
5. ~~Graph controls~~ — landed: `graph.solo`, `graph.hide`, `graph.showAll`, `graph.refs` (remote branches and
   tags on or off), `graph.columns`, `diff.compare` (two commits, or a commit and the working tree;
   Mod-click a second commit), author highlighting, an overview strip, `ui.zoom`.
6. ~~Identity and signing~~ — landed: `config.identity` (this repository or every one), `config.signing`
   (OpenPGP, SSH or X.509 keys), `commit.sign`, `commit.noVerify`, `commit.template`, `commit.coAuthors`,
   `ssh.keys` / `ssh.generate`.
7. ~~Submodules and worktrees~~ — landed: `submodule.list/update/add/sync/remove`,
   `worktree.list/add/remove/prune`; left panel sections; open either in a tab.
8. ~~Tools~~ — landed: `file.ignore`, `file.edit` (in-app editor), `patch.create` / `patch.apply`,
   `lfs.*`, `gitflow.init/start/finish` (native, git-flow's config keys), `hooks.*`, `tool.diff` /
   `tool.merge` (git's configured tools, not waited for), open / edit / reveal a file.
9. ~~Hosting integrations~~ — landed through the services' own tools (`gh`, `glab`), so GitCraft keeps
   no tokens: `pr.list/create/checkout`, `issues.list`, `ci.list`, `integrations.connect` (sign-in runs
   in the built-in terminal), `launchpad.open`, `hosting.repos`, `cloudpatch.create` (secret gist /
   private snippet). AI through a command the user sets (`gitcraft.aiCommand`): `commit.generateMessage`,
   `commit.explain`, `pr.describe`, `conflict.suggest`. Local: `repo.insights`, `team.overlap`.
   **Owner decision:** Jira / Trello issue trackers need API tokens stored by GitCraft.
10. ~~Workspaces, profiles~~ — landed: `ui.workspaces`, `ui.profile` (own tabs, favourites and commit
    author), `repo.scan`, `ui.treeView`. Cloud: `cloudpatch.create`, `launchpad.open` (see 9).
