//! Tags.

use serde_json::{Value, json};

use super::edit::UndoStep;
use super::{done, need_commits, need_remote};
use crate::{CmdError, CmdResult, CommandSpec, Selected, Session, p};

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?;
    let rev = match p::str(v, "oid").filter(|o| !o.trim().is_empty()) {
        Some(o) => o.trim().to_string(),
        None => match &s.sel.row {
            Selected::Commit(o) => o.clone(),
            _ => "HEAD".into(),
        },
    };
    let message = p::str(v, "message");
    let repo = s.repo()?.clone();
    let oid = repo.resolve(&rev).ok_or_else(|| CmdError::Params(format!("no commit `{rev}`")))?;
    let name = repo.create_tag(name, Some(&oid), message)?;
    // An annotated tag comes back as a lightweight one on redo.
    s.push_undo(UndoStep::TagCreate { name: name.clone(), oid: oid.clone() });
    Ok(json!({"message": format!("Created tag {name}"), "name": name, "oid": oid}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    let repo = s.repo()?.clone();
    let oid = repo.resolve(&format!("refs/tags/{name}")).ok_or_else(|| CmdError::Params(format!("no tag `{name}`")))?;
    repo.delete_tag(&name)?;
    s.push_undo(UndoStep::TagDelete { name: name.clone(), oid });
    done(format!("Deleted tag {name}"))
}

fn push(s: &mut Session, v: &Value) -> CmdResult {
    let name = p::req_str(v, "name")?.trim().to_string();
    let repo = s.repo()?.clone();
    let remote = match p::str(v, "remote").filter(|r| !r.trim().is_empty()) {
        Some(r) => r.to_string(),
        None => repo.default_remote()?,
    };
    repo.push_tag(&remote, &name)?;
    done(format!("Pushed tag {name} to {remote}"))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("tag.create", "Create Tag Here", "Graph › Commit Menu", create)
            .params(r#"{"name": string, "oid"?: commit (default: selected or HEAD), "message"?: string (annotated)}"#)
            .when(need_commits),
        CommandSpec::new("tag.delete", "Delete Tag", "Left Panel › Tags", delete).params(r#"{"name": string}"#).when(need_commits),
        CommandSpec::new("tag.push", "Push Tag", "Left Panel › Tags", push).params(r#"{"name": string, "remote"?: string}"#).slow().when(need_remote),
    ]
}
