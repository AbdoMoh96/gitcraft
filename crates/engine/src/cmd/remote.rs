//! Remotes: fetch, pull, push, add and remove.

use gitcraft_git::PullMode;
use serde_json::{Value, json};

use super::{done, need_branch, need_remote, need_repo, outcome};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn fetch(s: &mut Session, v: &Value) -> CmdResult {
    let remote = p::str(v, "remote").filter(|r| !r.trim().is_empty());
    let prune = p::bool(v, "prune").unwrap_or(true);
    let msg = s.repo()?.fetch(remote, prune)?;
    Ok(json!({"message": format!("Fetched {}", remote.unwrap_or("all remotes")), "git": msg}))
}

fn pull(s: &mut Session, v: &Value) -> CmdResult {
    let mode = match p::str(v, "mode") {
        Some(m) => PullMode::parse(m).ok_or_else(|| CmdError::Params("`mode` is merge, ffOnly or rebase".into()))?,
        None => PullMode::Merge,
    };
    let o = s.repo()?.pull(mode)?;
    outcome("Pulled", &o)
}

fn push(s: &mut Session, v: &Value) -> CmdResult {
    let remote = p::str(v, "remote").filter(|r| !r.trim().is_empty());
    let branch = p::str(v, "branch").filter(|r| !r.trim().is_empty());
    let force = p::bool(v, "force").unwrap_or(false);
    let set_up = p::bool(v, "setUpstream").unwrap_or(false);
    let msg = s.repo()?.push(remote, branch, force, set_up)?;
    Ok(json!({"message": if force { "Force pushed (with lease)" } else { "Pushed" }, "git": msg}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("remote.fetch", "Fetch", "Toolbar › Pull", fetch)
            .key("Mod+Shift+F")
            .params(r#"{"remote"?: string (default: all), "prune"?: bool (default true)}"#)
            .slow()
            .when(need_remote),
        CommandSpec::new("remote.pull", "Pull", "Toolbar", pull)
            .key("Mod+Shift+L")
            .params(r#"{"mode"?: "merge" | "ffOnly" | "rebase"}"#)
            .slow()
            .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("remote.pullFastForward", "Pull (fast-forward if possible)", "Toolbar › Pull", |s, _| {
            let o = s.repo()?.pull(PullMode::FastForwardOnly)?;
            outcome("Pulled", &o)
        })
        .slow()
        .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("remote.pullRebase", "Pull (rebase)", "Toolbar › Pull", |s, _| {
            let o = s.repo()?.pull(PullMode::Rebase)?;
            outcome("Pulled with rebase", &o)
        })
        .slow()
        .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("remote.push", "Push", "Toolbar", push)
            .key("Mod+Shift+K")
            .params(r#"{"remote"?: string, "branch"?: string (default: current), "force"?: bool (with lease), "setUpstream"?: bool}"#)
            .slow()
            .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("remote.forcePush", "Force Push", "Toolbar › Push", |s, v| {
            let mut v = v.clone();
            if let Some(o) = v.as_object_mut() {
                o.insert("force".into(), json!(true));
            }
            push(s, &v)
        })
        .params(r#"{"remote"?: string, "branch"?: string}"#)
        .slow()
        .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("remote.add", "Add Remote", "Left Panel › Remote", |s, v| {
            let name = p::req_str(v, "name")?;
            let url = p::req_str(v, "url")?;
            s.repo()?.add_remote(name, url)?;
            done(format!("Added remote {name}"))
        })
        .params(r#"{"name": string, "url": string}"#)
        .when(need_repo),
        CommandSpec::new("remote.remove", "Remove Remote", "Left Panel › Remote", |s, v| {
            let name = p::req_str(v, "name")?;
            s.repo()?.remove_remote(name)?;
            done(format!("Removed remote {name}"))
        })
        .params(r#"{"name": string}"#)
        .when(need_remote),
    ]
}
