//! Hosting services through their own tools (`gh`, `glab`): pull requests, issues, CI, the
//! launchpad (your work across repositories), your repositories, sharing patches; and the
//! account status. GitCraft stores no tokens.

use gitcraft_git::forge::{self, ForgeKind};
use serde_json::{Value, json};

use super::{done, need_branch, need_remote};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn the_forge(s: &Session, v: &Value) -> Result<gitcraft_git::forge::Forge, CmdError> {
    Ok(s.repo()?.forge(p::str(v, "remote").filter(|r| !r.trim().is_empty()))?)
}

fn kind(s: &Session, v: &Value) -> Result<ForgeKind, CmdError> {
    if let Some(k) = p::str(v, "provider").and_then(ForgeKind::parse) {
        return Ok(k);
    }
    match &s.repo {
        Some(_) => Ok(the_forge(s, v)?.kind),
        None => Ok(ForgeKind::GitHub),
    }
}

fn pr_list(s: &mut Session, v: &Value) -> CmdResult {
    let f = the_forge(s, v)?;
    let prs = s.repo()?.pull_requests(&f, p::str(v, "state").unwrap_or("open"))?;
    Ok(json!({"forge": f, "pullRequests": prs}))
}

fn pr_create(s: &mut Session, v: &Value) -> CmdResult {
    let f = the_forge(s, v)?;
    let repo = s.repo()?.clone();
    let head = match p::str(v, "head").filter(|h| !h.trim().is_empty()) {
        Some(h) => h.trim().to_string(),
        None => s.data.status.branch.clone().ok_or_else(|| CmdError::Params("`head` is required (HEAD is detached)".into()))?,
    };
    let base = match p::str(v, "base").filter(|h| !h.trim().is_empty()) {
        Some(b) => b.trim().to_string(),
        None => repo.remote_head(&f.remote).map(|h| h.rsplit('/').next().unwrap_or(&h).to_string()).unwrap_or_else(|| "main".into()),
    };
    let title = match p::str(v, "title").filter(|t| !t.trim().is_empty()) {
        Some(t) => t.trim().to_string(),
        None => repo.find_commit(&head).map(|c| c.summary).map_err(|_| CmdError::Params("`title` is required".into()))?,
    };
    let body = p::str(v, "body").unwrap_or("");
    // The branch must be on the remote first.
    if p::bool(v, "push").unwrap_or(true) {
        repo.push(Some(&f.remote), Some(&head), false, false)?;
    }
    let url = repo.create_pull_request(&f, &title, body, &head, &base, p::bool(v, "draft").unwrap_or(false))?;
    Ok(json!({"message": format!("Opened a pull request: {url}"), "url": url}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pr.list", "Pull Requests", "Left Panel › Pull Requests", pr_list)
            .params(r#"{"state"?: "open" | "closed" | "merged" | "all", "remote"?: string}"#)
            .pure()
            .slow()
            .when(need_remote),
        CommandSpec::new("pr.create", "Create Pull Request", "Left Panel › Branches", pr_create)
            .params(r#"{"title"?: string (default: the head commit's summary), "body"?: string, "head"?: branch (current), "base"?: branch (the remote's default), "draft"?: bool, "push"?: bool (true), "remote"?: string}"#)
            .slow()
            .when(|s| need_branch(s).or_else(|| need_remote(s))),
        CommandSpec::new("pr.checkout", "Check Out Pull Request", "Left Panel › Pull Requests", |s, v| {
            let f = the_forge(s, v)?;
            let n = p::u64(v, "number").ok_or_else(|| CmdError::Params("`number` is required".into()))?;
            s.repo()?.checkout_pull_request(&f, n)?;
            done(format!("Checked out pull request #{n}"))
        })
        .params(r#"{"number": number, "remote"?: string}"#)
        .slow()
        .when(need_remote),
        CommandSpec::new("issues.list", "Issues", "Left Panel › Issues", |s, v| {
            let f = the_forge(s, v)?;
            let issues = s.repo()?.issues(&f, p::str(v, "state").unwrap_or("open"))?;
            Ok(json!({"forge": f, "issues": issues}))
        })
        .params(r#"{"state"?: "open" | "closed" | "all", "remote"?: string}"#)
        .pure()
        .slow()
        .when(need_remote),
        CommandSpec::new("ci.list", "CI Status", "Left Panel › CI", |s, v| {
            let f = the_forge(s, v)?;
            let branch = p::str(v, "branch").map(str::trim).filter(|b| !b.is_empty()).map(str::to_string).or_else(|| s.data.status.branch.clone());
            let runs = s.repo()?.ci_runs(&f, if p::bool(v, "allBranches").unwrap_or(false) { None } else { branch.as_deref() })?;
            Ok(json!({"forge": f, "branch": branch, "runs": runs}))
        })
        .params(r#"{"branch"?: string (default: current), "allBranches"?: bool, "remote"?: string}"#)
        .pure()
        .slow()
        .when(need_remote),
        CommandSpec::new("integrations.connect", "Hosting Service Accounts", "Preferences", |s, v| {
            let mut out = Vec::new();
            let only = p::str(v, "provider").and_then(ForgeKind::parse);
            for k in [ForgeKind::GitHub, ForgeKind::GitLab] {
                if only.is_some_and(|o| o != k) {
                    continue;
                }
                let (signed_in, detail) = forge::auth_status(k);
                out.push(json!({"provider": k, "tool": k.tool(), "signedIn": signed_in, "detail": detail, "signIn": format!("{} auth login", k.tool())}));
            }
            let here = s.repo.as_ref().and_then(|r| r.forge(None).ok());
            Ok(json!({"accounts": out, "repository": here}))
        })
        .params(r#"{"provider"?: "github" | "gitlab"}"#)
        .pure()
        .slow(),
        CommandSpec::new("launchpad.open", "Launchpad", "Toolbar", |s, v| Ok(forge::launchpad(kind(s, v)?)?))
            .params(r#"{"provider"?: "github"}"#)
            .pure()
            .slow(),
        CommandSpec::new("hosting.repos", "Your Repositories", "Clone Repository", |s, v| Ok(json!({"repositories": forge::my_repositories(kind(s, v)?)?})))
            .params(r#"{"provider"?: "github" | "gitlab"}"#)
            .pure()
            .slow(),
        CommandSpec::new("cloudpatch.create", "Cloud Patches", "Graph › Commit Menu", |s, v| {
            let f = the_forge(s, v)?;
            let rev = p::str(v, "oid").map(str::trim).filter(|r| !r.is_empty());
            let repo = s.repo()?.clone();
            let patch = repo.make_patch(rev, p::bool(v, "staged").unwrap_or(false))?;
            let short = rev.map(|r| r.get(..7).unwrap_or(r).to_string()).unwrap_or_else(|| "changes".into());
            let name = format!("{}-{short}.patch", repo.name());
            let desc = p::str(v, "description").map(str::to_string).unwrap_or_else(|| format!("Patch from {} ({short})", repo.name()));
            let url = repo.share_patch(&f, &name, &desc, &patch)?;
            Ok(json!({"message": format!("Shared the patch (secret): {url}"), "url": url}))
        })
        .params(r#"{"oid"?: commit (default: the uncommitted changes), "staged"?: bool, "description"?: string, "remote"?: string}"#)
        .pure()
        .slow()
        .when(need_remote),
    ]
}
