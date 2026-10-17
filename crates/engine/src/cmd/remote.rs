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

/// Bring a remote branch down and check it out: fetch its remote, then check out the local branch
/// (made to track it when missing, its upstream set when it has none) and fast-forward it. When
/// the two have diverged, `mode` says what to do: `merge`, `rebase`, `reset` (the local branch is
/// moved to the remote one, keeping uncommitted changes) or, by default, nothing (the result says
/// `"diverged": true` so the UI can ask).
fn pull_remote(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    let mode = p::str(v, "mode").unwrap_or("auto").trim().to_string();
    if !matches!(mode.as_str(), "auto" | "merge" | "rebase" | "reset") {
        return Err(CmdError::Params("`mode` is auto, merge, rebase or reset".into()));
    }
    let repo = s.repo()?.clone();
    let (remote, branch) = name.split_once('/').ok_or_else(|| CmdError::Params(format!("`{name}` is not a remote branch")))?;
    let remote_ref = format!("refs/remotes/{name}");
    if repo.resolve(&remote_ref).is_none() {
        return Err(CmdError::Params(format!("no remote branch `{name}`")));
    }
    // Offline is fine: the last fetched state is used, and the message says so.
    let note = match repo.fetch(Some(remote), false) {
        Ok(_) => String::new(),
        Err(e) => format!(" (couldn't fetch {remote}: {})", e.to_string().lines().next().unwrap_or("")),
    };
    let local_ref = format!("refs/heads/{branch}");
    if repo.resolve(&local_ref).is_none() {
        let local = repo.checkout_remote(&name)?;
        return Ok(json!({"message": format!("Checked out {local}, tracking {name}{note}"), "name": local}));
    }
    if repo.status()?.branch.as_deref() != Some(branch) {
        repo.checkout_branch(branch)?;
    }
    if repo.config(&format!("branch.{branch}.merge")).is_none() {
        repo.set_upstream(branch, &name)?;
    }
    let (Some(mine), Some(theirs)) = (repo.resolve(&local_ref), repo.resolve(&remote_ref)) else {
        return Err(CmdError::Params(format!("no branch `{branch}`")));
    };
    let up_to_date = mine == theirs || repo.is_ancestor(&theirs, &mine);
    let behind = !up_to_date && repo.is_ancestor(&mine, &theirs);
    let reply = |what: String, o: Option<&gitcraft_git::Outcome>| -> CmdResult {
        let conflicts = o.is_some_and(|o| o.conflicts);
        let message =
            if conflicts { format!("{what} stopped with conflicts: resolve them, then continue or abort") } else { format!("{what}{note}") };
        Ok(json!({"message": message, "name": branch, "conflicts": conflicts}))
    };
    if up_to_date {
        return reply(format!("Checked out {branch}: up to date with {name}"), None);
    }
    if behind {
        let o = repo.merge(&name, true, false)?;
        return reply(format!("Pulled {name} into {branch} (fast-forward)"), Some(&o));
    }
    match mode.as_str() {
        "merge" => {
            let o = repo.merge(&name, false, false)?;
            reply(format!("Pulled {name} into {branch} (merge)"), Some(&o))
        }
        "rebase" => {
            let o = repo.rebase(&name)?;
            reply(format!("Rebased {branch} onto {name}"), Some(&o))
        }
        "reset" => {
            repo.run_ok(&["reset", "-q", "--keep", theirs.as_str()])?;
            reply(format!("Reset {branch} to {name} (its old tip was {})", mine.get(..7).unwrap_or(&mine)), None)
        }
        _ => {
            let count = repo.text(&["rev-list", "--left-right", "--count", &format!("{mine}...{theirs}")]).unwrap_or_default();
            let mut it = count.split_whitespace().map(|n| n.parse::<u64>().unwrap_or(0));
            let (ahead, behind) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
            // Local commits with no same-change twin on the remote (0: the remote history was
            // rewritten from them, so a merge would duplicate them).
            let unique = repo
                .text(&["rev-list", "--left-only", "--cherry-pick", "--no-merges", "--count", &format!("{mine}...{theirs}")])
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok())
                .unwrap_or(ahead);
            Ok(json!({
                "message": format!("{branch} and {name} have diverged ({ahead} ahead, {behind} behind){note}"),
                "name": branch, "remote": name, "diverged": true, "ahead": ahead, "behind": behind, "unique": unique,
            }))
        }
    }
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
        CommandSpec::new("branch.pullRemote", "Pull Remote Branch", "Graph", pull_remote)
            .params(r#"{"name": "origin/feature", "mode"?: "auto" | "merge" | "rebase" | "reset" (when the branches have diverged)}"#)
            .slow()
            .when(need_remote),
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
