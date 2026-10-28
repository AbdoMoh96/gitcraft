//! Submodules (`git submodule`) and linked worktrees (`git worktree`): listing them and the
//! operations on them.

use serde::Serialize;

use crate::{GitError, Repo, Result, check_name, check_path};

/// A submodule's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SubmoduleState {
    /// Checked out at the commit the repository records.
    Clean,
    /// Not checked out yet (`git submodule update --init`).
    Uninitialized,
    /// Checked out at another commit than the one recorded.
    Modified,
    /// In conflict in a merge.
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Submodule {
    pub path: String,
    pub name: String,
    pub url: Option<String>,
    /// The commit checked out (or recorded, when not checked out).
    pub oid: String,
    pub state: SubmoduleState,
}

/// Parse `git submodule status` (one line per submodule: a state character, the commit, the path
/// and, in brackets, a description).
pub fn parse_submodule_status(text: &str) -> Vec<(SubmoduleState, String, String)> {
    text.lines()
        .filter_map(|l| {
            let mut chars = l.chars();
            let state = match chars.next()? {
                ' ' => SubmoduleState::Clean,
                '-' => SubmoduleState::Uninitialized,
                '+' => SubmoduleState::Modified,
                'U' => SubmoduleState::Conflict,
                _ => return None,
            };
            let rest = chars.as_str();
            let (oid, rest) = rest.split_once(' ')?;
            if oid.is_empty() || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            // The path runs up to the ` (describe)` suffix, if any.
            let path = match rest.rfind(" (") {
                Some(i) if rest.ends_with(')') => rest.get(..i)?,
                _ => rest,
            };
            Some((state, oid.to_string(), path.to_string()))
        })
        .collect()
}

/// A linked (or the main) work tree.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub head: Option<String>,
    /// The checked-out branch (short name); `None` when detached.
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    /// Its folder is gone: `git worktree prune` removes it.
    pub prunable: bool,
    /// The repository's main work tree (the first one listed).
    pub main: bool,
}

/// Parse `git worktree list --porcelain`.
pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out: Vec<Worktree> = Vec::new();
    for block in text.split("\n\n") {
        let mut w = Worktree::default();
        let mut any = false;
        for line in block.lines() {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            match k {
                "worktree" => {
                    w.path = v.to_string();
                    any = true;
                }
                "HEAD" => w.head = Some(v.to_string()),
                "branch" => w.branch = Some(v.strip_prefix("refs/heads/").unwrap_or(v).to_string()),
                "bare" => w.bare = true,
                "detached" => w.detached = true,
                "locked" => w.locked = true,
                "prunable" => w.prunable = true,
                _ => {}
            }
        }
        if any && !w.path.is_empty() {
            w.main = out.is_empty();
            out.push(w);
        }
    }
    out
}

impl Repo {
    /// The submodules, with their URLs from `.gitmodules`. Empty when there are none.
    pub fn submodules(&self) -> Result<Vec<Submodule>> {
        if !self.root.join(".gitmodules").is_file() {
            return Ok(Vec::new());
        }
        let status = self.run_ok(&["submodule", "status"])?.text();
        let config = self.run(&["config", "-f", ".gitmodules", "-z", "--get-regexp", r"^submodule\..*\.(path|url)$"])?;
        let mut names: Vec<(String, String)> = Vec::new();
        let mut urls: Vec<(String, String)> = Vec::new();
        for rec in String::from_utf8_lossy(&config.stdout).split('\0') {
            let Some((k, v)) = rec.split_once('\n') else { continue };
            let Some(inner) = k.strip_prefix("submodule.") else { continue };
            if let Some(name) = inner.strip_suffix(".path") {
                names.push((v.to_string(), name.to_string()));
            } else if let Some(name) = inner.strip_suffix(".url") {
                urls.push((name.to_string(), v.to_string()));
            }
        }
        Ok(parse_submodule_status(&status)
            .into_iter()
            .map(|(state, oid, path)| {
                let name = names.iter().find(|(p, _)| *p == path).map(|(_, n)| n.clone()).unwrap_or_else(|| path.clone());
                let url = urls.iter().find(|(n, _)| *n == name).map(|(_, u)| u.clone());
                Submodule { path, name, url, oid, state }
            })
            .collect())
    }

    /// Check out the recorded commit of the given submodules (all when empty), cloning them first
    /// when `init`.
    pub fn submodule_update(&self, paths: &[String], init: bool, recursive: bool) -> Result<String> {
        let mut args = vec!["submodule", "update", "--progress"];
        if init {
            args.push("--init");
        }
        if recursive {
            args.push("--recursive");
        }
        args.push("--");
        for p in paths {
            args.push(check_path(p)?);
        }
        Ok(self.run_paths(&args)?.message())
    }

    /// Add `url` as a submodule at `path`.
    pub fn submodule_add(&self, url: &str, path: &str) -> Result<String> {
        let url = check_name("url", url)?;
        let path = check_path(path)?;
        Ok(self.run_ok(&["submodule", "add", "--progress", "--", url, path])?.message())
    }

    /// Copy the URLs in `.gitmodules` to the repository's config (after they change).
    pub fn submodule_sync(&self) -> Result<()> {
        self.run_ok(&["submodule", "sync", "--recursive"]).map(|_| ())
    }

