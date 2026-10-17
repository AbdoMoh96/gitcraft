//! Merge, rebase, cherry-pick, revert and reset, and continuing or aborting them.

use gitcraft_git::{RefKind, Repo, ResetMode};
use serde_json::{Value, json};

use super::edit::{Head, UndoStep};
use super::{done, need_branch, need_commits, need_operation, outcome, target_commit};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn merge(s: &mut Session, v: &Value) -> CmdResult {
    let r = p::req_str(v, "ref")?.trim().to_string();
    let ff_only = p::bool(v, "ffOnly").unwrap_or(false);
    let no_ff = p::bool(v, "noFF").unwrap_or(false);
    let into = s.data.status.branch.clone().unwrap_or_else(|| "HEAD".into());
    let o = s.repo()?.merge(&r, ff_only, no_ff)?;
    outcome(&format!("Merged {r} into {into}"), &o)
}

fn rebase(s: &mut Session, v: &Value) -> CmdResult {
    let onto = p::req_str(v, "ref")?.trim().to_string();
    let o = s.repo()?.rebase(&onto)?;
    outcome(&format!("Rebased onto {onto}"), &o)
}

fn cherry_pick(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let o = s.repo()?.cherry_pick(&oid)?;
    outcome(&format!("Cherry-picked {}", oid.get(..7).unwrap_or(&oid)), &o)
}

fn revert(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let o = s.repo()?.revert(&oid)?;
    outcome(&format!("Reverted {}", oid.get(..7).unwrap_or(&oid)), &o)
}

fn reset(s: &mut Session, v: &Value) -> CmdResult {
    let rev = target_commit(s, v)?;
    let mode = ResetMode::parse(p::str(v, "mode").unwrap_or("mixed")).ok_or_else(|| CmdError::Params("`mode` is soft, mixed or hard".into()))?;
    let repo = s.repo()?.clone();
    let before = repo.resolve("HEAD").ok_or_else(|| CmdError::Disabled("no commits yet".into()))?;
    let after = repo.resolve(&rev).ok_or_else(|| CmdError::Params(format!("no commit `{rev}`")))?;
    repo.reset(&after, mode)?;
    s.push_undo(UndoStep::Reset { before, after: after.clone(), mode });
    let branch = s.data.status.branch.clone().unwrap_or_else(|| "HEAD".into());
    Ok(json!({"message": format!("Reset {branch} to {}", after.get(..7).unwrap_or(&after)), "oid": after}))
}

/// Check out local branch `name` unless it is checked out already (undoable).
fn checkout_for(s: &mut Session, repo: &Repo, name: &str) -> Result<(), CmdError> {
    if s.data.status.branch.as_deref() == Some(name) {
        return Ok(());
    }
    let before = s.head();
    repo.checkout_branch(name)?;
    if let Some(from) = before {
        s.push_undo(UndoStep::Checkout { from, to: Head::Branch(name.to_string()) });
    }
    Ok(())
}

