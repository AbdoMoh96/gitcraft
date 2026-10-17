//! Merge, rebase, cherry-pick, revert and reset, and continuing or aborting them.

use gitcraft_git::ResetMode;
use serde_json::{Value, json};

use super::edit::UndoStep;
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
