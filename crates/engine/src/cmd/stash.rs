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
    let paths = if v.get("paths").is_some() || v.get("path").is_some() { p::paths(v)? } else { Vec::new() };
    s.repo()?.stash_paths(message, untracked, &paths)?;
    done(if paths.is_empty() {
        "Stashed changes".to_string()
    } else {
        format!("Stashed {} file{}", paths.len(), if paths.len() == 1 { "" } else { "s" })
    })
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("stash.save", "Stash", "Toolbar", save)
            .key("Mod+Alt+S")
            .params(r#"{"message"?: string, "untracked"?: bool (default true), "paths"?: [string] (default: every change)}"#)
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
        CommandSpec::new("stash.rename", "Rename Stash", "Left Panel › Stashes", |s, v| {
            let i = index(v);
            let m = p::req_str(v, "message")?.trim().to_string();
            s.repo()?.stash_rename(i, &m)?;
            done(format!("Renamed stash@{{{i}}} to \"{m}\" (now stash@{{0}})"))
        })
        .params(r#"{"index"?: number, "message": string}"#)
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