/// A ref dragged onto a local branch in the graph: merge it in, rebase onto the branch, or
/// fast-forward the branch to it. Merge and rebase check out the branch they change first.
fn drop_onto(s: &mut Session, v: &Value) -> CmdResult {
    let from = p::req_str(v, "from")?.trim().to_string();
    let onto = p::req_str(v, "onto")?.trim().to_string();
    let action = p::req_str(v, "action")?.trim().to_string();
    let repo = s.repo()?.clone();
    let local = |s: &Session, n: &str| s.data.refs.iter().any(|r| r.kind == RefKind::Local && r.name == n);
    if !local(s, &onto) {
        return Err(CmdError::Params(format!("`{onto}` is not a local branch")));
    }
    if from == onto {
        return Err(CmdError::Params("drop a branch onto a different branch".into()));
    }
    let from_oid = repo.resolve(&from).ok_or_else(|| CmdError::Params(format!("no branch or commit `{from}`")))?;
    let onto_ref = format!("refs/heads/{onto}");
    let onto_oid = repo.resolve(&onto_ref).ok_or_else(|| CmdError::Params(format!("no branch `{onto}`")))?;
    match action.as_str() {
        "merge" => {
            checkout_for(s, &repo, &onto)?;
            let o = repo.merge(&from, false, false)?;
            outcome(&format!("Merged {from} into {onto}"), &o)
        }
        "rebase" => {
            if !local(s, &from) {
                return Err(CmdError::Params(format!("only a local branch can be rebased, not `{from}`")));
            }
            checkout_for(s, &repo, &from)?;
            let o = repo.rebase(&onto)?;
            outcome(&format!("Rebased {from} onto {onto}"), &o)
        }
        "fastForward" => {
            if from_oid == onto_oid || !repo.is_ancestor(&onto_oid, &from_oid) {
                return Err(CmdError::Params(format!("{onto} can't be fast-forwarded to {from}")));
            }
            let what = format!("Fast-forwarded {onto} to {from}");
            if s.data.status.branch.as_deref() == Some(onto.as_str()) {
                let o = repo.merge(&from, true, false)?;
                return outcome(&what, &o);
            }
            repo.run_ok(&["update-ref", "-m", "gitcraft: fast-forward", onto_ref.as_str(), from_oid.as_str(), onto_oid.as_str()])?;
            s.push_undo(UndoStep::RefMove { name: onto, before: onto_oid, after: from_oid });
            done(what)
        }
        other => Err(CmdError::Params(format!("`action` is merge, rebase or fastForward, not `{other}`"))),
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("merge.into", "Merge into Current Branch", "Left Panel › Branches", merge)
            .params(r#"{"ref": branch or commit, "ffOnly"?: bool, "noFF"?: bool}"#)
            .when(need_commits),
        CommandSpec::new("merge.fastForward", "Fast-forward to", "Left Panel › Branches", |s, v| {
            let r = p::req_str(v, "ref")?.trim().to_string();
            let o = s.repo()?.merge(&r, true, false)?;
            outcome(&format!("Fast-forwarded to {r}"), &o)
        })
        .params(r#"{"ref": string}"#)
        .when(need_branch),
        CommandSpec::new("rebase.onto", "Rebase Current Branch onto", "Left Panel › Branches", rebase)
            .params(r#"{"ref": branch or commit}"#)
            .when(need_branch),
        CommandSpec::new("branch.dropOnto", "Drag Branch onto Branch", "Graph", drop_onto)
            .params(r#"{"from": branch, remote branch or tag, "onto": local branch, "action": "merge" | "rebase" | "fastForward"}"#)
            .when(need_commits),
        CommandSpec::new("op.continue", "Continue", "Commit Panel › Conflicts", |s, _| {
            let o = s.repo()?.continue_operation()?;
            outcome("Continued", &o)
        })
        .when(need_operation),
        CommandSpec::new("op.abort", "Abort", "Commit Panel › Conflicts", |s, _| {
            let what = s.data.status.state.label().to_lowercase();
            s.repo()?.abort_operation()?;
            done(format!("Stopped {what}"))
        })
        .when(need_operation),
        CommandSpec::new("rebase.skip", "Skip Commit", "Commit Panel › Conflicts", |s, _| {
            let o = s.repo()?.rebase_skip()?;
            outcome("Skipped the commit", &o)
        })
        .when(|s| need_operation(s).or_else(|| (s.data.status.state != gitcraft_git::RepoState::Rebasing).then_some("no rebase is in progress"))),
        CommandSpec::new("cherryPick.commit", "Cherry Pick Commit", "Graph › Commit Menu", cherry_pick)
            .params(r#"{"oid"?: commit (default: selected)}"#)
            .when(need_commits),
        CommandSpec::new("revert.commit", "Revert Commit", "Graph › Commit Menu", revert)
            .params(r#"{"oid"?: commit (default: selected)}"#)
            .when(need_commits),
        CommandSpec::new("reset.to", "Reset to This Commit", "Graph › Commit Menu", reset)
            .params(r#"{"oid"?: commit (default: selected), "mode"?: "soft" | "mixed" | "hard"}"#)
            .when(need_commits),
    ]
}
