//! Operations that change the repository: staging, commits, branches, merges, rebases, tags,
//! remotes, stashes and conflicts.

use serde::Serialize;

use crate::{GitError, Repo, Result, check_name, check_path};

/// What [`Repo::reword`] did: HEAD before and after, and the edited commit's new id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reworded {
    pub before: String,
    pub after: String,
    pub commit: String,
}

/// Commits re-created at most when editing an older commit's message.
const MAX_REWRITE: usize = 10_000;

/// `Name <email> 1700000000 +0200` → (name, email, date in git's internal format).
fn parse_ident(v: &str) -> Option<(String, String, String)> {
    let lt = v.find('<')?;
    let gt = lt.checked_add(v.get(lt..)?.find('>')?)?;
    let name = v.get(..lt)?.trim().to_string();
    let email = v.get(lt.checked_add(1)?..gt)?.to_string();
    let date = v.get(gt.checked_add(1)?..)?.trim().to_string();
    Some((name, email, date))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    /// Move the branch; keep the index and work tree.
    Soft,
    /// Move the branch and reset the index; keep the work tree.
    Mixed,
    /// Move the branch and throw away every change.
    Hard,
}

impl ResetMode {
    pub fn parse(s: &str) -> Option<ResetMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "soft" => Some(ResetMode::Soft),
            "mixed" => Some(ResetMode::Mixed),
            "hard" => Some(ResetMode::Hard),
            _ => None,
        }
    }
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PullMode {
    /// Fast-forward when possible, merge otherwise.
    #[default]
    Merge,
    FastForwardOnly,
    Rebase,
}

impl PullMode {
    pub fn parse(s: &str) -> Option<PullMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "merge" | "ff" => Some(PullMode::Merge),
            "ffonly" | "ff-only" | "fastforwardonly" => Some(PullMode::FastForwardOnly),
            "rebase" => Some(PullMode::Rebase),
            _ => None,
        }
    }
}

/// How an operation that can stop on conflicts ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub message: String,
    /// It stopped with files in conflict: resolve them, then continue or abort.
    pub conflicts: bool,
}

fn paths(ps: &[String]) -> Result<Vec<&str>> {
    if ps.is_empty() {
        return Err(GitError::Arg("no files given".into()));
    }
    ps.iter().map(|p| check_path(p)).collect()
}

impl Repo {
    /// Run an operation that may stop with conflicts: a failure that leaves conflicted files is
    /// an [`Outcome`] with `conflicts`, any other failure an error.
    fn conflicting(&self, args: &[&str]) -> Result<Outcome> {
        let o = self.run(args)?;
        if o.ok() {
            return Ok(Outcome { message: o.message(), conflicts: false });
        }
        if self.status().map(|s| s.conflicted().next().is_some()).unwrap_or(false) {
            return Ok(Outcome { message: o.message(), conflicts: true });
        }
        Err(crate::runner::failed(args, &o))
    }

    /// A branch name git accepts (`git check-ref-format --branch`).
    pub fn check_branch_name(&self, name: &str) -> Result<String> {
        let name = check_name("branch name", name)?;
        let o = self.run(&["check-ref-format", "--branch", name])?;
        if o.ok() { Ok(o.text().trim().to_string()) } else { Err(GitError::Arg(format!("`{name}` is not a valid branch name"))) }
    }

    fn check_tag_name(&self, name: &str) -> Result<String> {
        let name = check_name("tag name", name)?;
        let full = format!("refs/tags/{name}");
        let o = self.run(&["check-ref-format", full.as_str()])?;
        if o.ok() { Ok(name.to_string()) } else { Err(GitError::Arg(format!("`{name}` is not a valid tag name"))) }
    }

    // ---- staging ----

    pub fn stage(&self, ps: &[String]) -> Result<()> {
        let mut args = vec!["add", "-A", "--"];
        args.extend(paths(ps)?);
        self.run_paths(&args).map(|_| ())
    }

    pub fn stage_all(&self) -> Result<()> {
        self.run_ok(&["add", "-A"]).map(|_| ())
    }

    pub fn unstage(&self, ps: &[String]) -> Result<()> {
        let ps = paths(ps)?;
        let mut args =
            if self.has_commits() { vec!["reset", "-q", "HEAD", "--"] } else { vec!["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--"] };
        args.extend(ps);
        self.run_paths(&args).map(|_| ())
    }

    pub fn unstage_all(&self) -> Result<()> {
        if self.has_commits() { self.run_ok(&["reset", "-q"]) } else { self.run_ok(&["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--", "."]) }
            .map(|_| ())
    }

