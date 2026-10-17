//! The commit panel: the message, amend, and committing.

use gitcraft_git::RepoState;
use serde_json::{Value, json};

use super::edit::UndoStep;
use super::need_repo;
use crate::{CmdError, CmdResult, CommandSpec, Selected, Session, p};

/// Longest summary kept in the draft (the panel counts down from 72 but never cuts).
const MAX_DRAFT: usize = 64 * 1024;

fn message(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(t) = p::str(v, "summary") {
        s.draft.summary = t.chars().take(MAX_DRAFT).collect();
    }
    if let Some(t) = p::str(v, "description") {
        s.draft.description = t.chars().take(MAX_DRAFT).collect();
    }
    Ok(json!({"draft": s.draft}))
}

fn amend(s: &mut Session, v: &Value) -> CmdResult {
    let on = p::bool(v, "value").unwrap_or(!s.draft.amend);
    s.draft.amend = on;
    // Amending starts from the last commit's message.
    if on
        && s.draft.summary.trim().is_empty()
        && let Some(head) = s.data.status.head.clone()
        && let Ok(c) = s.repo()?.find_commit(&head)
    {
        s.draft.summary = c.summary;
        s.draft.description = c.body;
    }
    Ok(json!({"draft": s.draft}))
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let amend_flag = p::bool(v, "amend").unwrap_or(s.draft.amend);
    let summary = p::str(v, "summary").map(str::to_string).unwrap_or_else(|| s.draft.summary.clone());
    let description = p::str(v, "description").map(str::to_string).unwrap_or_else(|| s.draft.description.clone());
    let mut msg = crate::CommitDraft { summary, description, amend: amend_flag }.message();
    let repo = s.repo()?.clone();
    if msg.trim().is_empty() && repo.state() != RepoState::Clean {
        msg = repo.prepared_message().unwrap_or_default();
    }
    if msg.trim().is_empty() {
        return Err(CmdError::Params("write a commit summary".into()));
    }
    let st = repo.status()?;
    if !amend_flag && st.staged().next().is_none() && st.state != RepoState::Merging {
        return Err(CmdError::Params("nothing is staged: stage changes first".into()));
    }
    if st.conflicted().next().is_some() {
        return Err(CmdError::Params("resolve the conflicts first".into()));
    }
    let before = st.head.clone();
    let after = repo.commit(&msg, amend_flag)?;
    s.push_undo(UndoStep::Commit { before, after: after.clone(), branch: st.branch.clone(), amend: amend_flag });
    s.draft.clear();
    s.sel.row = Selected::Commit(after.clone());
    s.sel.file = None;
    let short = after.get(..7).unwrap_or(&after).to_string();
    Ok(json!({"message": format!("{} {short}", if amend_flag { "Amended" } else { "Committed" }), "oid": after}))
}

/// Edit the message of HEAD or a commit it descends from.
fn reword(s: &mut Session, v: &Value) -> CmdResult {
    let oid = p::str(v, "oid").ok_or_else(|| CmdError::Params("`oid` is required".into()))?;
    let summary = p::str(v, "summary").unwrap_or("").trim();
    if summary.is_empty() {
        return Err(CmdError::Params("write a commit summary".into()));
    }
    let description = p::str(v, "description").unwrap_or("");
    let msg = crate::CommitDraft { summary: summary.into(), description: description.into(), amend: false }.message();
    let w = s.repo()?.reword(oid, &msg)?;
    s.push_undo(UndoStep::Reword { before: w.before, after: w.after });
    s.sel.row = Selected::Commit(w.commit.clone());
    let short = w.commit.get(..7).unwrap_or(&w.commit).to_string();
    Ok(json!({"message": format!("Edited the message of {short}"), "oid": w.commit}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("commit.message", "Commit Message", "Commit Panel", message)
            .params(r#"{"summary"?: string, "description"?: string}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("commit.amend", "Amend Previous Commit", "Commit Panel", amend)
            .params(r#"{"value"?: bool}"#)
            .pure()
            .when(super::need_commits),
        CommandSpec::new("commit.reword", "Edit Commit Message", "Graph › Commit Menu", reword)
            .params(r#"{"oid": string, "summary": string, "description"?: string}"#)
            .when(super::need_commits),
        CommandSpec::new("commit.create", "Commit", "Commit Panel", create)
            .key("Mod+Enter")
            .params(r#"{"summary"?: string, "description"?: string, "amend"?: bool} (default: the commit panel's)"#)
            .when(need_repo),
        CommandSpec::new("commit.stageAllAndCommit", "Stage All and Commit", "Commit Panel", |s, v| {
            s.repo()?.stage_all()?;
            create(s, v)
        })
        .key("Mod+Shift+Enter")
        .params(r#"{"summary"?: string, "description"?: string}"#)
        .when(super::need_changes),
    ]
}
