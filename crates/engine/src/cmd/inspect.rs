//! Queries for agents and the CLI: status, history, commits, diffs, blame, refs, activity.

use gitcraft_git::LogQuery;
use serde_json::{Value, json};

use super::{need_commits, need_repo, target_commit};
use crate::{Area, CmdError, CmdResult, CommandSpec, FileSel, Session, p};

fn commit_json(c: &gitcraft_git::Commit) -> Value {
    json!({"oid": c.oid, "short": c.short(), "parents": c.parents, "author": c.author, "committer": c.committer, "summary": c.summary, "body": c.body})
}

fn log(s: &mut Session, v: &Value) -> CmdResult {
    let q = LogQuery {
        revs: match p::str(v, "ref").filter(|r| !r.trim().is_empty()) {
            Some(r) => vec![r.to_string()],
            None => Vec::new(),
        },
        limit: p::usize(v, "limit").unwrap_or(50).clamp(1, 10_000),
        skip: p::usize(v, "skip").unwrap_or(0).min(10_000_000),
        path: p::str(v, "path").filter(|x| !x.is_empty()).map(str::to_string),
        follow: p::str(v, "path").is_some_and(|x| !x.is_empty()),
        grep: p::str(v, "grep").map(str::to_string),
        author: p::str(v, "author").map(str::to_string),
        exclude: Vec::new(),
    };
    let mut q = q;
    if q.path.is_some() && q.revs.is_empty() {
        q.revs.push("HEAD".into());
    }
    let commits = s.repo()?.log(&q)?;
    Ok(json!({"commits": commits.iter().map(commit_json).collect::<Vec<_>>()}))
}

fn show(s: &mut Session, v: &Value) -> CmdResult {
    let rev = target_commit(s, v)?;
    let repo = s.repo()?;
    let c = repo.find_commit(&rev)?;
    let files = repo.commit_files(&c.oid)?;
    let labels: Vec<&str> =
        s.data.labels.get(&c.oid).map(|ix| ix.iter().filter_map(|&i| s.data.refs.get(i)).map(|r| r.name.as_str()).collect()).unwrap_or_default();
    Ok(json!({"commit": commit_json(&c), "files": files, "refs": labels}))
}

fn diff(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?.to_string();
    let oid = p::str(v, "oid").filter(|o| !o.trim().is_empty());
    let area = Area::parse(p::str(v, "area").unwrap_or(if oid.is_some() { "commit" } else { "unstaged" }), oid)
        .ok_or_else(|| CmdError::Params("`area` is unstaged, staged or commit (with `oid`)".into()))?;
    let context = p::u64(v, "context").map(|c| c.min(100_000) as u32).unwrap_or(s.view.context);
    let ws = p::bool(v, "ignoreWhitespace").unwrap_or(false);
    let d = s.diff_with(&FileSel { path, area }, context, ws)?;
    serde_json::to_value(&d).map_err(|e| CmdError::Failed(e.to_string()))
}

/// Largest file `file.content` returns.
const MAX_CONTENT: usize = 8 * 1024 * 1024;

/// A file's content at a revision, in the index (`rev: "index"`) or in the work tree
/// (`rev: "worktree"`).
fn content(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?.to_string();
    let rev = p::str(v, "rev").filter(|r| !r.trim().is_empty()).unwrap_or("HEAD").trim().to_string();
    let bytes = s.file_bytes(&path, &rev)?;
    let binary = bytes.iter().take(8000).any(|b| *b == 0);
    let truncated = bytes.len() > MAX_CONTENT;
    let text = if binary { String::new() } else { String::from_utf8_lossy(bytes.get(..MAX_CONTENT).unwrap_or(&bytes)).into_owned() };
    Ok(json!({"path": path, "rev": rev, "size": bytes.len(), "binary": binary, "truncated": truncated, "text": text}))
}

fn blame(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    let rev = p::str(v, "rev").filter(|r| !r.trim().is_empty());
    let b = s.repo()?.blame(path, rev)?;
    serde_json::to_value(&b).map_err(|e| CmdError::Failed(e.to_string()))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("repo.status", "Repository Status", "Agents", |s, _| Ok(s.inspect())).pure(),
        CommandSpec::new("graph.log", "Commit History", "Agents", log)
            .params(r#"{"limit"?: number (50), "skip"?: number, "ref"?: string (default: all), "path"?: string, "grep"?: string, "author"?: string}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("commit.show", "Commit Details", "Graph", show).params(r#"{"oid"?: commit (default: selected)}"#).pure().when(need_commits),
        CommandSpec::new("diff.file", "File Diff", "Diff View", diff)
            .params(r#"{"path": string, "area"?: "unstaged" | "staged" | "commit", "oid"?: commit, "context"?: number, "ignoreWhitespace"?: bool}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("file.content", "View File", "Diff View", content)
            .params(r#"{"path": string, "rev"?: commit | "index" | "worktree" (default HEAD)}"#)
            .pure()
            .when(need_repo),
        CommandSpec::new("file.blame", "File Blame", "Diff View", blame).params(r#"{"path": string, "rev"?: commit}"#).pure().when(need_commits),
        CommandSpec::new("refs.list", "Branches and Tags", "Left Panel", |s, _| {
            serde_json::to_value(&s.data.refs)
                .map(|r| json!({"refs": r, "remotes": s.data.remotes, "stashes": s.data.stashes}))
                .map_err(|e| CmdError::Failed(e.to_string()))
        })
        .pure()
        .when(need_repo),
        CommandSpec::new("repo.activity", "Activity Log", "View", |s, _| {
            serde_json::to_value(&s.activity).map(|a| json!({"activity": a})).map_err(|e| CmdError::Failed(e.to_string()))
        })
        .pure(),
    ]
}