    /// Throw away the work tree's changes to these files (untracked files are deleted).
    pub fn discard(&self, ps: &[String]) -> Result<()> {
        let ps = paths(ps)?;
        let st = self.status()?;
        let (untracked, tracked): (Vec<&str>, Vec<&str>) = ps.into_iter().partition(|p| st.file(p).is_some_and(|f| f.is_untracked()));
        if !untracked.is_empty() {
            let mut args = vec!["clean", "-f", "-q", "--"];
            args.extend(untracked);
            self.run_paths(&args)?;
        }
        if !tracked.is_empty() {
            let mut args = vec!["checkout", "-q", "--"];
            args.extend(tracked);
            self.run_paths(&args)?;
        }
        Ok(())
    }

    /// Throw away every change, staged or not, and delete untracked files.
    pub fn discard_all(&self) -> Result<()> {
        if !self.has_commits() {
            return Err(GitError::Arg("there is no commit to go back to".into()));
        }
        self.run_ok(&["reset", "--hard", "-q"])?;
        self.run_ok(&["clean", "-f", "-d", "-q"]).map(|_| ())
    }

    // ---- commits ----

    /// Commit the index with `message`; the new commit's id.
    pub fn commit(&self, message: &str, amend: bool) -> Result<String> {
        if message.trim().is_empty() {
            return Err(GitError::Arg("the commit message is empty".into()));
        }
        if message.len() > crate::MAX_ARG {
            return Err(GitError::Arg("the commit message is too long".into()));
        }
        let mut args = vec!["commit", "-q", "--cleanup=strip", "-F", "-"];
        if amend {
            args.push("--amend");
        }
        self.run_input(&args, message.as_bytes())?;
        self.resolve("HEAD").ok_or_else(|| GitError::Parse("no HEAD after commit".into()))
    }

    /// Give `oid` (HEAD or a commit HEAD descends from) a new message. The commits after it are
    /// re-created with the same trees, authors and author dates, and HEAD's branch moves to the
    /// new tip. The index and work tree are left alone (every tree is unchanged).
    pub fn reword(&self, oid: &str, message: &str) -> Result<Reworded> {
        let message = message.trim_end();
        if message.trim().is_empty() {
            return Err(GitError::Arg("the commit message is empty".into()));
        }
        if message.len() > crate::MAX_ARG {
            return Err(GitError::Arg("the commit message is too long".into()));
        }
        let oid = crate::check_oid(oid)?;
        let target = self.resolve(oid).ok_or_else(|| GitError::Arg(format!("no commit `{oid}`")))?;
        let head = self.resolve("HEAD").ok_or_else(|| GitError::Arg("there is no commit to edit".into()))?;
        if self.state() != crate::RepoState::Clean {
            return Err(GitError::Arg("finish or abort the operation in progress first".into()));
        }
        if !self.run(&["merge-base", "--is-ancestor", target.as_str(), head.as_str()])?.ok() {
            return Err(GitError::Arg("only commits on the checked-out branch can be edited".into()));
        }
        let range = format!("{target}..{head}");
        let list = self.text(&["rev-list", "--topo-order", "--reverse", "--ancestry-path", range.as_str()])?;
        let after_target: Vec<&str> = list.split_whitespace().collect();
        if after_target.len() > MAX_REWRITE {
            return Err(GitError::Arg(format!("the commit has more than {MAX_REWRITE} commits after it")));
        }
        let mut map = std::collections::HashMap::new();
        let new_target = self.recommit(&target, Some(message), &map)?;
        map.insert(target.clone(), new_target.clone());
        for c in after_target {
            let new = self.recommit(c, None, &map)?;
            map.insert(c.to_string(), new);
        }
        let after = map.get(&head).cloned().unwrap_or_else(|| new_target.clone());
        self.run_ok(&["update-ref", "-m", "gitcraft: edit commit message", "HEAD", after.as_str(), head.as_str()])?;
        Ok(Reworded { before: head, after, commit: new_target })
    }

