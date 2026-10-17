//! Branches: create, check out, delete, rename, upstreams; checking out a commit.

use gitcraft_git::RefKind;
use serde_json::{Value, json};

use super::edit::{Head, UndoStep};
use super::{done, need_branch, need_commits, need_repo, target_commit};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let from = p::str(v, "from").filter(|f| !f.trim().is_empty());
    let checkout = p::bool(v, "checkout").unwrap_or(true);
    let before = s.head();
    let repo = s.repo()?.clone();
    let name = repo.create_branch(name, from, checkout)?;
    let oid = repo.resolve(&format!("refs/heads/{name}")).unwrap_or_default();
    let created = UndoStep::BranchCreate { name: name.clone(), oid };
    match (checkout, before) {
        (true, Some(from)) => {
            s.push_undo(UndoStep::Many(format!("Create branch {name}"), vec![created, UndoStep::Checkout { from, to: Head::Branch(name.clone()) }]));
        }
        _ => s.push_undo(created),
    }
    Ok(json!({"message": format!("Created branch {name}"), "name": name}))
}

/// Check out a local branch, or a remote branch through its local branch (created to track it).
fn checkout(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    let before = s.head();
    let repo = s.repo()?.clone();
    let is_local = s.data.refs.iter().any(|r| r.kind == RefKind::Local && r.name == name);
    let is_remote = s.data.refs.iter().any(|r| r.kind == RefKind::Remote && r.name == name);
    let local = if is_local || !is_remote {
        repo.checkout_branch(&name)?;
        name
    } else {
        let had_local = name.split_once('/').is_some_and(|(_, b)| repo.resolve(&format!("refs/heads/{b}")).is_some());
        let local = repo.checkout_remote(&name)?;
        if !had_local {
            let oid = repo.resolve(&format!("refs/heads/{local}")).unwrap_or_default();
            if let Some(from) = before.clone() {
                s.push_undo(UndoStep::Many(
                    format!("Checkout {local}"),
                    vec![UndoStep::BranchCreate { name: local.clone(), oid }, UndoStep::Checkout { from, to: Head::Branch(local.clone()) }],
                ));
            }
            return Ok(json!({"message": format!("Checked out {local}, tracking {name}"), "name": local}));
        }
        local
    };
    if let Some(from) = before {
        s.push_undo(UndoStep::Checkout { from, to: Head::Branch(local.clone()) });
    }
    Ok(json!({"message": format!("Checked out {local}"), "name": local}))
}

fn checkout_commit(s: &mut Session, v: &Value) -> CmdResult {
    let rev = target_commit(s, v)?;
    let before = s.head();
    let repo = s.repo()?.clone();
    let oid = repo.resolve(&rev).ok_or_else(|| CmdError::Params(format!("no commit `{rev}`")))?;
    repo.checkout_detached(&oid)?;
    if let Some(from) = before {
        s.push_undo(UndoStep::Checkout { from, to: Head::Detached(oid.clone()) });
    }
    Ok(json!({"message": format!("Checked out {} (detached HEAD)", oid.get(..7).unwrap_or(&oid)), "oid": oid}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    let force = p::bool(v, "force").unwrap_or(false);
    let repo = s.repo()?.clone();
    let oid = repo.resolve(&format!("refs/heads/{name}")).ok_or_else(|| CmdError::Params(format!("no local branch `{name}`")))?;
    repo.delete_branch(&name, force)?;
    s.push_undo(UndoStep::BranchDelete { name: name.clone(), oid });
    done(format!("Deleted branch {name}"))
}

fn rename(s: &mut Session, v: &Value) -> CmdResult {
    let from = match p::str(v, "from").filter(|f| !f.trim().is_empty()) {
        Some(f) => f.trim().to_string(),
        None => s.data.status.branch.clone().ok_or_else(|| CmdError::Params("`from` is required (HEAD is detached)".into()))?,
    };
    let to = p::req_str(v, "to")?;
    let to = s.repo()?.rename_branch(&from, to)?;
    s.push_undo(UndoStep::BranchRename { from: from.clone(), to: to.clone() });
    Ok(json!({"message": format!("Renamed {from} to {to}"), "name": to}))
}

fn set_upstream(s: &mut Session, v: &Value) -> CmdResult {
    let branch = match p::str(v, "name").filter(|f| !f.trim().is_empty()) {
        Some(b) => b.trim().to_string(),
        None => s.data.status.branch.clone().ok_or_else(|| CmdError::Params("`name` is required (HEAD is detached)".into()))?,
    };
    let up = p::req_str(v, "upstream")?;
    s.repo()?.set_upstream(&branch, up)?;
    done(format!("{branch} now tracks {up}"))
}

fn delete_remote(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim();
    let (remote, branch) = match (p::str(v, "remote"), name.split_once('/')) {
        (Some(r), _) => (r.to_string(), name.to_string()),
        (None, Some((r, b))) => (r.to_string(), b.to_string()),
        (None, None) => return Err(CmdError::Params("`name` is `remote/branch` (or pass `remote`)".into())),
    };
    s.repo()?.delete_remote_branch(&remote, &branch)?;
    done(format!("Deleted {branch} from {remote}"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("branch.create", "Branch", "Toolbar", create)
            .key("Mod+B")
            .params(r#"{"name": string, "from"?: ref or commit (default HEAD), "checkout"?: bool (default true)}"#)
            .when(need_commits),
        CommandSpec::new("branch.checkout", "Checkout", "Left Panel › Branches", checkout)
            .params(r#"{"name": "main" | "origin/feature"}"#)
            .when(need_repo),
        CommandSpec::new("commit.checkout", "Checkout This Commit", "Graph › Commit Menu", checkout_commit)
            .params(r#"{"oid"?: commit (default: the selected one)}"#)
            .when(need_commits),
        CommandSpec::new("branch.delete", "Delete Branch", "Left Panel › Branches", delete)
            .params(r#"{"name": string, "force"?: bool}"#)
            .when(need_commits),
        CommandSpec::new("branch.rename", "Rename Branch", "Left Panel › Branches", rename)
            .params(r#"{"from"?: string (default: current), "to": string}"#)
            .when(need_commits),
        CommandSpec::new("branch.setUpstream", "Set Upstream", "Left Panel › Branches", set_upstream)
            .params(r#"{"name"?: string (default: current), "upstream": "origin/main"}"#)
            .when(need_branch),
        CommandSpec::new("branch.deleteRemote", "Delete Remote Branch", "Left Panel › Remote", delete_remote)
            .params(r#"{"name": "origin/feature"} | {"remote": string, "name": string}"#)
            .slow()
            .when(super::need_remote),
    ]
}
