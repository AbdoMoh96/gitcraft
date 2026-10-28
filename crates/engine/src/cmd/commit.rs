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
    let mut msg =
        crate::CommitDraft { summary, description, amend: amend_flag, co_authors: s.draft.co_authors.clone(), ..Default::default() }.message();
    let sign = p::bool(v, "sign").or(s.draft.sign);
    let no_verify = p::bool(v, "noVerify").unwrap_or(s.draft.no_verify);
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
    let after = repo.commit_with(&msg, &gitcraft_git::CommitOptions { amend: amend_flag, sign, no_verify })?;
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
    let msg = crate::CommitDraft { summary: summary.into(), description: description.into(), ..Default::default() }.message();
    let w = s.repo()?.reword(oid, &msg)?;
    s.push_undo(UndoStep::Reword { before: w.before, after: w.after });
    s.sel.row = Selected::Commit(w.commit.clone());
    let short = w.commit.get(..7).unwrap_or(&w.commit).to_string();
    Ok(json!({"message": format!("Edited the message of {short}"), "oid": w.commit}))
}

/// The commit template (`commit.template`, or `path`): its text, put in an empty commit message.
fn template(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    if let Some(path) = p::str(v, "path") {
        let scope = gitcraft_git::ConfigScope::parse(p::str(v, "scope").unwrap_or("local"))
            .ok_or_else(|| CmdError::Params("`scope` is local or global".into()))?;
        repo.set_config(scope, "commit.template", Some(path).filter(|x| !x.trim().is_empty()))?;
    }
    let Some(file) = repo.config("commit.template") else {
        return Ok(json!({"template": null, "message": "No commit template is set (commit.template)"}));
    };
    let path = expand_home(&file, repo.root());
    let meta = std::fs::metadata(&path).map_err(|e| CmdError::Failed(format!("{}: {e}", path.display())))?;
    if meta.len() > 256 * 1024 {
        return Err(CmdError::Failed("the commit template is too large".into()));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| CmdError::Failed(format!("{}: {e}", path.display())))?;
    let clean: String = text.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n");
    let clean = clean.trim();
    if s.draft.summary.trim().is_empty() && s.draft.description.trim().is_empty() {
        let (summary, description) = clean.split_once('\n').unwrap_or((clean, ""));
        s.draft.summary = summary.trim().to_string();
        s.draft.description = description.trim().to_string();
    }
    Ok(json!({"template": clean, "path": path.display().to_string(), "draft": s.draft}))
}

/// `~/x` and relative paths (from the repository) in a config value.
fn expand_home(p: &str, root: &std::path::Path) -> std::path::PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let (Some(rest), Some(h)) = (p.strip_prefix("~/"), home) {
        return std::path::PathBuf::from(h).join(rest);
    }
    let pb = std::path::PathBuf::from(p);
    if pb.is_absolute() { pb } else { root.join(pb) }
}

/// `Name <email>` entries, checked.
fn co_authors(s: &mut Session, v: &Value) -> CmdResult {
    let list =
        v.get("authors").and_then(Value::as_array).ok_or_else(|| CmdError::Params("`authors` (array of \"Name <email>\") is required".into()))?;
    let mut out = Vec::new();
    for a in list.iter().take(50) {
        let a = a.as_str().unwrap_or("").trim();
        let ok = a.len() < 300 && !a.contains(['\n', '\r', '\0']) && a.contains('<') && a.ends_with('>');
        if !ok {
            return Err(CmdError::Params(format!("`{a}` is not `Name <email>`")));
        }
        out.push(a.to_string());
    }
    s.draft.co_authors = out;
    Ok(json!({"draft": s.draft}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("commit.sign", "Sign Commits", "Commit Panel", |s, v| {
            let current = s.draft.sign.unwrap_or_else(|| s.data.signs_by_default());
            s.draft.sign = Some(p::bool(v, "value").unwrap_or(!current));
            Ok(json!({"value": s.draft.sign}))
        })
        .params(r#"{"value"?: bool}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("commit.noVerify", "Skip Hooks", "Commit Panel", |s, v| {
            s.draft.no_verify = p::bool(v, "value").unwrap_or(!s.draft.no_verify);
            Ok(json!({"value": s.draft.no_verify}))
        })
        .params(r#"{"value"?: bool}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("commit.coAuthors", "Co-authors", "Commit Panel", co_authors)
            .params(r#"{"authors": ["Name <email>"]}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("commit.template", "Commit Template", "Commit Panel", template)
            .params(r#"{"path"?: string (sets commit.template), "scope"?: "local" | "global"}"#)
            .pure()
            .when(need_repo),
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
            .params(r#"{"summary"?: string, "description"?: string, "amend"?: bool, "sign"?: bool, "noVerify"?: bool} (default: the commit panel's)"#)
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
