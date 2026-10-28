//! The session: one open repository, what is loaded from it, the selection, the commit message
//! being written, view settings, undo and the activity log.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use gitcraft_git::{Commit, FileDiff, Git, LogQuery, Ref, RefKind, Remote, Repo, Stash, Status};
use gitcraft_graph::Layout;
use serde::Serialize;
use serde_json::{Value, json};

use crate::cmd::edit::UndoStep;
use crate::{CmdError, CmdResult, CommandSpec, Registry};

/// How the diff view shows a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffMode {
    /// Hunks, each with its stage / unstage / discard buttons.
    #[default]
    Hunk,
    /// The whole file with the changes inline.
    Inline,
    /// Old and new side by side.
    Split,
}

impl DiffMode {
    pub const ALL: [DiffMode; 3] = [DiffMode::Hunk, DiffMode::Inline, DiffMode::Split];
    pub fn code(self) -> &'static str {
        match self {
            DiffMode::Hunk => "hunk",
            DiffMode::Inline => "inline",
            DiffMode::Split => "split",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DiffMode::Hunk => "Hunk",
            DiffMode::Inline => "Inline",
            DiffMode::Split => "Split",
        }
    }
    pub fn parse(s: &str) -> Option<DiffMode> {
        DiffMode::ALL.into_iter().find(|m| m.code().eq_ignore_ascii_case(s.trim()))
    }
}

/// Which list a selected file is in.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "oid")]
pub enum Area {
    Unstaged,
    Staged,
    /// A file of a commit.
    Commit(String),
}

impl Area {
    pub fn parse(s: &str, oid: Option<&str>) -> Option<Area> {
        match s.trim().to_ascii_lowercase().as_str() {
            "unstaged" | "worktree" => Some(Area::Unstaged),
            "staged" | "index" => Some(Area::Staged),
            "commit" => oid.map(|o| Area::Commit(o.to_string())),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSel {
    pub path: String,
    pub area: Area,
}

/// The selected graph row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "oid")]
pub enum Selected {
    #[default]
    None,
    /// The work in progress row: the commit panel with staged and unstaged files.
    Wip,
    Commit(String),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Selection {
    pub row: Selected,
    /// The file open in the diff view.
    pub file: Option<FileSel>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub diff_mode: DiffMode,
    /// Lines of context around each change.
    pub context: u32,
    /// How many commits the graph loads.
    pub commit_limit: usize,
    pub left_panel: bool,
    /// Highlight commits whose message, author or id matches.
    pub search: String,
    /// The diff view shows blame instead of changes.
    pub blame: bool,
    /// The file whose history the graph is showing, if any.
    pub history: Option<String>,
    /// Diffs leave out changes in whitespace (`git diff -w`); lines can't be staged then.
    pub ignore_whitespace: bool,
    /// Highlight the words that changed within changed lines.
    pub word_diff: bool,
    /// Colour code by language.
    pub syntax: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            diff_mode: DiffMode::Hunk,
            context: 3,
            commit_limit: 2000,
            left_panel: true,
            search: String::new(),
            blame: false,
            history: None,
            ignore_whitespace: false,
            word_diff: true,
            syntax: true,
        }
    }
}

/// The commit message being written in the commit panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CommitDraft {
    pub summary: String,
    pub description: String,
    pub amend: bool,
}

impl CommitDraft {
    pub fn message(&self) -> String {
        let s = self.summary.trim();
        let d = self.description.trim();
        if d.is_empty() { s.to_string() } else { format!("{s}\n\n{d}") }
    }
    pub fn clear(&mut self) {
        *self = CommitDraft::default();
    }
}

/// Everything loaded from the repository.
#[derive(Clone, Debug, Default)]
pub struct RepoData {
    pub status: Status,
    pub refs: Vec<Ref>,
    /// Newest first; the graph's rows.
    pub commits: Vec<Commit>,
    pub graph: Layout,
    pub stashes: Vec<Stash>,
    pub remotes: Vec<Remote>,
    /// Commit id → row.
    pub row_of: HashMap<String, usize>,
    /// Commit id → indices into `refs` of the refs pointing at it.
    pub labels: HashMap<String, Vec<usize>>,
    /// More commits exist than were loaded.
    pub more: bool,
}

impl RepoData {
    pub fn local_branches(&self) -> impl Iterator<Item = &Ref> {
        self.refs.iter().filter(|r| r.kind == RefKind::Local)
    }
    pub fn remote_branches(&self) -> impl Iterator<Item = &Ref> {
        self.refs.iter().filter(|r| r.kind == RefKind::Remote)
    }
    pub fn tags(&self) -> impl Iterator<Item = &Ref> {
        self.refs.iter().filter(|r| r.kind == RefKind::Tag)
    }
    pub fn commit(&self, oid: &str) -> Option<&Commit> {
        self.row_of.get(oid).and_then(|&i| self.commits.get(i))
    }
    pub fn head_ref(&self) -> Option<&Ref> {
        self.refs.iter().find(|r| r.head)
    }
}

/// One entry of the activity log.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Activity {
    pub command: String,
    pub label: String,
    pub ok: bool,
    pub message: String,
}

