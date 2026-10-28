//! Submodules and linked worktrees.

use serde_json::{Value, json};

use super::{done, need_commits, need_repo};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn need_submodules(s: &Session) -> Option<&'static str> {
    need_repo(s).or_else(|| s.data.submodules.is_empty().then_some("the repository has no submodules"))
}

fn paths_opt(v: &Value) -> Vec<String> {
    p::paths(v).unwrap_or_default()
}

fn worktree_list(s: &mut Session, _: &Value) -> CmdResult {
    Ok(json!({"worktrees": s.repo()?.worktrees()?}))
}

fn worktree_add(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?.trim().to_string();
    let branch = p::str(v, "branch").map(str::trim).filter(|b| !b.is_empty());
    let new_branch = p::bool(v, "newBranch").unwrap_or(false);
    let from = p::str(v, "from").map(str::trim).filter(|b| !b.is_empty());
    let repo = s.repo()?;
    // A relative folder is next to the repository's.
    let full = {
        let pb = std::path::PathBuf::from(&path);
        if pb.is_absolute() { pb } else { repo.root().parent().unwrap_or(repo.root()).join(pb) }
    };
    let full_s = full.to_string_lossy().into_owned();
    repo.worktree_add(&full_s, branch, new_branch, from)?;
    Ok(json!({"message": format!("Added a work tree at {full_s}"), "path": full_s}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("submodule.list", "Submodules", "Left Panel › Submodules", |s, _| Ok(json!({"submodules": s.repo()?.submodules()?})))
            .pure()
            .when(need_repo),
        CommandSpec::new("submodule.update", "Update Submodules", "Left Panel › Submodules", |s, v| {
            let paths = paths_opt(v);
            let init = p::bool(v, "init").unwrap_or(true);
            let msg = s.repo()?.submodule_update(&paths, init, p::bool(v, "recursive").unwrap_or(true))?;
            Ok(json!({"message": if paths.is_empty() { "Updated the submodules".to_string() } else { format!("Updated {}", paths.join(", ")) }, "git": msg}))
        })
        .params(r#"{"path"?: string, "paths"?: [string] (default: all), "init"?: bool (true), "recursive"?: bool (true)}"#)
        .slow()
        .when(need_submodules),
        CommandSpec::new("submodule.add", "Add Submodule", "Left Panel › Submodules", |s, v| {
            let url = p::req_str(v, "url")?.trim().to_string();
            let path = p::req_str(v, "path")?.trim().to_string();
            let msg = s.repo()?.submodule_add(&url, &path)?;
            Ok(json!({"message": format!("Added the submodule {path} (staged)"), "git": msg}))
        })
        .params(r#"{"url": string, "path": folder in the repository}"#)
        .slow()
        .when(need_repo),
        CommandSpec::new("submodule.sync", "Sync Submodule URLs", "Left Panel › Submodules", |s, _| {
            s.repo()?.submodule_sync()?;
            done("Synchronized the submodule URLs")
        })
        .when(need_submodules),
        CommandSpec::new("submodule.remove", "Remove Submodule", "Left Panel › Submodules", |s, v| {
            let path = p::req_str(v, "path")?.trim().to_string();
            s.repo()?.submodule_remove(&path)?;
            done(format!("Removed the submodule {path} (staged)"))
        })
        .params(r#"{"path": string}"#)
        .when(need_submodules),
        CommandSpec::new("worktree.list", "Worktrees", "Left Panel › Worktrees", worktree_list).pure().when(need_repo),
        CommandSpec::new("worktree.add", "Add Worktree", "Left Panel › Worktrees", worktree_add)
            .params(r#"{"path": folder (relative: beside the repository), "branch"?: string, "newBranch"?: bool, "from"?: commit (for a new branch)}"#)
            .when(need_commits),
        CommandSpec::new("worktree.remove", "Remove Worktree", "Left Panel › Worktrees", |s, v| {
            let path = p::req_str(v, "path")?.trim().to_string();
            if s.repo()?.root().to_string_lossy() == path {
                return Err(CmdError::Params("this is the work tree that is open".into()));
            }
            s.repo()?.worktree_remove(&path, p::bool(v, "force").unwrap_or(false))?;
            done(format!("Removed the work tree {path}"))
        })
        .params(r#"{"path": string, "force"?: bool}"#)
        .when(need_repo),
        CommandSpec::new("worktree.prune", "Prune Worktrees", "Left Panel › Worktrees", |s, _| {
            s.repo()?.worktree_prune()?;
            done("Pruned work trees whose folders are gone")
        })
        .when(need_repo),
    ]
}
