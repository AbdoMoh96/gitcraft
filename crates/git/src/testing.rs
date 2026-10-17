//! Throwaway repositories for tests and demos: an isolated git (no user or system config, a
//! fixed author) in a temporary folder that is removed when dropped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Git, Repo, Result};

/// A temporary folder, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Result<TempDir> {
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let p = std::env::temp_dir().join(format!("gitcraft-{tag}-{}-{}-{nanos}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&p).map_err(|e| crate::GitError::Io(e.to_string()))?;
        Ok(TempDir(p))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An isolated git whose global config lives in `dir`.
pub fn isolated_git(dir: &Path) -> Result<Git> {
    let cfg = dir.join("gitconfig");
    std::fs::write(&cfg, "[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n[pull]\n\trebase = false\n[advice]\n\tdetachedHead = false\n")
        .map_err(|e| crate::GitError::Io(e.to_string()))?;
    Ok(Git::isolated(&cfg))
}

/// A new, empty repository on `main` in a temporary folder.
pub fn repo(tag: &str) -> Result<(TempDir, Repo)> {
    let dir = TempDir::new(tag)?;
    let git = isolated_git(dir.path())?;
    let repo = git.init(&dir.path().join("work"), Some("main"))?;
    Ok((dir, repo))
}

/// Write `content` to `path` in the work tree (folders created).
pub fn write(repo: &Repo, path: &str, content: &str) -> Result<()> {
    let p = repo.root().join(path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| crate::GitError::Io(e.to_string()))?;
    }
    std::fs::write(&p, content).map_err(|e| crate::GitError::Io(e.to_string()))
}

/// Stage everything and commit; the new commit's id.
pub fn commit_all(repo: &Repo, message: &str) -> Result<String> {
    repo.stage_all()?;
    repo.commit(message, false)
}

/// A repository with a little history: two commits on `main`, a `feature` branch with one more,
/// a tag, and `main` checked out.
pub fn sample(tag: &str) -> Result<(TempDir, Repo)> {
    let (dir, repo) = repo(tag)?;
    write(&repo, "README.md", "# Sample\n\nA repository for GitCraft.\n")?;
    write(&repo, "src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n")?;
    commit_all(&repo, "Initial commit")?;
    write(&repo, "src/main.rs", "fn main() {\n    println!(\"hello, world\");\n}\n")?;
    commit_all(&repo, "Greet the world")?;
    repo.create_tag("v0.1.0", None, None)?;
    repo.create_branch("feature", None, true)?;
    write(&repo, "src/lib.rs", "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n")?;
    commit_all(&repo, "Add a library")?;
    repo.checkout_branch("main")?;
    Ok((dir, repo))
}
