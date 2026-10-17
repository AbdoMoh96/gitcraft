//! Stashes.

use serde_json::Value;

use super::{done, need_changes, need_stash, outcome};
use crate::{CmdResult, CommandSpec, Session, p};

fn index(v: &Value) -> usize {
    p::usize(v, "index").unwrap_or(0)
}

fn save(s: &mut Session, v: &Value) -> CmdResult {
    let message = p::str(v, "message");
    let untracked = p::bool(v, "untracked").unwrap_or(true);
    s.repo()?.stash_save(message, untracked)?;
    done("Stashed changes")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("stash.save", "Stash", "Toolbar", save)
            .key("Mod+Alt+S")
            .params(r#"{"message"?: string, "untracked"?: bool (default true)}"#)
            .when(need_changes),
        CommandSpec::new("stash.pop", "Pop", "Toolbar", |s, v| {
            let i = index(v);
            let o = s.repo()?.stash_pop(i)?;
            outcome(&format!("Popped stash@{{{i}}}"), &o)
        })
        .key("Mod+Alt+P")
        .params(r#"{"index"?: number (default 0, the newest)}"#)
        .when(need_stash),
        CommandSpec::new("stash.apply", "Apply Stash", "Left Panel › Stashes", |s, v| {
            let i = index(v);
            let o = s.repo()?.stash_apply(i)?;
            outcome(&format!("Applied stash@{{{i}}}"), &o)
        })
        .params(r#"{"index"?: number}"#)
        .when(need_stash),
        CommandSpec::new("stash.drop", "Delete Stash", "Left Panel › Stashes", |s, v| {
            let i = index(v);
            s.repo()?.stash_drop(i)?;
            done(format!("Deleted stash@{{{i}}}"))
        })
        .params(r#"{"index"?: number}"#)
        .when(need_stash),
    ]
}
