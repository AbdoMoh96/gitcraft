//! Resolving conflicts: take one side, mark resolved, and the three versions for the merge tool.

use serde_json::{Value, json};

use super::{done, need_repo};
use crate::git::{RefKind, conflict};
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

/// The branch to check the current one against: `origin/HEAD`'s, else main or master.
pub fn default_target(s: &Session) -> Option<String> {
    let repo = s.repo.as_ref()?;
    let known = |n: &str| s.data.refs.iter().any(|r| matches!(r.kind, RefKind::Local | RefKind::Remote) && r.name == n);
    repo.remote_head("origin")
        .filter(|n| known(n))
        .or_else(|| ["origin/main", "origin/master", "main", "master"].into_iter().find(|n| known(n)).map(str::to_string))
}

/// Would merging bring conflicts? Checks the current branch against its upstream (when behind)
/// and the target branch (`target`, default [`default_target`]), in memory.
fn predict(s: &mut Session, v: &Value) -> CmdResult {
    let st = &s.data.status;
    let Some(head) = st.head.clone() else {
        return Ok(json!({"checks": [], "supported": true}));
    };
    let branch = st.branch.clone();
    let mut targets: Vec<(String, &str)> = Vec::new();
    if let Some(up) = st.upstream.clone().filter(|_| st.behind > 0) {
        targets.push((up, "upstream"));
    }
    let target = match v.get("target").and_then(Value::as_str) {
        Some(t) => Some(t.to_string()),
        None => default_target(s),
    };
    if let Some(t) = target
        && Some(&t) != branch.as_ref()
        && !targets.iter().any(|(x, _)| *x == t)
    {
        targets.push((t, "target"));
    }
    let repo = s.repo()?;
    let mut checks = Vec::new();
    let mut supported = true;
    for (t, why) in targets {
        match repo.merge_conflicts(&head, &t)? {
            Some(files) => checks.push(json!({"target": t, "why": why, "conflicts": !files.is_empty(), "files": files})),
            None => supported = false,
        }
    }
    let conflicts = checks.iter().any(|c| c["conflicts"] == json!(true));
    Ok(json!({"branch": branch, "checks": checks, "conflicts": conflicts, "supported": supported}))
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
        CommandSpec::new("conflict.predict", "Check for Merge Conflicts", "Toolbar", predict).params(r#"{"target"?: string}"#).pure().when(need_repo),
        CommandSpec::new("conflict.versions", "Conflict Versions", "Merge Tool", versions).params(r#"{"path": string}"#).pure().when(need_repo),
    ]
}
