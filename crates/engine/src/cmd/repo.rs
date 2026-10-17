//! Opening, creating, cloning, closing and reloading repositories.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::need_repo;
use crate::{CmdResult, CommandSpec, Session, p};

fn open(s: &mut Session, v: &Value) -> CmdResult {
    let path = PathBuf::from(p::req_str(v, "path")?);
    s.open(&path)?;
    Ok(json!({"message": format!("Opened {}", s.repo()?.name()), "path": s.repo()?.root().display().to_string()}))
}

fn init(s: &mut Session, v: &Value) -> CmdResult {
    let path = PathBuf::from(p::req_str(v, "path")?);
    let branch = p::str(v, "branch").filter(|b| !b.trim().is_empty());
    let repo = s.git.init(&path, branch)?;
    s.set_repo(repo)?;
    Ok(json!({"message": format!("Created a repository in {}", path.display()), "path": s.repo()?.root().display().to_string()}))
}

fn clone(s: &mut Session, v: &Value) -> CmdResult {
    let url = p::req_str(v, "url")?;
    let path = match p::str(v, "path").filter(|x| !x.trim().is_empty()) {
        Some(x) => PathBuf::from(x),
        None => {
            let name = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("repository").trim_end_matches(".git");
            let base = p::str(v, "into").map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            base.join(if name.is_empty() { "repository" } else { name })
        }
    };
    let repo = s.git.clone_repo(url, &path)?;
    let root = repo.root().display().to_string();
    // A background worker's session has nothing to show: the app opens `open` itself.
    if !s.is_detached() {
        s.set_repo(repo)?;
    }
    Ok(json!({"message": format!("Cloned {url}"), "path": root, "open": root}))
}

fn close(s: &mut Session, _: &Value) -> CmdResult {
    s.close();
    super::done("Closed the repository")
}

fn refresh(s: &mut Session, _: &Value) -> CmdResult {
    s.refresh()?;
    super::done("Reloaded")
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("repo.open", "Open Repository", "File", open).key("Mod+O").params(r#"{"path": string}"#).pure(),
        CommandSpec::new("repo.init", "Init Repository", "File", init).params(r#"{"path": string, "branch"?: string}"#).pure(),
        CommandSpec::new("repo.clone", "Clone Repository", "File", clone)
            .params(r#"{"url": string, "path"?: string (default: <into>/<name>), "into"?: string}"#)
            .pure()
            .slow(),
        CommandSpec::new("repo.close", "Close Repository", "File", close).key("Mod+W").pure().when(need_repo),
        CommandSpec::new("repo.refresh", "Refresh", "View", refresh).key("F5").pure().when(need_repo),
    ]
}