    /// Remove a submodule: unregister it, delete its folder and its `.gitmodules` entry (staged).
    pub fn submodule_remove(&self, path: &str) -> Result<()> {
        let path = check_path(path)?;
        if !self.submodules()?.iter().any(|s| s.path == path) {
            return Err(GitError::Arg(format!("`{path}` is not a submodule")));
        }
        self.run_paths(&["submodule", "deinit", "-f", "--", path])?;
        self.run_paths(&["rm", "-f", "-q", "--", path])?;
        Ok(())
    }

    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        Ok(parse_worktrees(&self.run_ok(&["worktree", "list", "--porcelain"])?.text()))
    }

    /// A new work tree at `path` with `branch` checked out; `new_branch` creates it from `start`
    /// (HEAD) first. With no branch, `path`'s folder name is used as a new branch.
    pub fn worktree_add(&self, path: &str, branch: Option<&str>, new_branch: bool, start: Option<&str>) -> Result<()> {
        let path = check_path(path)?;
        if path.starts_with('-') {
            return Err(GitError::Arg("the folder may not start with `-`".into()));
        }
        let mut args = vec!["worktree", "add"];
        let b;
        match branch {
            Some(x) if new_branch => {
                b = self.check_branch_name(x)?;
                args.extend(["-b", b.as_str(), path]);
                if let Some(s) = start {
                    args.push(check_name("start point", s)?);
                }
            }
            Some(x) => {
                args.extend([path, check_name("branch", x)?]);
            }
            None => args.push(path),
        }
        self.run_ok(&args).map(|_| ())
    }

    /// Remove a linked work tree (`force`: even with changes in it).
    pub fn worktree_remove(&self, path: &str, force: bool) -> Result<()> {
        let path = check_path(path)?;
        if self.worktrees()?.iter().any(|w| w.main && w.path == path) {
            return Err(GitError::Arg("the main work tree can't be removed".into()));
        }
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.extend(["--", path]);
        self.run_ok(&args).map(|_| ())
    }

    /// Forget work trees whose folders are gone.
    pub fn worktree_prune(&self) -> Result<()> {
        self.run_ok(&["worktree", "prune"]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{commit_all, sample, write};

    #[test]
    fn parses_status_and_worktrees() {
        let v = parse_submodule_status(" 1234abcd lib/a (v1.0)\n-5678abcd lib/b\n+9abc0000 lib/with space (heads/main)\nUabcd1234 c\nxjunk\n");
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], (SubmoduleState::Clean, "1234abcd".into(), "lib/a".into()));
        assert_eq!(v[1].0, SubmoduleState::Uninitialized);
        assert_eq!(v[2].2, "lib/with space");
        let w = parse_worktrees(
            "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /r2\nHEAD def\ndetached\nlocked reason\nprunable gitdir file points to non-existent location\n\n",
        );
        assert_eq!(w.len(), 2);
        assert!(w[0].main && !w[1].main);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert!(w[1].detached && w[1].locked && w[1].prunable);
        for junk in ["", "\n\n\n", "worktree", "HEAD x", " ", "+", "U"] {
            let _ = parse_worktrees(junk);
            let _ = parse_submodule_status(junk);
        }
    }

    #[test]
    fn worktrees_add_list_remove() {
        let (d, r) = sample("mod-worktree").unwrap();
        let wt = d.path().join("wt-feature").to_string_lossy().into_owned();
        r.worktree_add(&wt, Some("feature"), false, None).unwrap();
        let wt2 = d.path().join("wt-new").to_string_lossy().into_owned();
        r.worktree_add(&wt2, Some("topic"), true, Some("v0.1.0")).unwrap();
        let list = r.worktrees().unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|w| w.branch.as_deref() == Some("topic")));
        assert!(r.worktree_remove(&list[0].path, false).is_err(), "the main one");
        r.worktree_remove(&wt, false).unwrap();
        std::fs::remove_dir_all(&wt2).unwrap();
        r.worktree_prune().unwrap();
        assert_eq!(r.worktrees().unwrap().len(), 1);
        assert!(r.worktree_add("-x", None, false, None).is_err());
    }

    #[test]
    fn submodules_add_list_update_remove() {
        let (d, r) = sample("mod-sub").unwrap();
        let (_d2, lib) = sample("mod-sub-lib").unwrap();
        // Local file URLs need protocol.file.allow for submodules (git 2.38.1+).
        let mut git = r.git().clone();
        git.env.push(("GIT_CONFIG_COUNT".into(), "1".into()));
        git.env.push(("GIT_CONFIG_KEY_0".into(), "protocol.file.allow".into()));
        git.env.push(("GIT_CONFIG_VALUE_0".into(), "always".into()));
        let r = git.open(r.root()).unwrap();
        assert!(r.submodules().unwrap().is_empty());
        r.submodule_add(&lib.root().to_string_lossy(), "vendor/lib").unwrap();
        commit_all(&r, "Add a submodule").unwrap();
        let subs = r.submodules().unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].path, "vendor/lib");
        assert_eq!(subs[0].state, SubmoduleState::Clean);
        assert!(subs[0].url.is_some());
        // A fresh clone has it uninitialized until updated.
        let clone = git.clone_repo(&r.root().to_string_lossy(), &d.path().join("clone")).unwrap();
        assert_eq!(clone.submodules().unwrap()[0].state, SubmoduleState::Uninitialized);
        clone.submodule_update(&[], true, true).unwrap();
        assert_eq!(clone.submodules().unwrap()[0].state, SubmoduleState::Clean);
        write(&r, "x.txt", "x\n").unwrap();
        r.submodule_sync().unwrap();
        r.submodule_remove("vendor/lib").unwrap();
        assert!(!r.root().join("vendor/lib").exists());
        assert!(r.submodule_remove("nope").is_err());
    }
}