/// The longest the activity log grows.
const MAX_ACTIVITY: usize = 500;

pub struct Session {
    pub registry: Arc<Registry>,
    pub git: Git,
    pub repo: Option<Repo>,
    pub data: RepoData,
    pub sel: Selection,
    pub view: ViewState,
    pub draft: CommitDraft,
    pub activity: Vec<Activity>,
    pub(crate) undo: Vec<UndoStep>,
    pub(crate) redo: Vec<UndoStep>,
    /// A session that runs one command for the app's background worker: it never reloads.
    detached: bool,
    /// Bumped by every reload; keys the diff cache.
    generation: u64,
    diff_cache: Option<((FileSel, u32, bool, bool, u64), Result<FileDiff, CmdError>)>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Git::default())
    }
}

impl Session {
    pub fn new(git: Git) -> Self {
        Session {
            registry: Arc::new(crate::cmd::registry()),
            git,
            repo: None,
            data: RepoData::default(),
            sel: Selection::default(),
            view: ViewState::default(),
            draft: CommitDraft::default(),
            activity: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            detached: false,
            generation: 0,
            diff_cache: None,
        }
    }

    /// A session over `repo` that runs one command and never reloads: what the app's background
    /// worker uses for network commands while the window stays responsive.
    pub fn detached(repo: Repo) -> Self {
        let mut s = Session::new(repo.git().clone());
        s.data.status = repo.status().unwrap_or_default();
        s.data.remotes = repo.remotes().unwrap_or_default();
        s.data.stashes = repo.stashes().unwrap_or_default();
        s.repo = Some(repo);
        s.detached = true;
        s
    }

    /// A background worker's session (see [`Session::detached`]).
    pub fn is_detached(&self) -> bool {
        self.detached
    }

    /// Open the repository containing `path`.
    pub fn open(&mut self, path: &Path) -> Result<(), CmdError> {
        let repo = self.git.open(path)?;
        self.set_repo(repo)
    }

    pub fn set_repo(&mut self, repo: Repo) -> Result<(), CmdError> {
        self.repo = Some(repo);
        self.data = RepoData::default();
        self.sel = Selection::default();
        self.draft = CommitDraft::default();
        self.view.history = None;
        self.view.search.clear();
        self.undo.clear();
        self.redo.clear();
        self.refresh()?;
        self.sel.row =
            if self.data.status.is_clean() { self.data.status.head.clone().map(Selected::Commit).unwrap_or(Selected::Wip) } else { Selected::Wip };
        Ok(())
    }

    pub fn close(&mut self) {
        self.repo = None;
        self.data = RepoData::default();
        self.sel = Selection::default();
        self.draft = CommitDraft::default();
        self.undo.clear();
        self.redo.clear();
        self.diff_cache = None;
    }

    pub fn repo(&self) -> Result<&Repo, CmdError> {
        self.repo.as_ref().ok_or_else(|| CmdError::Disabled("no repository is open".into()))
    }

