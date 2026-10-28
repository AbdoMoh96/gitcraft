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

/// A small repository to learn with: a README, two commits and a branch with an idea on it.
fn practice(s: &mut Session, v: &Value) -> CmdResult {
    let path = PathBuf::from(p::req_str(v, "path")?.trim());
    if path.exists() && std::fs::read_dir(&path).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(crate::CmdError::Params(format!("{} exists and isn't empty: pick a new folder", path.display())));
    }
    let repo = s.git.init(&path, Some("main"))?;
    // Commits need an author: a learner without one set gets a placeholder for this repository.
    if repo.config("user.name").is_none() {
        repo.set_config(gitcraft_git::ConfigScope::Local, "user.name", Some("Git Learner"))?;
    }
    if repo.config("user.email").is_none() {
        repo.set_config(gitcraft_git::ConfigScope::Local, "user.email", Some("learner@example.com"))?;
    }
    let write = |name: &str, text: &str| -> Result<(), crate::CmdError> {
        let f = repo.root().join(name);
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d).map_err(|e| crate::CmdError::Failed(e.to_string()))?;
        }
        std::fs::write(&f, text).map_err(|e| crate::CmdError::Failed(format!("{name}: {e}")))
    };
    let commit = |msg: &str| -> Result<String, crate::CmdError> {
        repo.stage_all()?;
        Ok(repo.commit(msg, false)?)
    };
    write("README.md", "# Practice repository\n\nA place to try GitCraft without worrying about breaking anything.\n")?;
    commit("Start the practice repository")?;
    write("notes/ideas.md", "# Ideas\n\n- Learn to stage part of a file\n")?;
    commit("Write down some ideas")?;
    repo.create_branch("try-something", None, true)?;
    write("notes/ideas.md", "# Ideas\n\n- Learn to stage part of a file\n- Merge a branch\n")?;
    commit("Add an idea on a branch")?;
    repo.checkout_branch("main")?;
    write(
        "README.md",
        "# Practice repository\n\nA place to try GitCraft without worrying about breaking anything.\n\nEdit me, then stage and commit the change.\n",
    )?;
    s.set_repo(repo)?;
    let root = s.repo()?.root().display().to_string();
    Ok(json!({"message": format!("Created a practice repository in {root}"), "path": root}))
}

fn close(s: &mut Session, _: &Value) -> CmdResult {
    s.close();
    super::done("Closed the repository")
}

fn refresh(s: &mut Session, _: &Value) -> CmdResult {
    s.refresh()?;
    super::done("Reloaded")
}

/// Folders scanned at most, and how deep.
const MAX_SCANNED: usize = 20_000;
const MAX_DEPTH: usize = 6;

/// Folders under `root` (to `depth` levels) that are git repositories; hidden folders and build
/// output are skipped, links aren't followed.
pub fn scan(root: &std::path::Path, depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((dir, d)) = stack.pop() {
        seen += 1;
        if seen > MAX_SCANNED {
            break;
        }
        if dir.join(".git").exists() {
            found.push(dir);
            // A repository's own folders are its business (submodules are listed there).
            continue;
        }
        if d >= depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut subs: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                !n.starts_with('.') && !matches!(n.as_str(), "node_modules" | "target" | "build" | "dist" | "vendor" | "Library" | "AppData")
            })
            .map(|e| e.path())
            .collect();
        subs.sort();
        stack.extend(subs.into_iter().rev().map(|p| (p, d + 1)));
    }
    found.sort();
    found
}

fn scan_cmd(_: &mut Session, v: &Value) -> CmdResult {
    let root = PathBuf::from(p::req_str(v, "path")?.trim());
    if !root.is_dir() {
        return Err(crate::CmdError::Params(format!("{} is not a folder", root.display())));
    }
    let depth = p::usize(v, "depth").unwrap_or(3).clamp(1, MAX_DEPTH);
    let repos: Vec<String> = scan(&root, depth).into_iter().map(|p| p.display().to_string()).collect();
    Ok(json!({"message": format!("Found {} repositor{}", repos.len(), if repos.len() == 1 { "y" } else { "ies" }), "repositories": repos}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("repo.open", "Open Repository", "File", open).key("Mod+O").params(r#"{"path": string}"#).pure(),
        CommandSpec::new("repo.init", "Init Repository", "File", init).params(r#"{"path": string, "branch"?: string}"#).pure(),
        CommandSpec::new("repo.clone", "Clone Repository", "File", clone)
            .params(r#"{"url": string, "path"?: string (default: <into>/<name>), "into"?: string}"#)
            .pure()
            .slow(),
        CommandSpec::new("repo.practice", "Practice Repository", "File", practice).params(r#"{"path": new folder}"#).pure(),
        CommandSpec::new("repo.scan", "Find Repositories", "File", scan_cmd).params(r#"{"path": folder, "depth"?: 1–6 (3)}"#).pure(),
        CommandSpec::new("repo.close", "Close Repository", "File", close).key("Mod+W").pure().when(need_repo),
        CommandSpec::new("repo.refresh", "Refresh", "View", refresh).key("F5").pure().when(need_repo),
    ]
}
