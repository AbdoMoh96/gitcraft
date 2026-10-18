//! Resolving conflicts: take one side, mark resolved, and the three versions for the merge tool.

use serde_json::{Value, json};

use super::{done, need_repo};
use crate::git::conflict;
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn need_conflicts(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.status.conflicted().next().is_none().then_some("there are no conflicts"))
}

fn versions(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    if path.split(['/', '\\']).any(|c| c == "..") || std::path::Path::new(path).is_absolute() {
        return Err(CmdError::Params("`path` is relative to the repository".into()));
    }
    let repo = s.repo()?;
    let stage = |n: &str| repo.show_file(n, path).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    let worktree = std::fs::read(repo.root().join(path)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    // The merge tool's view: merged again from the stages when git can, else the work tree copy.
    let merged = match repo.conflict_text(path) {
        Ok(Some(t)) => Some(t),
        _ => worktree.clone().filter(|w| w.len() <= conflict::MAX_BYTES),
    };
    let chunks = merged.as_deref().map(conflict::parse).unwrap_or_default();
    Ok(json!({
        "path": path,
        "base": stage(":1"),
        "ours": stage(":2"),
        "theirs": stage(":3"),
        "worktree": worktree,
        "chunks": chunks,
        "mergeable": conflict::has_conflicts(&chunks),
    }))
}

/// Resolve a file with the merge tool's output and stage it.
fn edit(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    let content = p::req_str(v, "content")?;
    if !s.data.status.conflicted().any(|f| f.path == path) {
        return Err(CmdError::Params(format!("`{path}` is not in conflict")));
    }
    s.repo()?.resolve_content(path, content.as_bytes())?;
    done(format!("Resolved {path}"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("conflict.takeOurs", "Take Current (Ours)", "Merge Tool", |s, v| {
            let path = p::req_str(v, "path")?;
            s.repo()?.resolve_with(path, true)?;
            done(format!("Resolved {path} with our version"))
        })
        .params(r#"{"path": string}"#)
        .when(need_conflicts),
        CommandSpec::new("conflict.takeTheirs", "Take Incoming (Theirs)", "Merge Tool", |s, v| {
            let path = p::req_str(v, "path")?;
            s.repo()?.resolve_with(path, false)?;
            done(format!("Resolved {path} with their version"))
        })
        .params(r#"{"path": string}"#)
        .when(need_conflicts),
        CommandSpec::new("conflict.markResolved", "Mark Resolved", "Merge Tool", |s, v| {
            let path = p::req_str(v, "path")?;
            s.repo()?.mark_resolved(path)?;
            done(format!("Marked {path} resolved"))
        })
        .params(r#"{"path": string}"#)
        .when(need_conflicts),
        CommandSpec::new("conflict.edit", "Save Merge Output", "Merge Tool", edit)
            .params(r#"{"path": string, "content": string}"#)
            .when(need_conflicts),
        CommandSpec::new("conflict.versions", "Conflict Versions", "Merge Tool", versions).params(r#"{"path": string}"#).pure().when(need_repo),
    ]
}