    /// Reload status, branches, history, stashes and remotes.
    pub fn refresh(&mut self) -> Result<(), CmdError> {
        if self.detached {
            return Ok(());
        }
        let Some(repo) = self.repo.clone() else { return Ok(()) };
        self.generation = self.generation.wrapping_add(1);
        self.diff_cache = None;
        let status = repo.status()?;
        let refs = repo.refs()?;
        let limit = self.view.commit_limit.clamp(1, gitcraft_git::log::MAX_LIMIT - 1);
        let q = LogQuery {
            revs: if self.view.history.is_some() { vec!["HEAD".into()] } else { Vec::new() },
            limit: limit + 1,
            path: self.view.history.clone(),
            follow: self.view.history.is_some(),
            ..LogQuery::default()
        };
        let mut commits = if status.head.is_some() || !refs.is_empty() { repo.log(&q)? } else { Vec::new() };
        let more = commits.len() > limit;
        commits.truncate(limit);
        let graph = gitcraft_graph::layout(commits.iter().map(|c| (c.oid.as_str(), c.parents.as_slice())));
        let row_of = commits.iter().enumerate().map(|(i, c)| (c.oid.clone(), i)).collect();
        let mut labels: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, r) in refs.iter().enumerate() {
            labels.entry(r.commit.clone()).or_default().push(i);
        }
        let stashes = repo.stashes().unwrap_or_default();
        let remotes = repo.remotes().unwrap_or_default();
        self.data = RepoData { status, refs, commits, graph, stashes, remotes, row_of, labels, more };
        self.fix_selection();
        Ok(())
    }

    /// Reload only when something changed outside GitCraft (status or refs differ): cheap enough
    /// to call every few seconds while the window has focus. Whether it reloaded.
    pub fn refresh_if_changed(&mut self) -> Result<bool, CmdError> {
        let Some(repo) = self.repo.clone() else { return Ok(false) };
        if self.detached {
            return Ok(false);
        }
        let mut status = repo.status()?;
        let refs = repo.refs()?;
        let stash = repo.resolve("refs/stash");
        // The cached status went through the same parse, so a plain comparison is exact.
        status.state = repo.state();
        let same = status == self.data.status && refs == self.data.refs && stash == self.data.stashes.first().map(|s| s.oid.clone());
        if same {
            return Ok(false);
        }
        self.refresh()?;
        Ok(true)
    }

    /// Bumped by every reload: views that cache what they computed from the repository key on it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Keep the selection pointing at things that still exist.
    fn fix_selection(&mut self) {
        if let Selected::Commit(oid) = &self.sel.row
            && !self.data.row_of.contains_key(oid)
        {
            self.sel.row = Selected::Wip;
            self.sel.file = None;
        }
        let keep = match &self.sel.file {
            None => true,
            Some(FileSel { path, area: Area::Unstaged }) => self.data.status.unstaged().any(|f| &f.path == path),
            Some(FileSel { path, area: Area::Staged }) => self.data.status.staged().any(|f| &f.path == path),
            Some(FileSel { area: Area::Commit(oid), .. }) => self.sel.row == Selected::Commit(oid.clone()),
        };
        if !keep {
            self.sel.file = None;
        }
    }

    /// Run a command by id.
    pub fn run(&mut self, id: &str, params: &Value) -> CmdResult {
        let reg = self.registry.clone();
        let Some(spec) = reg.get(id) else { return Err(CmdError::Unknown(id.to_string())) };
        // Files change outside GitCraft (editors, build tools, agents): a command that changes
        // the repository is checked against the work tree as it is now.
        if spec.mutates
            && let Some(repo) = &self.repo
            && let Ok(st) = repo.status()
        {
            self.data.status = st;
        }
        if let Some(why) = (spec.enabled)(self) {
            return Err(CmdError::Disabled(format!("{id}: {why}")));
        }
        let params = if params.is_null() { &Value::Object(Default::default()) } else { params };
        // Last-resort guard: a panic in a command becomes an error and the session reloads.
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (spec.run)(self, params)))
            .unwrap_or_else(|_| Err(CmdError::Failed(format!("internal error in `{id}`; nothing was lost, the repository has been reloaded"))));
        if spec.mutates
            && let Err(e) = self.refresh()
        {
            log::warn!("reload after {id}: {e}");
        }
        if spec.mutates {
            self.note(spec, &r);
        }
        r
    }

    /// Record a command's result in the activity log (also for results the app's background
    /// worker brings back).
    pub fn note(&mut self, spec: &CommandSpec, r: &CmdResult) {
        let message = match r {
            Ok(v) => v.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
            Err(e) => e.to_string(),
        };
        self.activity.push(Activity { command: spec.id.to_string(), label: spec.label.to_string(), ok: r.is_ok(), message });
        if self.activity.len() > MAX_ACTIVITY {
            let extra = self.activity.len() - MAX_ACTIVITY;
            self.activity.drain(..extra);
        }
    }

    /// The diff of the selected file (cached until the next reload).
    pub fn selected_diff(&mut self) -> Option<Result<FileDiff, CmdError>> {
        let sel = self.sel.file.clone()?;
        let ws = self.view.ignore_whitespace;
        let key = (sel.clone(), self.view.context, self.view.diff_mode == DiffMode::Inline, ws, self.generation);
        if let Some((k, d)) = &self.diff_cache
            && *k == key
        {
            return Some(d.clone());
        }
        // Inline shows the whole file: as much context as it has lines.
        let context = if self.view.diff_mode == DiffMode::Inline { 100_000 } else { self.view.context };
        let d = self.diff_with(&sel, context, ws);
        self.diff_cache = Some((key, d.clone()));
        Some(d)
    }

    /// The diff of a file as staging sees it (whitespace included).
    pub fn diff_for(&self, sel: &FileSel, context: u32) -> Result<FileDiff, CmdError> {
        self.diff_with(sel, context, false)
    }

    /// The diff of a file, leaving out whitespace changes when `ignore_ws`.
    pub fn diff_with(&self, sel: &FileSel, context: u32, ignore_ws: bool) -> Result<FileDiff, CmdError> {
        let repo = self.repo()?;
        let o = gitcraft_git::diff::DiffOpts { context, ignore_whitespace: ignore_ws };
        Ok(match &sel.area {
            Area::Unstaged => repo.diff_unstaged_with(&sel.path, o)?,
            Area::Staged => repo.diff_staged_with(&sel.path, o)?,
            Area::Commit(oid) => repo.diff_commit_file_with(oid, &sel.path, o)?,
        })
    }

    /// A file's bytes at `rev`: a commit, `index`, or `worktree` (read from disk, inside the
    /// repository only).
    pub fn file_bytes(&self, path: &str, rev: &str) -> Result<Vec<u8>, CmdError> {
        let repo = self.repo()?;
        let path = gitcraft_git::check_path(path)?;
        match rev {
            "worktree" | "wip" => {
                if path.split(['/', '\\']).any(|c| c == "..") || Path::new(path).is_absolute() {
                    return Err(CmdError::Params("the path must be inside the repository".into()));
                }
                std::fs::read(repo.root().join(path)).map_err(|e| CmdError::Failed(format!("{path}: {e}")))
            }
            "index" => Ok(repo.show_file(":0", path)?),
            r => Ok(repo.show_file(gitcraft_git::check_name("revision", r)?, path)?),
        }
    }

    /// The old and new bytes of the selected file's change (for image diffs): `None` for a side
    /// that doesn't exist (an added or deleted file).
    pub fn file_sides(&self, sel: &FileSel) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let (old, new) = match &sel.area {
            Area::Unstaged => ("index".to_string(), "worktree".to_string()),
            Area::Staged => ("HEAD".to_string(), "index".to_string()),
            Area::Commit(o) => (format!("{o}^1"), o.clone()),
        };
        let old =
            self.file_bytes(&sel.path, &old).ok().or_else(|| if sel.area == Area::Unstaged { self.file_bytes(&sel.path, "HEAD").ok() } else { None });
        (old, self.file_bytes(&sel.path, &new).ok())
    }

    /// The state agents see (`repo.status` / the `gitcraft://repository` resource).
    pub fn inspect(&self) -> Value {
        let Some(repo) = &self.repo else { return json!({"open": false}) };
        let st = &self.data.status;
        let files = |it: &mut dyn Iterator<Item = &gitcraft_git::FileStatus>| -> Vec<Value> {
            it.map(|f| json!({"path": f.path, "origPath": f.orig_path, "index": f.index, "worktree": f.worktree, "conflict": f.conflict})).collect()
        };
        json!({
            "open": true,
            "path": repo.root().display().to_string(),
            "name": repo.name(),
            "branch": st.branch,
            "head": st.head,
            "upstream": st.upstream,
            "ahead": st.ahead,
            "behind": st.behind,
            "state": st.state,
            "staged": files(&mut st.staged()),
            "unstaged": files(&mut st.unstaged()),
            "conflicts": st.conflicted().map(|f| f.path.clone()).collect::<Vec<_>>(),
            "branches": self.data.local_branches().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "remoteBranches": self.data.remote_branches().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "tags": self.data.tags().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "stashes": self.data.stashes.iter().map(|s| json!({"index": s.index, "message": s.message})).collect::<Vec<_>>(),
            "remotes": self.data.remotes.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "commitsLoaded": self.data.commits.len(),
            "moreCommits": self.data.more,
            "selection": self.sel,
            "draft": self.draft,
            "canUndo": self.undo.last().map(UndoStep::label),
            "canRedo": self.redo.last().map(UndoStep::label),
        })
    }
}
