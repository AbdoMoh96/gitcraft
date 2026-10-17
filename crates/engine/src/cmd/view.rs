//! Selection and view settings: the graph row, the file in the diff view, diff mode, search.

use serde_json::{Value, json};

use super::need_repo;
use crate::{Area, CmdError, CmdResult, CommandSpec, DiffMode, FileSel, Selected, Session, p};

fn select(s: &mut Session, v: &Value) -> CmdResult {
    let target = p::str(v, "oid").or_else(|| p::str(v, "ref")).unwrap_or("wip").trim();
    s.sel.file = None;
    if target.eq_ignore_ascii_case("wip") {
        s.sel.row = Selected::Wip;
        return Ok(json!({"selected": "wip"}));
    }
    let oid = match s.data.row_of.get(target) {
        Some(_) => target.to_string(),
        None => s.repo()?.resolve(target).ok_or_else(|| CmdError::Params(format!("no commit `{target}`")))?,
    };
    s.sel.row = Selected::Commit(oid.clone());
    Ok(json!({"selected": oid, "loaded": s.data.row_of.contains_key(&oid)}))
}

/// The selected row's index, with the WIP row as -1.
fn row_index(s: &Session) -> Option<isize> {
    match &s.sel.row {
        Selected::Wip => Some(-1),
        Selected::Commit(o) => s.data.row_of.get(o).map(|&i| i as isize),
        Selected::None => None,
    }
}

fn step(s: &mut Session, by: isize) -> CmdResult {
    let has_wip = !s.data.status.is_clean();
    let first = if has_wip { -1 } else { 0 };
    let last = s.data.commits.len() as isize - 1;
    let cur = row_index(s).unwrap_or(first - by.signum());
    let next = (cur + by).clamp(first, last.max(first));
    s.sel.file = None;
    s.sel.row = if next < 0 {
        Selected::Wip
    } else {
        match s.data.commits.get(next as usize) {
            Some(c) => Selected::Commit(c.oid.clone()),
            None => Selected::Wip,
        }
    };
    Ok(json!({"selection": s.sel}))
}

fn select_file(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?.to_string();
    let oid = p::str(v, "oid").map(str::to_string).or_else(|| match &s.sel.row {
        Selected::Commit(o) => Some(o.clone()),
        _ => None,
    });
    let area_name = p::str(v, "area").unwrap_or(if oid.is_some() && p::str(v, "oid").is_some() { "commit" } else { "unstaged" });
    let area = Area::parse(area_name, oid.as_deref())
        .ok_or_else(|| CmdError::Params("`area` is unstaged, staged or commit (with a selected commit or `oid`)".into()))?;
    if let Area::Commit(o) = &area {
        s.sel.row = Selected::Commit(o.clone());
    } else {
        s.sel.row = Selected::Wip;
    }
    s.sel.file = Some(FileSel { path, area });
    Ok(json!({"selection": s.sel}))
}

