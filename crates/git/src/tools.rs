//! Repository tools: ignoring files, patches (making and applying), Git LFS and hooks.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ops::Outcome;
use crate::{GitError, Repo, Result, check_name, check_path};

/// Largest patch read or written.
pub const MAX_PATCH: usize = 64 * 1024 * 1024;

/// A `.gitignore` pattern matching exactly `path` (from the repository's top folder).
pub fn ignore_pattern_for(path: &str) -> String {
    let mut out = String::from("/");
    for c in path.trim_start_matches('/').chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\' | '!' | '#') {
            out.push('\\');
        }
        out.push(c);
    }
    // A trailing space would be dropped by git unless escaped.
    if out.ends_with(' ') {
        out.pop();
        out.push_str("\\ ");
    }
    out
}

/// A hook script in the hooks folder.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    /// `pre-commit`, `commit-msg`, …
    pub name: String,
    /// It runs (an executable file under the hook's name).
    pub enabled: bool,
    /// Only the example (`pre-commit.sample`) or a disabled copy exists.
    pub sample: bool,
    pub path: String,
}

/// The hooks git knows (`githooks(5)`).
pub const HOOK_NAMES: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "post-rewrite",
    "pre-auto-gc",
    "push-to-checkout",
    "fsmonitor-watchman",
    "update",
    "pre-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "sendemail-validate",
];

fn io(p: &Path, e: std::io::Error) -> GitError {
    GitError::Io(format!("{}: {e}", p.display()))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(unix)]
fn make_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(p).map_err(|e| io(p, e))?.permissions();
    perm.set_mode(perm.mode() | 0o755);
    std::fs::set_permissions(p, perm).map_err(|e| io(p, e))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

fn check_hook(name: &str) -> Result<&str> {
    HOOK_NAMES.iter().find(|h| **h == name).copied().ok_or_else(|| GitError::Arg(format!("`{name}` is not a git hook")))
}

