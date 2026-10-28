//! History editing: interactive rebase (reorder, reword, squash, fixup, drop) and its shortcuts,
//! squashing a commit into its parent and dropping a commit. Each is undoable.

use gitcraft_git::{TodoAction, TodoItem};
use serde_json::{Value, json};

use super::edit::UndoStep;
use super::{need_commits, outcome, target_commit};
use crate::{CmdError, CmdResult, CommandSpec, Selected, Session, p};

fn need_clean_state(s: &Session) -> Option<&'static str> {
    need_commits(s).or_else(|| (s.data.status.state != gitcraft_git::RepoState::Clean).then_some("finish or abort the operation in progress first"))
}

/// The commits an interactive rebase from `oid` would replay, oldest first.
fn plan(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let (base, commits) = s.repo()?.rebase_plan(&oid)?;
    Ok(json!({
        "base": base,
        "commits": commits.iter().map(|c| json!({"oid": c.oid, "short": c.short(), "summary": c.summary, "body": c.body, "author": c.author.name})).collect::<Vec<_>>(),
    }))
}

fn items(v: &Value) -> Result<Vec<TodoItem>, CmdError> {
    let a = v.get("items").and_then(Value::as_array).ok_or_else(|| CmdError::Params("`items` (array) is required".into()))?;
    a.iter()
        .map(|x| {
            let action = p::str(x, "action")
                .and_then(TodoAction::parse)
                .ok_or_else(|| CmdError::Params("each item has an `action`: pick, reword, squash, fixup or drop".into()))?;
            let oid = p::req_str(x, "oid")?.to_string();
            Ok(TodoItem { action, oid, message: p::str(x, "message").map(str::to_string) })
        })
        .collect()
}

/// Run a rebase over `items` and record it for undo when it finished.
fn rewrite(s: &mut Session, oid: &str, items: &[TodoItem], label: &str, what: &str) -> CmdResult {
    let repo = s.repo()?.clone();
    let before = repo.resolve("HEAD").ok_or_else(|| CmdError::Disabled("no commits yet".into()))?;
    let o = repo.rebase_interactive(oid, items)?;
    if !o.conflicts
        && let Some(after) = repo.resolve("HEAD")
        && after != before
    {
        s.push_undo(UndoStep::Rewrite { label: label.into(), before, after: after.clone() });
        s.sel.row = Selected::Commit(after);
        s.sel.file = None;
    }
    outcome(what, &o)
}

fn interactive(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let items = items(v)?;
    let n = items.len();
    rewrite(s, &oid, &items, "Interactive rebase", &format!("Rebased {n} commit{}", if n == 1 { "" } else { "s" }))
}

/// Squash a commit into the one before it (both messages kept, or `message`).
fn squash(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let repo = s.repo()?.clone();
    let full = repo.resolve(&oid).ok_or_else(|| CmdError::Params(format!("no commit `{oid}`")))?;
    let parent = repo.resolve(&format!("{full}^1")).ok_or_else(|| CmdError::Params("the first commit has nothing to squash into".into()))?;
    let (_, commits) = repo.rebase_plan(&parent)?;
    let message = p::str(v, "message").map(str::to_string);
    let list: Vec<TodoItem> = commits
        .iter()
        .map(|c| {
            let action = if c.oid == full { TodoAction::Squash } else { TodoAction::Pick };
            TodoItem { action, oid: c.oid.clone(), message: if c.oid == full { message.clone() } else { None } }
        })
        .collect();
    let short = full.get(..7).unwrap_or(&full).to_string();
    rewrite(s, &parent, &list, "Squash", &format!("Squashed {short} into {}", parent.get(..7).unwrap_or(&parent)))
}

fn drop_commit(s: &mut Session, v: &Value) -> CmdResult {
    let oid = target_commit(s, v)?;
    let repo = s.repo()?.clone();
    let full = repo.resolve(&oid).ok_or_else(|| CmdError::Params(format!("no commit `{oid}`")))?;
    let (_, commits) = repo.rebase_plan(&full)?;
    let list: Vec<TodoItem> = commits
        .iter()
        .map(|c| TodoItem { action: if c.oid == full { TodoAction::Drop } else { TodoAction::Pick }, oid: c.oid.clone(), message: None })
        .collect();
    let short = full.get(..7).unwrap_or(&full).to_string();
    rewrite(s, &full, &list, "Drop commit", &format!("Dropped {short}"))
}

/// Move a commit one place up (newer) or down (older) in the branch's history.
fn shift(s: &mut Session, v: &Value, newer: bool) -> CmdResult {
    let oid = target_commit(s, v)?;
    let repo = s.repo()?.clone();
    let full = repo.resolve(&oid).ok_or_else(|| CmdError::Params(format!("no commit `{oid}`")))?;
    let from = if newer {
        full.clone()
    } else {
        repo.resolve(&format!("{full}^1")).ok_or_else(|| CmdError::Params("the first commit can't move down".into()))?
    };
    let (_, commits) = repo.rebase_plan(&from)?;
    let mut ids: Vec<String> = commits.iter().map(|c| c.oid.clone()).collect();
    let i = ids.iter().position(|c| *c == full).ok_or_else(|| CmdError::Params("the commit is not on this branch".into()))?;
    let j = if newer { i + 1 } else { i.checked_sub(1).ok_or_else(|| CmdError::Params("the commit can't move down".into()))? };
    if j >= ids.len() {
        return Err(CmdError::Params("the commit is already the newest".into()));
    }
    ids.swap(i, j);
    let list: Vec<TodoItem> = ids.into_iter().map(|oid| TodoItem { action: TodoAction::Pick, oid, message: None }).collect();
    let short = full.get(..7).unwrap_or(&full).to_string();
    rewrite(s, &from, &list, "Move commit", &format!("Moved {short} {}", if newer { "up" } else { "down" }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("rebase.plan", "Interactive Rebase Plan", "Graph › Commit Menu", plan)
            .params(r#"{"oid"?: oldest commit to edit (default: selected)}"#)
            .pure()
            .when(need_commits),
        CommandSpec::new("rebase.interactive", "Interactive Rebase", "Graph › Commit Menu", interactive)
            .params(r#"{"oid"?: oldest commit to edit (default: selected), "items": [{"action": "pick" | "reword" | "squash" | "fixup" | "drop", "oid": commit, "message"?: string}] (oldest first; commits left out are dropped)}"#)
            .when(need_clean_state),
        CommandSpec::new("commit.squash", "Squash into Parent", "Graph › Commit Menu", squash)
            .params(r#"{"oid"?: commit (default: selected), "message"?: string (default: both messages)}"#)
            .when(need_clean_state),
        CommandSpec::new("commit.drop", "Drop Commit", "Graph › Commit Menu", drop_commit)
            .params(r#"{"oid"?: commit (default: selected)}"#)
            .when(need_clean_state),
        CommandSpec::new("commit.moveUp", "Move Commit Up", "Graph › Commit Menu", |s, v| shift(s, v, true))
            .params(r#"{"oid"?: commit (default: selected)}"#)
            .when(need_clean_state),
        CommandSpec::new("commit.moveDown", "Move Commit Down", "Graph › Commit Menu", |s, v| shift(s, v, false))
            .params(r#"{"oid"?: commit (default: selected)}"#)
            .when(need_clean_state),
    ]
}
