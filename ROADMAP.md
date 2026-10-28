# GitCraft roadmap

**Stage:** M6 — parity with the incumbent's feature set, hosting through gh / glab.

| Number | Value |
|---|---|
| Feature parity | 92 / 125 (73.6%) |
| Commands | 81 engine + 13 UI |
| Tests | 78 (`cargo xtask stats`) |
| Lines of Rust | ~14,200 |

Milestones and the current focus: [`docs/roadmap.md`](docs/roadmap.md). Ranked work:
[`docs/gaps.md`](docs/gaps.md).

## Progress log
- **2026-10-28 — M6.** Hosting through GitHub's `gh` and GitLab's `glab` (they keep the sign-in;
  GitCraft never sees a token): Pull Requests, Issues and CI sections in the left panel (`pr.list`,
  `issues.list`, `ci.list`; open, check out a pull request, branch for an issue), Create Pull Request
  from a branch's menu (`pr.create`: pushes the branch first), the Launchpad (`launchpad.open`: your
  pull requests, reviews waiting for you and assigned issues across GitHub), Clone › your
  repositories (`hosting.repos`), Share as Cloud Patch (`cloudpatch.create`: a secret gist or private
  snippet), and Accounts (`integrations.connect`: status, and Sign In runs `gh auth login` in the
  terminal). AI through a command the user sets (`gitcraft.aiCommand`, Accounts and AI; off by
  default): Generate commit message, Explain commit, Write pull request description, Conflict help.
  Insights (`repo.insights`: commits by author, weekday, most changed files) and Team: Changes Like
  Yours (`team.overlap`: recent remote branches touching the files you're changing), both local.
- **2026-10-28 — M3.** A terminal inside GitCraft (`ui.terminal`, Mod+`): the user's shell in a
  pseudo-terminal (portable-pty) opened in the repository, shown in a resizable panel at the bottom
  and drawn by a VT100 emulator (vt100), with colours, scrollback and the keys a shell needs; the
  desktop's terminal is still in the repository menu and Preferences. Instant reloads: the open
  repository's folder is watched (notify) and any change outside git's object store reloads it at
  once; the periodic check stays as a fallback. Links: `gitcraft://open?path=…` (and `file://`
  URLs) open a repository; the Linux installer registers the scheme. Learn Git (`ui.learn`, start
  screen and repository menu): a practice repository (`repo.practice`) and nine lessons that tick
  off as their commands succeed.
- **2026-10-28 — M3.** Many repositories. Workspaces (`ui.workspaces`: save the open tabs under a
  name, open them all again from the repository menu). Profiles (`ui.profile`: each keeps its own
  tabs and favourites and can commit as its own author, set as git's author and committer for the
  profile's repositories). Find Repositories (`repo.scan`: the git repositories under a folder,
  three levels down; open them in tabs or save them as a workspace). The commit panel's Tree view
  groups files by folder (`ui.treeView`), with stage / unstage for a whole folder.
- **2026-10-28 — M3.** Everyday extras. Stashes: Stash This File (`stash.save` with `paths`), Rename
  (`stash.rename`), View Changes (a compare of the stash with its base). Remotes: Edit URL and Rename
  (`remote.editUrl`, `remote.rename`), Prune Deleted Branches (`remote.prune`), drag a local branch
  onto a remote in the left panel to push it there. Tags: Delete from Remote (`tag.deleteRemote`).
  Mod-click several local branches to delete them together (`branch.delete` with `names`, one undo);
  Shift-click several commits to cherry-pick them together (`cherryPick.commit` with `oids`). Pin a
  branch to the top of the left panel (`ui.pin`), favourite repositories (`ui.favorite`, first in the
  repository menu and the start screen), automatic fetch every N minutes (`ui.autoFetch`,
  Preferences; quiet unless remote branches changed), and an Activity list in the status bar with
  every result and error of the session.
- **2026-10-28 — M5.** Tools. Ignore (`file.ignore`: the file, its extension, its folder, or only in
  this clone through `.git/info/exclude`). Edit in GitCraft (`file.edit`: an editor in the center with
  syntax colours, Mod+S saves) and Open in Editor / with the Default App / Show in File Manager
  (`ui.openInEditor`, `ui.openFile`, `ui.revealFile`; the editor command is in Preferences). Patches
  (`patch.create`: a commit as a mail patch or the uncommitted changes, to a file or the clipboard;
  `patch.apply`: `git am --3way` for mail patches, else `git apply --3way`). Git LFS (`lfs.list`,
  `lfs.track`, `lfs.untrack`, `lfs.pull`). Git Flow (`gitflow.init`, `gitflow.start`,
  `gitflow.finish`: done by GitCraft with git-flow's config keys; a release is merged into main,
  tagged, and the tag merged back into develop). Git Hooks (`hooks.list`, on / off, read and write a
  script). External diff and merge tools (`tool.diff`, `tool.merge`: git's configured tool, started
  without waiting).
- **2026-10-28 — M5.** Submodules and worktrees in the left panel. Submodules (`submodule.list`) show
  whether each is cloned, changed or in conflict; Update / Clone (`submodule.update`, in the
  background), Add Submodule (`submodule.add`), Sync URLs, Remove (`submodule.remove`), and a
  double-click opens one in a tab. Worktrees (`worktree.list`): Add Worktree (`worktree.add`: a
  branch, new or existing, in a folder beside the repository), Remove (`worktree.remove`), Prune
  Missing, and a double-click opens one in a tab.
- **2026-10-28 — M3.** Who commits and how: Author Identity (`config.identity`, for this repository or
  every repository), Commit Signing (`config.signing`: GPG, SSH or X.509 key, sign commits and tags by
  default), SSH Keys (`ssh.keys`: copy a public key; `ssh.generate`: a new ed25519 key with
  `ssh-keygen`, never overwriting one). The commit form has Sign (`commit.sign`), Skip hooks
  (`commit.noVerify`), Co-authors (`commit.coAuthors`: picked from the history's authors, written as
  `Co-authored-by:` trailers) and Template (`commit.template`). Settings the views need are loaded
  with each reload in one `git config --get-regexp` run.
- **2026-10-28 — M4.** Graph controls: Solo and Hide a branch (`graph.solo`, `graph.hide`, from a
  branch's menu; the header shows the filter with Show all), remote branches and tags on or off and
  the Branch / Author / Date / Commit columns (right-click the graph header: `graph.refs`,
  `graph.columns`), Compare (`diff.compare`: Mod-click a second commit, or Compare › With the Working
  Tree / HEAD / the Selected Commit; the commit panel lists the files, a click opens the diff),
  Highlight Commits by an author, an overview strip of the history at the graph's right edge
  (branch tips, HEAD, search hits, the visible window; click to jump), and zoom (`ui.zoom`,
  Preferences and Mod +/-, remembered).
- **2026-10-28 — M4.** Diff quality: View › Ignore whitespace (`view.ignoreWhitespace`, `git diff -w`;
  staging is disabled while it hides changes), Highlight changed words (`view.wordDiff`: the words
  that differ between a removed line and the added line that replaced it), Syntax colours
  (`view.syntax`: GitCraft's own highlighter for about twenty languages, chosen by extension).
  Changed images (PNG, JPEG, GIF, BMP, WebP, ICO) show before and after side by side.
  `file.content` returns a file at a commit, in the index or in the work tree.
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