impl Repo {
    /// Add `pattern` to `.gitignore` (or, `local`, to `.git/info/exclude`, which isn't shared).
    /// Nothing changes when the pattern is there already. The file written.
    pub fn ignore(&self, pattern: &str, local: bool) -> Result<PathBuf> {
        let pattern = pattern.trim_end_matches(['\n', '\r']);
        if pattern.trim().is_empty() || pattern.len() > 4096 || pattern.contains(['\0', '\n', '\r']) {
            return Err(GitError::Arg("not a valid ignore pattern".into()));
        }
        let file = if local { self.git_dir.join("info").join("exclude") } else { self.root.join(".gitignore") };
        let current = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io(&file, e)),
        };
        if current.lines().any(|l| l == pattern) {
            return Ok(file);
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
        }
        let mut text = current;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(pattern);
        text.push('\n');
        std::fs::write(&file, text).map_err(|e| io(&file, e))?;
        Ok(file)
    }

    /// A patch: of commits (`git format-patch`, with authors and messages) when `rev` names a
    /// commit or a range, else of the uncommitted changes (staged ones when `staged`).
    pub fn make_patch(&self, rev: Option<&str>, staged: bool) -> Result<String> {
        let o = match rev {
            Some(r) => {
                let r = check_name("revision", r)?;
                if r.contains("..") {
                    self.run_ok(&["format-patch", "--stdout", "--no-color", r])?
                } else {
                    self.run_ok(&["format-patch", "--stdout", "--no-color", "-1", r])?
                }
            }
            None if staged => self.run_ok(&["diff", "--cached", "--binary", "--no-color", "--no-ext-diff"])?,
            None => self.run_ok(&["diff", "HEAD", "--binary", "--no-color", "--no-ext-diff"])?,
        };
        if o.stdout.is_empty() {
            return Err(GitError::Arg("there is nothing to put in a patch".into()));
        }
        if o.stdout.len() > MAX_PATCH {
            return Err(GitError::Arg("the patch is too large".into()));
        }
        Ok(o.text())
    }

    /// Apply a patch. A mail-style patch (from `format-patch`) is committed with its author and
    /// message (`git am --3way`) when `commit`; anything else is applied to the work tree and the
    /// index (`git apply --3way`), which may leave conflicts.
    pub fn apply_patch(&self, patch: &str, commit: bool) -> Result<Outcome> {
        if patch.trim().is_empty() || patch.len() > MAX_PATCH {
            return Err(GitError::Arg("the patch is empty or too large".into()));
        }
        let mail = patch.starts_with("From ") && patch.contains("\nSubject: ");
        let args: &[&str] = if mail && commit { &["am", "--3way", "--keep-cr"] } else { &["apply", "--3way", "--whitespace=nowarn", "-"] };
        let o = self.git.run_in(Some(&self.root), args, Some(patch.as_bytes()))?;
        if o.ok() {
            return Ok(Outcome { message: o.message(), conflicts: false });
        }
        if self.status().map(|s| s.conflicted().next().is_some()).unwrap_or(false) {
            return Ok(Outcome { message: o.message(), conflicts: true });
        }
        if mail && commit {
            // A failed `am` leaves its session behind: clean it up so the repository isn't stuck.
            let _ = self.run(&["am", "--abort"]);
        }
        Err(crate::runner::failed(args, &o))
    }

    /// Whether `git lfs` is installed.
    pub fn lfs_available(&self) -> bool {
        self.run(&["lfs", "version"]).is_ok_and(|o| o.ok())
    }

    fn need_lfs(&self) -> Result<()> {
        if self.lfs_available() { Ok(()) } else { Err(GitError::NotFound("git lfs is not installed (https://git-lfs.com)".into())) }
    }

    /// The patterns stored with LFS (from `.gitattributes`).
    pub fn lfs_patterns(&self) -> Vec<String> {
        let Ok(text) = std::fs::read_to_string(self.root.join(".gitattributes")) else { return Vec::new() };
        text.lines()
            .filter(|l| l.split_whitespace().skip(1).any(|a| a == "filter=lfs"))
            .filter_map(|l| l.split_whitespace().next().map(str::to_string))
            .collect()
    }

    /// Files stored with LFS in HEAD.
    pub fn lfs_files(&self) -> Result<Vec<String>> {
        self.need_lfs()?;
        Ok(self.run_ok(&["lfs", "ls-files", "--name-only"])?.text().lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
    }

    /// Store files matching `pattern` with LFS (`git lfs track`); sets up LFS in the repository.
    pub fn lfs_track(&self, pattern: &str) -> Result<()> {
        self.need_lfs()?;
        let pattern = check_name("pattern", pattern)?;
        self.run_ok(&["lfs", "install", "--local"])?;
        self.run_paths(&["lfs", "track", "--", pattern])?;
        self.run_paths(&["add", "--", ".gitattributes"]).map(|_| ())
    }

    pub fn lfs_untrack(&self, pattern: &str) -> Result<()> {
        self.need_lfs()?;
        let pattern = check_name("pattern", pattern)?;
        self.run_paths(&["lfs", "untrack", "--", pattern])?;
        self.run_paths(&["add", "--", ".gitattributes"]).map(|_| ())
    }

    /// Download the LFS files of the checked-out commit.
    pub fn lfs_pull(&self) -> Result<String> {
        self.need_lfs()?;
        Ok(self.run_ok(&["lfs", "pull"])?.message())
    }

    /// The folder hooks run from (`core.hooksPath`, else `.git/hooks`).
    pub fn hooks_dir(&self) -> PathBuf {
        match self.config("core.hooksPath") {
            Some(p) if !p.is_empty() => {
                let pb = PathBuf::from(p);
                if pb.is_absolute() { pb } else { self.root.join(pb) }
            }
            _ => self.git_dir.join("hooks"),
        }
    }

    /// Every hook that has a script, an example or a disabled copy.
    pub fn hooks(&self) -> Vec<Hook> {
        let dir = self.hooks_dir();
        HOOK_NAMES
            .iter()
            .filter_map(|n| {
                let live = dir.join(n);
                if live.is_file() {
                    return Some(Hook { name: n.to_string(), enabled: is_executable(&live), sample: false, path: live.display().to_string() });
                }
                let off = [dir.join(format!("{n}.disabled")), dir.join(format!("{n}.sample"))].into_iter().find(|p| p.is_file())?;
                Some(Hook { name: n.to_string(), enabled: false, sample: true, path: off.display().to_string() })
            })
            .collect()
    }

    /// Turn a hook on (from its disabled copy or its example) or off (kept as `<name>.disabled`).
    pub fn set_hook_enabled(&self, name: &str, on: bool) -> Result<()> {
        let name = check_hook(name)?;
        let dir = self.hooks_dir();
        let live = dir.join(name);
        let disabled = dir.join(format!("{name}.disabled"));
        if on {
            if !live.is_file() {
                let from = [disabled.clone(), dir.join(format!("{name}.sample"))]
                    .into_iter()
                    .find(|p| p.is_file())
                    .ok_or_else(|| GitError::Arg(format!("there is no {name} hook to turn on: write one first")))?;
                if from == disabled {
                    std::fs::rename(&from, &live).map_err(|e| io(&from, e))?;
                } else {
                    std::fs::copy(&from, &live).map_err(|e| io(&from, e))?;
                }
            }
            make_executable(&live)
        } else {
            if !live.is_file() {
                return Ok(());
            }
            std::fs::rename(&live, &disabled).map_err(|e| io(&live, e))
        }
    }

    /// A hook's script (the live one, else its disabled copy or example).
    pub fn read_hook(&self, name: &str) -> Result<String> {
        let name = check_hook(name)?;
        let dir = self.hooks_dir();
        let p = [dir.join(name), dir.join(format!("{name}.disabled")), dir.join(format!("{name}.sample"))]
            .into_iter()
            .find(|p| p.is_file())
            .ok_or_else(|| GitError::Arg(format!("there is no {name} hook")))?;
        let meta = std::fs::metadata(&p).map_err(|e| io(&p, e))?;
        if meta.len() > 1024 * 1024 {
            return Err(GitError::Arg("the hook is too large".into()));
        }
        std::fs::read_to_string(&p).map_err(|e| io(&p, e))
    }

    /// Write a hook's script and turn it on.
    pub fn write_hook(&self, name: &str, script: &str) -> Result<()> {
        let name = check_hook(name)?;
        if script.len() > 1024 * 1024 || script.contains('\0') {
            return Err(GitError::Arg("not a valid hook script".into()));
        }
        let dir = self.hooks_dir();
        std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
        let live = dir.join(name);
        std::fs::write(&live, script).map_err(|e| io(&live, e))?;
        let _ = std::fs::remove_file(dir.join(format!("{name}.disabled")));
        make_executable(&live)
    }

    /// Start a configured external diff or merge tool on `path` without waiting for it.
    pub fn launch_tool(&self, merge: bool, path: &str) -> Result<()> {
        let path = check_path(path)?;
        let key = if merge { "merge.tool" } else { "diff.tool" };
        if self.config(key).is_none() && !(merge && self.config("diff.tool").is_some()) {
            return Err(GitError::Arg(format!("no external tool is set: set `{key}` in your git config")));
        }
        let args: Vec<&str> = if merge { vec!["mergetool", "--no-prompt", "--", path] } else { vec!["difftool", "--no-prompt", "--", path] };
        self.git.spawn_detached(&self.root, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{commit_all, repo, sample, write};

    #[test]
    fn ignore_patterns() {
        assert_eq!(ignore_pattern_for("build/out.log"), "/build/out.log");
        assert_eq!(ignore_pattern_for("a[1]*.txt"), "/a\\[1\\]\\*.txt");
        assert_eq!(ignore_pattern_for("#x "), "/\\#x\\ ");
        let (_d, r) = sample("tools-ignore").unwrap();
        write(&r, "debug.log", "x\n").unwrap();
        write(&r, "a[1].txt", "x\n").unwrap();
        r.ignore(&ignore_pattern_for("debug.log"), false).unwrap();
        r.ignore(&ignore_pattern_for("debug.log"), false).unwrap();
        r.ignore(&ignore_pattern_for("a[1].txt"), true).unwrap();
        let gi = std::fs::read_to_string(r.root().join(".gitignore")).unwrap();
        assert_eq!(gi, "/debug.log\n");
        let st = r.status().unwrap();
        assert!(st.file("debug.log").is_none() && st.file("a[1].txt").is_none());
        assert!(r.ignore("a\nb", false).is_err());
        assert!(r.ignore("  ", false).is_err());
    }

    #[test]
    fn patches_round_trip() {
        let (d, r) = sample("tools-patch").unwrap();
        let p = r.make_patch(Some("feature"), false).unwrap();
        assert!(p.starts_with("From ") && p.contains("Subject: [PATCH] Add a library"));
        r.apply_patch(&p, true).unwrap();
        assert_eq!(r.find_commit("HEAD").unwrap().summary, "Add a library");
        // Uncommitted changes as a plain patch, applied to a clone.
        write(&r, "README.md", "patched\n").unwrap();
        let p2 = r.make_patch(None, false).unwrap();
        let other = r.git().clone_repo(&r.root().to_string_lossy(), &d.path().join("other")).unwrap();
        other.apply_patch(&p2, true).unwrap();
        assert_eq!(std::fs::read_to_string(other.root().join("README.md")).unwrap(), "patched\n");
        assert!(r.make_patch(None, true).is_err(), "nothing staged");
        assert!(r.apply_patch("", true).is_err());
        assert!(r.apply_patch("not a patch\n", false).is_err());
        assert!(r.apply_patch("From 123 Mon\nSubject: [PATCH] x\n\nnonsense", true).is_err());
        assert_eq!(r.state(), crate::RepoState::Clean, "a failed am is cleaned up");
    }

    #[test]
    fn hooks_on_off_and_written() {
        let (_d, r) = repo("tools-hooks").unwrap();
        write(&r, "a.txt", "a\n").unwrap();
        commit_all(&r, "a").unwrap();
        r.write_hook("pre-commit", "#!/bin/sh\nexit 0\n").unwrap();
        let h = r.hooks();
        let pc = h.iter().find(|x| x.name == "pre-commit").unwrap();
        assert!(pc.enabled && !pc.sample);
        r.set_hook_enabled("pre-commit", false).unwrap();
        assert!(!r.hooks().iter().find(|x| x.name == "pre-commit").unwrap().enabled);
        r.set_hook_enabled("pre-commit", true).unwrap();
        assert!(r.hooks().iter().find(|x| x.name == "pre-commit").unwrap().enabled);
        assert!(r.read_hook("pre-commit").unwrap().contains("exit 0"));
        assert!(r.set_hook_enabled("../evil", true).is_err());
        assert!(r.write_hook("not-a-hook", "x").is_err());
        assert!(r.set_hook_enabled("post-checkout", true).is_err(), "nothing to turn on");
    }

    #[test]
    fn lfs_patterns_from_attributes() {
        let (_d, r) = repo("tools-lfs").unwrap();
        write(&r, ".gitattributes", "*.psd filter=lfs diff=lfs merge=lfs -text\n*.txt text\n").unwrap();
        assert_eq!(r.lfs_patterns(), vec!["*.psd".to_string()]);
        if r.lfs_available() {
            r.lfs_track("*.bin").unwrap();
            assert!(r.lfs_patterns().contains(&"*.bin".to_string()));
            r.lfs_untrack("*.bin").unwrap();
            assert!(!r.lfs_patterns().contains(&"*.bin".to_string()));
        } else {
            assert!(r.lfs_track("*.bin").is_err());
        }
    }
}