    /// A copy of commit `oid` with its parents mapped through `map` and, when given, a new message.
    fn recommit(&self, oid: &str, message: Option<&str>, map: &std::collections::HashMap<String, String>) -> Result<String> {
        let oid = crate::check_oid(oid)?;
        let raw = self.run_ok(&["cat-file", "commit", oid])?.stdout;
        let split = raw.windows(2).position(|w| w == b"\n\n");
        let (head, body) = match split {
            Some(i) => (raw.get(..i).unwrap_or_default(), raw.get(i.saturating_add(2)..).unwrap_or_default()),
            None => (raw.as_slice(), &[][..]),
        };
        let header = String::from_utf8_lossy(head);
        let (mut tree, mut parents, mut author, mut encoding) = (None, Vec::new(), None, None);
        for line in header.lines() {
            if let Some(v) = line.strip_prefix("tree ") {
                tree = Some(crate::check_oid(v)?.to_string());
            } else if let Some(v) = line.strip_prefix("parent ") {
                let p = crate::check_oid(v)?;
                parents.push(map.get(p).cloned().unwrap_or_else(|| p.to_string()));
            } else if let Some(v) = line.strip_prefix("author ") {
                author = parse_ident(v);
            } else if let Some(v) = line.strip_prefix("encoding ") {
                encoding = Some(check_name("encoding", v)?.to_string());
            }
        }
        let tree = tree.ok_or_else(|| GitError::Parse(format!("commit {oid} has no tree")))?;
        let mut args: Vec<String> = Vec::new();
        // A new message is UTF-8; an old one keeps the encoding it was written in.
        if let (Some(e), None) = (&encoding, message) {
            args.push("-c".into());
            args.push(format!("i18n.commitEncoding={e}"));
        }
        args.push("commit-tree".into());
        args.push(tree);
        for p in parents {
            args.push("-p".into());
            args.push(p);
        }
        let mut git = self.git.clone();
        if let Some((name, email, date)) = author {
            git.env.push(("GIT_AUTHOR_NAME".into(), name));
            git.env.push(("GIT_AUTHOR_EMAIL".into(), email));
            git.env.push(("GIT_AUTHOR_DATE".into(), date));
        }
        let input = match message {
            Some(m) => format!("{m}\n").into_bytes(),
            None => body.to_vec(),
        };
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = git.run_ok(Some(&self.root), &argv, Some(&input))?.text();
        Ok(crate::check_oid(out.trim())?.to_string())
    }

    // ---- branches ----