fn toggle(cur: bool, v: &Value) -> bool {
    p::bool(v, "value").unwrap_or(!cur)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("graph.select", "Select Commit", "Graph", select).params(r#"{"oid": commit id, ref name or "wip"}"#).pure().when(need_repo),
        CommandSpec::new("graph.selectNext", "Next Commit", "Graph", |s, _| step(s, 1)).key("Down").pure().when(need_repo),
        CommandSpec::new("graph.selectPrevious", "Previous Commit", "Graph", |s, _| step(s, -1)).key("Up").pure().when(need_repo),
        CommandSpec::new("graph.selectHead", "Go to HEAD", "Graph", |s, _| {
            let head = s.data.status.head.clone().ok_or_else(|| CmdError::Disabled("no commits yet".into()))?;
            select(s, &json!({"oid": head}))
        })
        .key("Mod+Shift+H")
        .pure()
        .when(super::need_commits),
        CommandSpec::new("graph.loadMore", "Load More Commits", "Graph", |s, v| {
            let more = p::usize(v, "count").unwrap_or(2000).clamp(1, 100_000);
            s.view.commit_limit = s.view.commit_limit.saturating_add(more).min(gitcraft_git::log::MAX_LIMIT - 1);
            s.refresh()?;
            Ok(json!({"loaded": s.data.commits.len(), "more": s.data.more}))
        })
        .params(r#"{"count"?: number}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("file.select", "Open File Diff", "Commit Panel", select_file)
            .params(r#"{"path": string, "area"?: "unstaged" | "staged" | "commit", "oid"?: commit id}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("file.close", "Close Diff", "Diff View", |s, _| {
            s.sel.file = None;
            s.view.blame = false;
            Ok(json!({}))
        })
        .key("Escape")
        .pure(),
        CommandSpec::new("view.diffMode", "Diff View Mode", "Diff View", |s, v| {
            let m = p::req_str(v, "value")?;
            s.view.diff_mode = DiffMode::parse(m).ok_or_else(|| CmdError::Params("`value` is hunk, inline or split".into()))?;
            Ok(json!({"value": s.view.diff_mode.code()}))
        })
        .params(r#"{"value": "hunk" | "inline" | "split"}"#)
        .pure(),
        CommandSpec::new("view.context", "Context Lines", "Diff View", |s, v| {
            s.view.context = p::u64(v, "value").ok_or_else(|| CmdError::Params("`value` (number) is required".into()))?.min(100) as u32;
            Ok(json!({"value": s.view.context}))
        })
        .params(r#"{"value": number (0–100)}"#)
        .pure(),
        CommandSpec::new("view.leftPanel", "Left Panel", "View", |s, v| {
            s.view.left_panel = toggle(s.view.left_panel, v);
            Ok(json!({"value": s.view.left_panel}))
        })
        .key("Mod+J")
        .params(r#"{"value"?: bool}"#)
        .pure(),
        CommandSpec::new("view.search", "Search Commits", "Graph", |s, v| {
            s.view.search = p::str(v, "text").unwrap_or("").chars().take(500).collect();
            let q = s.view.search.to_lowercase();
            let hits: Vec<&str> =
                if q.is_empty() { Vec::new() } else { s.data.commits.iter().filter(|c| matches(c, &q)).map(|c| c.oid.as_str()).collect() };
            Ok(json!({"text": s.view.search, "matches": hits.len(), "first": hits.first()}))
        })
        .key("Mod+F")
        .params(r#"{"text": string}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("view.blame", "Blame", "Diff View", |s, v| {
            s.view.blame = toggle(s.view.blame, v);
            Ok(json!({"value": s.view.blame}))
        })
        .params(r#"{"value"?: bool}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("view.history", "File History", "Diff View", |s, v| {
            let path = match p::str(v, "path").filter(|x| !x.trim().is_empty()) {
                Some(x) => x.to_string(),
                None => s.sel.file.as_ref().map(|f| f.path.clone()).ok_or_else(|| CmdError::Params("`path` is required (or open a file)".into()))?,
            };
            s.view.history = Some(path.clone());
            s.sel.file = None;
            s.refresh()?;
            Ok(json!({"path": path, "commits": s.data.commits.len()}))
        })
        .params(r#"{"path"?: string (default: the open file)}"#)
        .pure()
        .when(super::need_commits),
        CommandSpec::new("view.historyClear", "Show All Commits", "Graph", |s, _| {
            s.view.history = None;
            s.refresh()?;
            Ok(json!({"commits": s.data.commits.len()}))
        })
        .pure()
        .when(|s| need_repo(s).or_else(|| s.view.history.is_none().then_some("the graph shows every commit"))),
    ]
}

/// Whether a commit matches a lower-cased search.
pub fn matches(c: &gitcraft_git::Commit, q: &str) -> bool {
    c.oid.starts_with(q)
        || c.summary.to_lowercase().contains(q)
        || c.body.to_lowercase().contains(q)
        || c.author.name.to_lowercase().contains(q)
        || c.author.email.to_lowercase().contains(q)
}
