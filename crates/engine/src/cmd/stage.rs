//! Staging and discarding: whole files, everything, one hunk, or chosen lines.

use gitcraft_git::patch::Target;
use serde_json::{Value, json};

use super::{done, need_changes, need_staged, need_unstaged};
use crate::{Area, CmdError, CmdResult, CommandSpec, FileSel, Session, p};

fn files(n: usize) -> String {
    if n == 1 { "1 file".into() } else { format!("{n} files") }
}

fn stage_files(s: &mut Session, v: &Value) -> CmdResult {
    let ps = p::paths(v)?;
    s.repo()?.stage(&ps)?;
    done(format!("Staged {}", files(ps.len())))
}

fn unstage_files(s: &mut Session, v: &Value) -> CmdResult {
    let ps = p::paths(v)?;
    s.repo()?.unstage(&ps)?;
    done(format!("Unstaged {}", files(ps.len())))
}

fn discard_files(s: &mut Session, v: &Value) -> CmdResult {
    let ps = p::paths(v)?;
    s.repo()?.discard(&ps)?;
    done(format!("Discarded changes to {}", files(ps.len())))
}

/// Stage, unstage or discard hunk `hunk` of `path` (only `lines` of it when given). The hunk and
/// line indices are those of the diff shown with `context` lines (the view's setting).
fn partial(s: &mut Session, v: &Value, target: Target, with_lines: bool) -> CmdResult {
    let path = p::req_str(v, "path")?.to_string();
    let hunk = p::usize(v, "hunk").ok_or_else(|| CmdError::Params("`hunk` (number) is required".into()))?;
    let lines = if with_lines {
        Some(p::indices(v, "lines").ok_or_else(|| CmdError::Params("`lines` (array of numbers) is required".into()))?)
    } else {
        None
    };
    let context = p::u64(v, "context").map(|c| c.min(100_000) as u32).unwrap_or(s.view.context);
    let area = if target == Target::Unstage { Area::Staged } else { Area::Unstaged };
    let diff = s.diff_for(&FileSel { path: path.clone(), area }, context)?;
    let repo = s.repo()?;
    // An untracked file has no index entry for a patch to apply to: stage it whole.
    if target == Target::Stage && lines.is_none() && diff.old_path.is_none() && diff.hunks.len() <= 1 {
        repo.stage(std::slice::from_ref(&path))?;
    } else {
        repo.apply_partial(&diff, hunk, lines.as_deref(), target)?;
    }
    let what = match target {
        Target::Stage => "Staged",
        Target::Unstage => "Unstaged",
        Target::Discard => "Discarded",
    };
    Ok(json!({"message": format!("{what} {} in {path}", if with_lines { "lines" } else { "a hunk" })}))
}

const HUNK: &str = r#"{"path": string, "hunk": index, "context"?: number (the view's)}"#;
const LINES: &str = r#"{"path": string, "hunk": index, "lines": [index in the hunk], "context"?: number}"#;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("stage.file", "Stage File", "Commit Panel › Unstaged", stage_files)
            .params(r#"{"path": string} | {"paths": [string]}"#)
            .when(need_unstaged),
        CommandSpec::new("unstage.file", "Unstage File", "Commit Panel › Staged", unstage_files)
            .params(r#"{"path": string} | {"paths": [string]}"#)
            .when(need_staged),
        CommandSpec::new("stage.all", "Stage All Changes", "Commit Panel › Unstaged", |s, _| {
            s.repo()?.stage_all()?;
            done("Staged all changes")
        })
        .key("Mod+Shift+S")
        .when(need_unstaged),
        CommandSpec::new("unstage.all", "Unstage All Changes", "Commit Panel › Staged", |s, _| {
            s.repo()?.unstage_all()?;
            done("Unstaged all changes")
        })
        .key("Mod+Shift+U")
        .when(need_staged),
        CommandSpec::new("discard.file", "Discard Changes", "Commit Panel › Unstaged", discard_files)
            .params(r#"{"path": string} | {"paths": [string]}"#)
            .when(need_unstaged),
        CommandSpec::new("discard.all", "Discard All Changes", "Toolbar › WIP", |s, _| {
            s.repo()?.discard_all()?;
            done("Discarded all changes")
        })
        .when(need_changes),
        CommandSpec::new("stage.hunk", "Stage Hunk", "Diff View", |s, v| partial(s, v, Target::Stage, false)).params(HUNK).when(need_unstaged),
        CommandSpec::new("unstage.hunk", "Unstage Hunk", "Diff View", |s, v| partial(s, v, Target::Unstage, false)).params(HUNK).when(need_staged),
        CommandSpec::new("discard.hunk", "Discard Hunk", "Diff View", |s, v| partial(s, v, Target::Discard, false)).params(HUNK).when(need_unstaged),
        CommandSpec::new("stage.lines", "Stage Lines", "Diff View", |s, v| partial(s, v, Target::Stage, true)).params(LINES).when(need_unstaged),
        CommandSpec::new("unstage.lines", "Unstage Lines", "Diff View", |s, v| partial(s, v, Target::Unstage, true)).params(LINES).when(need_staged),
        CommandSpec::new("discard.lines", "Discard Lines", "Diff View", |s, v| partial(s, v, Target::Discard, true))
            .params(LINES)
            .when(need_unstaged),
    ]
}