    pub fn create_branch(&self, name: &str, start: Option<&str>, checkout: bool) -> Result<String> {
        let name = self.check_branch_name(name)?;
        let start = start.map(|s| check_name("start point", s)).transpose()?;
        let mut args = if checkout { vec!["switch", "-q", "-c", name.as_str()] } else { vec!["branch", name.as_str()] };
        if let Some(s) = start {
            args.push(s);
        }
        self.run_ok(&args)?;
        Ok(name)
    }

    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        let name = check_name("branch", name)?;
        self.run_ok(&["switch", "-q", name]).map(|_| ())
    }

    /// Check out a remote branch (`origin/feature`): its local branch, created to track it when missing.
    pub fn checkout_remote(&self, remote_branch: &str) -> Result<String> {
        let rb = check_name("remote branch", remote_branch)?;
        let Some((_, local)) = rb.split_once('/') else { return Err(GitError::Arg(format!("`{rb}` is not a remote branch"))) };
        let local = local.to_string();
        if self.resolve(&format!("refs/heads/{local}")).is_some() {
            self.checkout_branch(&local)?;
        } else {
            self.run_ok(&["switch", "-q", "-c", local.as_str(), "--track", rb])?;
        }
        Ok(local)
    }

    /// Check out a commit with no branch (detached HEAD).
    pub fn checkout_detached(&self, rev: &str) -> Result<()> {
        let rev = check_name("revision", rev)?;
        self.run_ok(&["switch", "-q", "--detach", rev]).map(|_| ())
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        let name = check_name("branch", name)?;
        self.run_ok(&["branch", if force { "-D" } else { "-d" }, name]).map(|_| ())
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<String> {
        let from = check_name("branch", from)?;
        let to = self.check_branch_name(to)?;
        self.run_ok(&["branch", "-m", from, to.as_str()])?;
        Ok(to)
    }

    pub fn set_upstream(&self, branch: &str, upstream: &str) -> Result<()> {
        let branch = check_name("branch", branch)?;
        let up = format!("--set-upstream-to={}", check_name("upstream", upstream)?);
        self.run_ok(&["branch", up.as_str(), branch]).map(|_| ())
    }

    // ---- merge, rebase, cherry-pick, revert, reset ----

    pub fn merge(&self, rev: &str, ff_only: bool, no_ff: bool) -> Result<Outcome> {
        let rev = check_name("revision", rev)?;
        let mut args = vec!["merge", "--no-edit"];
        if ff_only {
            args.push("--ff-only");
        } else if no_ff {
            args.push("--no-ff");
        }
        args.push(rev);
        self.conflicting(&args)
    }

    /// Whether `ancestor` is `descendant` or one of its ancestors.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        let (Ok(a), Ok(d)) = (check_name("revision", ancestor), check_name("revision", descendant)) else { return false };
        self.run(&["merge-base", "--is-ancestor", a, d]).is_ok_and(|o| o.ok())
    }

    pub fn merge_abort(&self) -> Result<()> {
        self.run_ok(&["merge", "--abort"]).map(|_| ())
    }

    pub fn rebase(&self, onto: &str) -> Result<Outcome> {
        let onto = check_name("revision", onto)?;
        self.conflicting(&["rebase", onto])
    }

    pub fn rebase_continue(&self) -> Result<Outcome> {
        self.conflicting(&["rebase", "--continue"])
    }

    pub fn rebase_skip(&self) -> Result<Outcome> {
        self.conflicting(&["rebase", "--skip"])
    }

    pub fn rebase_abort(&self) -> Result<()> {
        self.run_ok(&["rebase", "--abort"]).map(|_| ())
    }

    pub fn cherry_pick(&self, oid: &str) -> Result<Outcome> {
        let oid = check_name("commit", oid)?;
        let merge = self.find_commit(oid).map(|c| c.is_merge()).unwrap_or(false);
        let mut args = vec!["cherry-pick"];
        if merge {
            args.extend(["-m", "1"]);
        }
        args.push(oid);
        self.conflicting(&args)
    }

    pub fn revert(&self, oid: &str) -> Result<Outcome> {
        let oid = check_name("commit", oid)?;
        let merge = self.find_commit(oid).map(|c| c.is_merge()).unwrap_or(false);
        let mut args = vec!["revert", "--no-edit"];
        if merge {
            args.extend(["-m", "1"]);
        }
        args.push(oid);
        self.conflicting(&args)
    }

    /// Continue the operation in progress (merge, cherry-pick, revert or rebase) once its
    /// conflicts are resolved and staged.
    pub fn continue_operation(&self) -> Result<Outcome> {
        match self.state() {
            crate::RepoState::Rebasing => self.rebase_continue(),
            crate::RepoState::CherryPicking => self.conflicting(&["cherry-pick", "--continue"]),
            crate::RepoState::Reverting => self.conflicting(&["revert", "--continue"]),
            crate::RepoState::Merging => self.conflicting(&["commit", "--no-edit", "-q"]),
            _ => Err(GitError::Arg("no merge, rebase, cherry-pick or revert is in progress".into())),
        }
    }

    /// Abort the operation in progress.
    pub fn abort_operation(&self) -> Result<()> {
        let args: &[&str] = match self.state() {
            crate::RepoState::Rebasing => &["rebase", "--abort"],
            crate::RepoState::CherryPicking => &["cherry-pick", "--abort"],
            crate::RepoState::Reverting => &["revert", "--abort"],
            crate::RepoState::Merging => &["merge", "--abort"],
            _ => return Err(GitError::Arg("no merge, rebase, cherry-pick or revert is in progress".into())),
        };
        self.run_ok(args).map(|_| ())
    }

    pub fn reset(&self, rev: &str, mode: ResetMode) -> Result<()> {
        let rev = check_name("revision", rev)?;
        self.run_ok(&["reset", "-q", mode.flag(), rev]).map(|_| ())
    }

    // ---- tags ----

    /// A tag at `rev` (HEAD), annotated when there is a message.
    pub fn create_tag(&self, name: &str, rev: Option<&str>, message: Option<&str>) -> Result<String> {
        let name = self.check_tag_name(name)?;
        let rev = rev.map(|r| check_name("revision", r)).transpose()?.unwrap_or("HEAD");
        match message.filter(|m| !m.trim().is_empty()) {
            Some(m) => self.run_input(&["tag", "-a", "-F", "-", name.as_str(), rev], m.as_bytes())?,
            None => self.run_ok(&["tag", name.as_str(), rev])?,
        };
        Ok(name)
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        let name = check_name("tag", name)?;
        self.run_ok(&["tag", "-d", name]).map(|_| ())
    }

    // ---- remotes ----

    /// The remote to use when none is named: the current branch's upstream's, else `origin`,
    /// else the only one.
    pub fn default_remote(&self) -> Result<String> {
        let st = self.status()?;
        if let Some(up) = &st.upstream
            && let Some((r, _)) = up.split_once('/')
        {
            return Ok(r.to_string());
        }
        let remotes = self.remotes()?;
        if remotes.iter().any(|r| r.name == "origin") {
            return Ok("origin".into());
        }
        remotes.first().map(|r| r.name.clone()).ok_or_else(|| GitError::Arg("this repository has no remote".into()))
    }

    pub fn fetch(&self, remote: Option<&str>, prune: bool) -> Result<String> {
        let mut args = vec!["fetch", "--progress"];
        if prune {
            args.push("--prune");
        }
        match remote {
            Some(r) => args.push(check_name("remote", r)?),
            None => args.push("--all"),
        }
        Ok(self.run_ok(&args)?.message())
    }

    pub fn pull(&self, mode: PullMode) -> Result<Outcome> {
        let flag = match mode {
            PullMode::Merge => "--no-rebase",
            PullMode::FastForwardOnly => "--ff-only",
            PullMode::Rebase => "--rebase",
        };
        self.conflicting(&["pull", "--no-edit", flag])
    }

    /// Push `branch` (the current one) to `remote` (the default), setting it as upstream when
    /// asked or when the branch has none.
    pub fn push(&self, remote: Option<&str>, branch: Option<&str>, force_with_lease: bool, set_upstream: bool) -> Result<String> {
        let st = self.status()?;
        let branch = match branch {
            Some(b) => check_name("branch", b)?.to_string(),
            None => st.branch.clone().ok_or_else(|| GitError::Arg("HEAD is detached: check out a branch to push".into()))?,
        };
        let remote = match remote {
            Some(r) => check_name("remote", r)?.to_string(),
            None => self.default_remote()?,
        };
        let has_upstream = self.run(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", &format!("{branch}@{{upstream}}")])?.ok();
        let mut args = vec!["push", "--progress"];
        if set_upstream || !has_upstream {
            args.push("-u");
        }
        if force_with_lease {
            args.push("--force-with-lease");
        }
        args.extend([remote.as_str(), branch.as_str()]);
        Ok(self.run_ok(&args)?.message())
    }

    pub fn delete_remote_branch(&self, remote: &str, branch: &str) -> Result<String> {
        let (remote, branch) = (check_name("remote", remote)?, check_name("branch", branch)?);
        Ok(self.run_ok(&["push", remote, "--delete", branch])?.message())
    }

    pub fn push_tag(&self, remote: &str, tag: &str) -> Result<String> {
        let remote = check_name("remote", remote)?;
        let r = format!("refs/tags/{}", check_name("tag", tag)?);
        Ok(self.run_ok(&["push", remote, r.as_str()])?.message())
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        let (name, url) = (check_name("remote name", name)?, check_name("url", url)?);
        self.run_ok(&["remote", "add", name, url]).map(|_| ())
    }

    pub fn remove_remote(&self, name: &str) -> Result<()> {
        let name = check_name("remote", name)?;
        self.run_ok(&["remote", "remove", name]).map(|_| ())
    }

    // ---- stashes ----

    pub fn stash_save(&self, message: Option<&str>, include_untracked: bool) -> Result<()> {
        let mut args = vec!["stash", "push", "-q"];
        if include_untracked {
            args.push("--include-untracked");
        }
        let m;
        if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
            m = check_name("stash message", msg)?.to_string();
            args.extend(["-m", m.as_str()]);
        }
        let before = self.resolve("refs/stash");
        self.run_ok(&args)?;
        if self.resolve("refs/stash") == before {
            return Err(GitError::Arg("there are no changes to stash".into()));
        }
        Ok(())
    }

    pub fn stash_apply(&self, index: usize) -> Result<Outcome> {
        let r = format!("stash@{{{index}}}");
        self.conflicting(&["stash", "apply", r.as_str()])
    }

    pub fn stash_pop(&self, index: usize) -> Result<Outcome> {
        let r = format!("stash@{{{index}}}");
        self.conflicting(&["stash", "pop", r.as_str()])
    }

    pub fn stash_drop(&self, index: usize) -> Result<()> {
        let r = format!("stash@{{{index}}}");
        self.run_ok(&["stash", "drop", "-q", r.as_str()]).map(|_| ())
    }

    // ---- conflicts ----

    /// Resolve a conflicted file by taking our side or theirs whole, and stage it.
    pub fn resolve_with(&self, path: &str, ours: bool) -> Result<()> {
        let path = check_path(path)?;
        self.run_paths(&["checkout", if ours { "--ours" } else { "--theirs" }, "--", path])?;
        self.run_paths(&["add", "--", path]).map(|_| ())
    }

    /// Mark a conflicted file resolved as it is in the work tree.
    pub fn mark_resolved(&self, path: &str) -> Result<()> {
        let path = check_path(path)?;
        self.run_paths(&["add", "--", path]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_ident;
    use crate::testing::{commit_all, repo, sample, write};
    use crate::{Change, LogQuery, RefKind, RepoState, patch::Target};

    #[test]
    fn reword_head_and_older_commits() {
        let (_d, r) = repo("ops-reword").unwrap();
        write(&r, "a.txt", "one\n").unwrap();
        let first = commit_all(&r, "First").unwrap();
        write(&r, "a.txt", "two\n").unwrap();
        commit_all(&r, "Second").unwrap();
        write(&r, "a.txt", "three\n").unwrap();
        let third = commit_all(&r, "Third").unwrap();
        // An uncommitted change and a staged one survive.
        write(&r, "a.txt", "four\n").unwrap();
        write(&r, "b.txt", "new\n").unwrap();
        r.stage(&["b.txt".into()]).unwrap();
        let before = r.status().unwrap();

        let w = r.reword(&third, "Third, renamed\n\nWith a body\n").unwrap();
        assert_eq!(w.before, third);
        assert_eq!(w.after, w.commit);
        let c = r.find_commit(&w.after).unwrap();
        assert_eq!((c.summary.as_str(), c.body.as_str()), ("Third, renamed", "With a body"));

        let w2 = r.reword(&first, "First, renamed").unwrap();
        let log = r.log(&LogQuery::default()).unwrap();
        let summaries: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["Third, renamed", "Second", "First, renamed"]);
        assert_eq!(log[2].oid, w2.commit);
        assert_eq!(log[0].author.time, c.author.time);
        assert_eq!(r.text(&["rev-parse", "HEAD^{tree}"]).unwrap(), r.text(&["rev-parse", &format!("{third}^{{tree}}")]).unwrap());
        assert_eq!(r.status().unwrap().files, before.files);

        assert!(r.reword(&first, "  ").is_err());
        assert!(r.reword("-x", "m").is_err());
        assert!(r.reword("deadbeef", "m").is_err());
        assert_eq!(parse_ident("Ada <a@b> 1 +0000"), Some(("Ada".into(), "a@b".into(), "1 +0000".into())));
        for junk in ["", "<", ">", "a>b<", "é<é>é"] {
            let _ = parse_ident(junk);
        }
    }

    #[test]
    fn stage_commit_and_history() {
        let (_d, r) = repo("ops-commit").unwrap();
        assert!(!r.has_commits());
        assert!(r.log(&LogQuery::default()).unwrap().is_empty());
        write(&r, "a.txt", "one\n").unwrap();
        let st = r.status().unwrap();
        assert_eq!(st.files.len(), 1);
        assert!(st.files[0].is_untracked());
        r.stage(&["a.txt".into()]).unwrap();
        assert!(r.status().unwrap().files[0].is_staged());
        r.unstage(&["a.txt".into()]).unwrap();
        assert!(r.status().unwrap().files[0].is_untracked());
        r.stage_all().unwrap();
        let oid = r.commit("First\n\nBody text", false).unwrap();
        let c = r.find_commit(&oid).unwrap();
        assert_eq!(c.summary, "First");
        assert_eq!(c.body, "Body text");
        assert_eq!(c.author.name, "Ada Example");
        assert!(r.status().unwrap().is_clean());
        write(&r, "a.txt", "one\ntwo\n").unwrap();
        commit_all(&r, "Second").unwrap();
        let amended = r.commit("Second, amended", true).unwrap();
        let log = r.log(&LogQuery::default()).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].oid, amended);
        assert_eq!(log[0].parents, vec![oid.clone()]);
        assert!(r.commit("   ", false).is_err());
        let files = r.commit_files(&amended).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].change, Change::Modified);
        let root_files = r.commit_files(&oid).unwrap();
        assert_eq!(root_files[0].change, Change::Added);
        let d = r.diff_commit_file(&amended, "a.txt", 3).unwrap();
        assert_eq!((d.added(), d.deleted()), (1, 0));
        let d = r.diff_commit_file(&oid, "a.txt", 3).unwrap();
        assert_eq!(d.added(), 1);
    }

    #[test]
    fn branches_tags_and_merge() {
        let (_d, r) = sample("ops-branch").unwrap();
        let refs = r.refs().unwrap();
        let locals: Vec<&str> = refs.iter().filter(|x| x.kind == RefKind::Local).map(|x| x.name.as_str()).collect();
        assert_eq!(locals, vec!["feature", "main"]);
        assert!(refs.iter().any(|x| x.kind == RefKind::Tag && x.name == "v0.1.0"));
        assert!(refs.iter().find(|x| x.name == "main").unwrap().head);
        assert!(r.create_branch("bad..name", None, false).is_err());
        assert!(r.create_branch("-x", None, false).is_err());
        r.create_branch("topic", Some("v0.1.0"), false).unwrap();
        r.rename_branch("topic", "topic2").unwrap();
        r.delete_branch("topic2", false).unwrap();
        let out = r.merge("feature", true, false).unwrap();
        assert!(!out.conflicts);
        assert!(r.root().join("src/lib.rs").exists());
        r.create_tag("v0.2.0", None, Some("Release 0.2")).unwrap();
        let tag = r.refs().unwrap().into_iter().find(|x| x.name == "v0.2.0").unwrap();
        assert_ne!(tag.target, tag.commit);
        r.delete_tag("v0.2.0").unwrap();
        r.checkout_detached("v0.1.0").unwrap();
        assert_eq!(r.status().unwrap().branch, None);
        r.checkout_branch("main").unwrap();
        r.reset("HEAD~1", crate::ResetMode::Hard).unwrap();
        assert!(!r.root().join("src/lib.rs").exists());
    }

    #[test]
    fn conflicts_continue_and_abort() {
        let (_d, r) = repo("ops-conflict").unwrap();
        write(&r, "f.txt", "base\n").unwrap();
        commit_all(&r, "base").unwrap();
        r.create_branch("other", None, true).unwrap();
        write(&r, "f.txt", "theirs\n").unwrap();
        commit_all(&r, "theirs").unwrap();
        r.checkout_branch("main").unwrap();
        write(&r, "f.txt", "ours\n").unwrap();
        commit_all(&r, "ours").unwrap();
        let out = r.merge("other", false, false).unwrap();
        assert!(out.conflicts);
        assert_eq!(r.state(), RepoState::Merging);
        assert_eq!(r.status().unwrap().conflicted().count(), 1);
        r.abort_operation().unwrap();
        assert_eq!(r.state(), RepoState::Clean);
        let out = r.merge("other", false, false).unwrap();
        assert!(out.conflicts);
        r.resolve_with("f.txt", false).unwrap();
        assert_eq!(std::fs::read_to_string(r.root().join("f.txt")).unwrap(), "theirs\n");
        assert!(r.prepared_message().unwrap().starts_with("Merge branch 'other'"));
        r.continue_operation().unwrap();
        assert_eq!(r.state(), RepoState::Clean);
        assert!(r.log(&LogQuery::default()).unwrap()[0].is_merge());
        // A failure that leaves no conflict is an error.
        assert!(r.merge("no-such-branch", false, false).is_err());
    }

    #[test]
    fn stash_round_trip() {
        let (_d, r) = sample("ops-stash").unwrap();
        assert!(r.stash_save(None, false).is_err());
        write(&r, "README.md", "changed\n").unwrap();
        write(&r, "new.txt", "untracked\n").unwrap();
        r.stash_save(Some("my work"), true).unwrap();
        assert!(r.status().unwrap().is_clean());
        let s = r.stashes().unwrap();
        assert_eq!(s.len(), 1);
        assert!(s[0].message.contains("my work"));
        r.stash_apply(0).unwrap();
        assert!(!r.status().unwrap().is_clean());
        r.discard_all().unwrap();
        assert!(r.status().unwrap().is_clean());
        r.stash_pop(0).unwrap();
        assert!(r.stashes().unwrap().is_empty());
        assert!(r.root().join("new.txt").exists());
        r.discard(&["new.txt".into(), "README.md".into()]).unwrap();
        assert!(r.status().unwrap().is_clean());
    }

    #[test]
    fn paths_are_literal() {
        // `:x` and `*` would be pathspec magic and a glob; they must name exactly these files.
        let (_d, r) = repo("ops-literal").unwrap();
        write(&r, ":x", "1\n").unwrap();
        write(&r, "*", "2\n").unwrap();
        write(&r, "other.txt", "3\n").unwrap();
        r.stage(&[":x".into()]).unwrap();
        r.stage(&["*".into()]).unwrap();
        let st = r.status().unwrap();
        assert_eq!(st.staged().count(), 2);
        assert!(st.file("other.txt").unwrap().is_untracked());
        assert_eq!(r.diff_staged(":x", 3).unwrap().added(), 1);
    }

    #[test]
    fn partial_staging() {
        let (_d, r) = repo("ops-partial").unwrap();
        let base: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        write(&r, "f.txt", &base).unwrap();
        commit_all(&r, "base").unwrap();
        let changed = base.replace("line 2\n", "line two\n").replace("line 18\n", "line eighteen\n");
        write(&r, "f.txt", &changed).unwrap();
        let d = r.diff_unstaged("f.txt", 3).unwrap();
        assert_eq!(d.hunks.len(), 2);
        r.apply_partial(&d, 1, None, Target::Stage).unwrap();
        let staged = r.diff_staged("f.txt", 3).unwrap();
        assert_eq!(staged.hunks.len(), 1);
        assert!(staged.hunks[0].lines.iter().any(|l| l.text == "line eighteen"));
        assert_eq!(r.diff_unstaged("f.txt", 3).unwrap().hunks.len(), 1);
        // Unstage it again, then stage only the addition of the first hunk.
        r.apply_partial(&staged, 0, None, Target::Unstage).unwrap();
        assert!(r.diff_staged("f.txt", 3).unwrap().hunks.is_empty());
        let d = r.diff_unstaged("f.txt", 3).unwrap();
        let add = d.hunks[0].lines.iter().position(|l| l.text == "line two").unwrap();
        r.apply_partial(&d, 0, Some(&[add]), Target::Stage).unwrap();
        let staged = r.diff_staged("f.txt", 3).unwrap();
        assert_eq!((staged.added(), staged.deleted()), (1, 0));
        // Discard the second hunk from the work tree.
        let d = r.diff_unstaged("f.txt", 3).unwrap();
        let last = d.hunks.len() - 1;
        r.apply_partial(&d, last, None, Target::Discard).unwrap();
        let now = std::fs::read_to_string(r.root().join("f.txt")).unwrap();
        assert!(now.contains("line 18\n") && now.contains("line two\n"));
        // An untracked file diffs as all added.
        write(&r, "new.txt", "a\nb\n").unwrap();
        assert_eq!(r.diff_unstaged("new.txt", 3).unwrap().added(), 2);
    }

    #[test]
    fn remotes_fetch_pull_push() {
        let (d, upstream) = sample("ops-remote").unwrap();
        let git = upstream.git().clone();
        let clone = git.clone_repo(&upstream.root().to_string_lossy(), &d.path().join("clone")).unwrap();
        assert_eq!(clone.remotes().unwrap()[0].name, "origin");
        assert!(clone.refs().unwrap().iter().any(|x| x.name == "origin/feature"));
        let local = clone.checkout_remote("origin/feature").unwrap();
        assert_eq!(local, "feature");
        assert_eq!(clone.status().unwrap().upstream.as_deref(), Some("origin/feature"));
        clone.checkout_branch("main").unwrap();
        // New work upstream, then fetch and pull it.
        write(&upstream, "UP.md", "upstream\n").unwrap();
        commit_all(&upstream, "Upstream work").unwrap();
        clone.fetch(None, true).unwrap();
        assert_eq!(clone.status().unwrap().behind, 1);
        clone.pull(crate::PullMode::FastForwardOnly).unwrap();
        assert!(clone.root().join("UP.md").exists());
        // Push a new branch: it gets an upstream.
        clone.create_branch("pushed", None, true).unwrap();
        write(&clone, "P.md", "p\n").unwrap();
        commit_all(&clone, "Pushed work").unwrap();
        clone.push(None, None, false, false).unwrap();
        assert!(upstream.resolve("refs/heads/pushed").is_some());
        assert_eq!(clone.status().unwrap().upstream.as_deref(), Some("origin/pushed"));
        clone.delete_remote_branch("origin", "pushed").unwrap();
        assert!(upstream.resolve("refs/heads/pushed").is_none());
        clone.add_remote("second", "/nowhere").unwrap();
        assert_eq!(clone.remotes().unwrap().len(), 2);
        clone.remove_remote("second").unwrap();
    }

    #[test]
    fn cherry_pick_revert_blame_history() {
        let (_d, r) = sample("ops-pick").unwrap();
        let feature = r.resolve("feature").unwrap();
        r.cherry_pick(&feature).unwrap();
        assert!(r.root().join("src/lib.rs").exists());
        let head = r.resolve("HEAD").unwrap();
        r.revert(&head).unwrap();
        assert!(!r.root().join("src/lib.rs").exists());
        let b = r.blame("src/main.rs", None).unwrap();
        assert_eq!(b.lines.len(), 3);
        assert_eq!(b.commits[&b.lines[1].oid].summary, "Greet the world");
        let hist = r.log(&LogQuery { revs: vec!["HEAD".into()], path: Some("src/main.rs".into()), follow: true, ..LogQuery::default() }).unwrap();
        assert_eq!(hist.len(), 2);
        let found = r.log(&LogQuery { grep: Some("greet".into()), ..LogQuery::default() }).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(r.show_file("HEAD", "README.md").unwrap(), b"# Sample\n\nA repository for GitCraft.\n");
    }
}
