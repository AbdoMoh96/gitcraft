//! Command implementations, one module per area. Each module exposes `specs()`.

pub mod branch;
pub mod commit;
pub mod conflict;
pub mod edit;
pub mod inspect;
pub mod merge;
pub mod remote;
pub mod repo;
pub mod stage;
pub mod stash;
pub mod tag;
pub mod view;

use gitcraft_git::{Outcome, RepoState};
use serde_json::{Value, json};

use crate::{CmdResult, Registry, Session};

/// The full registry.
pub fn registry() -> Registry {
    let mut v = Vec::new();
    v.extend(repo::specs());
    v.extend(edit::specs());
    v.extend(view::specs());
    v.extend(stage::specs());
    v.extend(commit::specs());
    v.extend(branch::specs());
    v.extend(merge::specs());
    v.extend(tag::specs());
    v.extend(remote::specs());
    v.extend(stash::specs());
    v.extend(conflict::specs());
    v.extend(inspect::specs());
    Registry::new(v)
}

/// A shortcut that differs on macOS: `mac` there, `other` everywhere else.
pub const fn mac_or(mac: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") { mac } else { other }
}

/// The standard result of a command that did something: what happened, in a sentence.
pub fn done(message: impl Into<String>) -> CmdResult {
    Ok(json!({"message": message.into()}))
}

/// The result of an operation that may stop on conflicts.
pub fn outcome(what: &str, o: &Outcome) -> CmdResult {
    let message = if o.conflicts { format!("{what} stopped with conflicts: resolve them, then continue or abort") } else { what.to_string() };
    Ok(json!({"message": message, "conflicts": o.conflicts, "git": o.message}))
}

// ---- when commands are enabled ----

pub fn need_repo(s: &Session) -> Option<&'static str> {
    s.repo.is_none().then_some("no repository is open")
}

pub fn need_commits(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.status.head.is_none().then_some("the repository has no commits yet"))
}

pub fn need_changes(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.status.is_clean().then_some("there are no changes"))
}

pub fn need_unstaged(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.status.unstaged().next().is_none().then_some("there are no unstaged changes"))
}

pub fn need_staged(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.status.staged().next().is_none().then_some("there are no staged changes"))
}

pub fn need_operation(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| (s.data.status.state == RepoState::Clean).then_some("no merge, rebase, cherry-pick or revert is in progress"))
}

pub fn need_remote(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.remotes.is_empty().then_some("the repository has no remote"))
}

pub fn need_branch(s: &Session) -> Option<&'static str> {
    need_commits(s).or_else(|| s.data.status.branch.is_none().then_some("HEAD is detached"))
}

pub fn need_stash(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.stashes.is_empty().then_some("there are no stashes"))
}

/// The commit a command acts on: `oid` (or `ref`) from params, else the selected commit.
pub fn target_commit(s: &Session, v: &Value) -> Result<String, crate::CmdError> {
    if let Some(r) = crate::p::str(v, "oid").or_else(|| crate::p::str(v, "ref")).filter(|r| !r.trim().is_empty()) {
        return Ok(r.trim().to_string());
    }
    match &s.sel.row {
        crate::Selected::Commit(oid) => Ok(oid.clone()),
        _ => Err(crate::CmdError::Params("`oid` is required (or select a commit)".into())),
    }
}
