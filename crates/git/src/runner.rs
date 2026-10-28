//! Running git: the program, its environment, and a repository handle.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{GitError, Result};

/// What a git run produced.
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub code: Option<i32>,
}

impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
    /// What git said, for a status line: stderr (where git reports progress and results), else stdout.
    pub fn message(&self) -> String {
        let e = self.stderr.trim();
        if e.is_empty() { self.text().trim().to_string() } else { e.to_string() }
    }
}

/// The git program and the environment it runs with.
#[derive(Clone, Debug)]
pub struct Git {
    pub bin: PathBuf,
    /// Extra environment, after GitCraft's own (tests isolate the user's config with it).
    pub env: Vec<(String, String)>,
}

impl Default for Git {
    fn default() -> Self {
        let bin = std::env::var_os("GITCRAFT_GIT").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("git"));
        Git { bin, env: Vec::new() }
    }
}

/// On Windows a GUI app has no console, so each console program it starts (git) gets a new
/// console window: it flashes, steals focus from the app (lost clicks) and costs time per run.
/// `CREATE_NO_WINDOW` runs git without one; its output still comes through the pipes.
#[cfg(windows)]
fn no_console_window(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_console_window(_: &mut Command) {}

/// Longest output we keep from one git run (a huge diff or log is cut, never allocated whole).
const MAX_OUTPUT: u64 = 256 * 1024 * 1024;

impl Git {
    /// A git that ignores the user's and the system's configuration and commits as a fixed
    /// author: reproducible runs for tests and demos. `config` is the global config file to use.
    pub fn isolated(config: &Path) -> Git {
        let mut g = Git::default();
        let c = config.to_string_lossy().into_owned();
        g.env = vec![
            ("GIT_CONFIG_GLOBAL".into(), c),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_AUTHOR_NAME".into(), "Ada Example".into()),
            ("GIT_AUTHOR_EMAIL".into(), "ada@example.com".into()),
            ("GIT_COMMITTER_NAME".into(), "Ada Example".into()),
            ("GIT_COMMITTER_EMAIL".into(), "ada@example.com".into()),
        ];
        g
    }

    fn command(&self, dir: Option<&Path>) -> Command {
        let mut c = Command::new(&self.bin);
        if let Some(d) = dir {
            c.current_dir(d);
        }
        // Machine-readable, never interactive: no prompts, no editor, no pager, no colour, and
        // no optional locks for reads running beside the user's own git.
        c.env("LC_ALL", "C")
            .env("LANGUAGE", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", ":")
            .env("GIT_SEQUENCE_EDITOR", ":")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("GIT_PAGER", "cat")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c.args(["-c", "color.ui=false", "-c", "core.quotepath=false", "-c", "log.showSignature=false"]);
        no_console_window(&mut c);
        c
    }

    /// Run git in `dir` with `args`, feeding `input` on stdin. Never fails on git's exit code;
    /// see [`Output::code`].
    pub fn run_in(&self, dir: Option<&Path>, args: &[&str], input: Option<&[u8]>) -> Result<Output> {
        self.run_full(dir, args, input, false)
    }

    /// [`Git::run_in`], with pathspecs read literally when `literal` (`GIT_LITERAL_PATHSPECS`:
    /// a file named `:x` or `*` is that file, not a pattern). Only for commands given user paths:
    /// some git commands use pathspec magic internally (`stash -u`).
    pub fn run_full(&self, dir: Option<&Path>, args: &[&str], input: Option<&[u8]>, literal: bool) -> Result<Output> {
        let mut c = self.command(dir);
        if literal {
            c.env("GIT_LITERAL_PATHSPECS", "1");
        }
        c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
        log::debug!("git {}", args.join(" "));
        let mut child = c.spawn().map_err(|e| GitError::NotFound(format!("{}: {e}", self.bin.display())))?;
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let (out, err) = std::thread::scope(|scope| {
            if let (Some(mut w), Some(bytes)) = (stdin, input) {
                scope.spawn(move || {
                    let _ = w.write_all(bytes);
                });
            }
            let err = scope.spawn(move || {
                let mut buf = Vec::new();
                if let Some(e) = stderr.as_mut() {
                    let _ = e.take(MAX_OUTPUT).read_to_end(&mut buf);
                }
                buf
            });
            let mut out = Vec::new();
            if let Some(o) = stdout.as_mut() {
                let _ = o.take(MAX_OUTPUT).read_to_end(&mut out);
            }
            (out, err.join().unwrap_or_default())
        });
        let status = child.wait().map_err(|e| GitError::Io(e.to_string()))?;
        Ok(Output { stdout: out, stderr: String::from_utf8_lossy(&err).into_owned(), code: status.code() })
    }

    /// Start git (an external tool it launches) and don't wait: its output is dropped and it is
    /// reaped on a thread when it exits.
    pub fn spawn_detached(&self, dir: &Path, args: &[&str]) -> Result<()> {
        let mut c = self.command(Some(dir));
        c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = c.spawn().map_err(|e| GitError::NotFound(format!("{}: {e}", self.bin.display())))?;
        let _ = std::thread::Builder::new().name("gitcraft tool".into()).spawn(move || child.wait());
        Ok(())
    }

    /// Run and require success.
    pub fn run_ok(&self, dir: Option<&Path>, args: &[&str], input: Option<&[u8]>) -> Result<Output> {
        let o = self.run_in(dir, args, input)?;
        if o.ok() { Ok(o) } else { Err(failed(args, &o)) }
    }

    /// `git --version`.
    pub fn version(&self) -> Result<String> {
        Ok(self.run_ok(None, &["--version"], None)?.text().trim().to_string())
    }

    /// `git init` at `path` (created when missing), then open it.
    pub fn init(&self, path: &Path, initial_branch: Option<&str>) -> Result<Repo> {
        std::fs::create_dir_all(path).map_err(|e| GitError::Io(format!("{}: {e}", path.display())))?;
        let p = path.to_string_lossy().into_owned();
        let mut args = vec!["init"];
        if let Some(b) = initial_branch {
            args.extend(["-b", crate::check_name("branch", b)?]);
        }
        args.extend(["--", p.as_str()]);
        self.run_ok(None, &args, None)?;
        self.open(path)
    }

    /// `git clone url path`, then open it.
    pub fn clone_repo(&self, url: &str, path: &Path) -> Result<Repo> {
        let url = crate::check_name("url", url)?;
        let p = path.to_string_lossy().into_owned();
        self.run_ok(None, &["clone", "--progress", "--", url, p.as_str()], None)?;
        self.open(path)
    }

    /// The repository containing `path`.
    pub fn open(&self, path: &Path) -> Result<Repo> {
        let o = self.run_in(Some(path), &["rev-parse", "--show-toplevel", "--absolute-git-dir"], None);
        let o = match o {
            Ok(o) if o.ok() => o,
            Ok(o) => return Err(GitError::NotARepo(format!("{}: {}", path.display(), o.message()))),
            Err(GitError::Io(e)) => return Err(GitError::NotARepo(format!("{}: {e}", path.display()))),
            Err(e) => return Err(e),
        };
        let text = o.text();
        let mut lines = text.lines();
        let (Some(top), Some(git_dir)) = (lines.next(), lines.next()) else {
            return Err(GitError::NotARepo(format!("{} (a bare repository or no work tree)", path.display())));
        };
        Ok(Repo { root: PathBuf::from(top), git_dir: PathBuf::from(git_dir), git: self.clone() })
    }
}

pub(crate) fn failed(args: &[&str], o: &Output) -> GitError {
    let msg = o.message();
    let stderr = if msg.is_empty() { format!("git {} failed", args.first().copied().unwrap_or("")) } else { msg };
    GitError::Failed { cmd: format!("git {}", args.join(" ")), code: o.code, stderr }
}

/// An open repository with a work tree.
#[derive(Clone, Debug)]
pub struct Repo {
    pub(crate) root: PathBuf,
    pub(crate) git_dir: PathBuf,
    pub(crate) git: Git,
}

impl Repo {
    /// The work tree's top folder.
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }
    pub fn git(&self) -> &Git {
        &self.git
    }
    /// The folder's name, as the tab and title show it.
    pub fn name(&self) -> String {
        self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.root.display().to_string())
    }

    /// Run git in the work tree; any exit code.
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        self.git.run_in(Some(&self.root), args, None)
    }
    /// Run and require success.
    pub fn run_ok(&self, args: &[&str]) -> Result<Output> {
        self.git.run_ok(Some(&self.root), args, None)
    }
    /// Run with user paths as literal pathspecs, and require success.
    pub fn run_paths(&self, args: &[&str]) -> Result<Output> {
        let o = self.git.run_full(Some(&self.root), args, None, true)?;
        if o.ok() { Ok(o) } else { Err(failed(args, &o)) }
    }
    /// Run with stdin and require success.
    pub fn run_input(&self, args: &[&str], input: &[u8]) -> Result<Output> {
        self.git.run_ok(Some(&self.root), args, Some(input))
    }
    /// Stdout of a successful run, as text.
    pub fn text(&self, args: &[&str]) -> Result<String> {
        Ok(self.run_ok(args)?.text())
    }

    /// The commit `rev` names, or `None` (an unborn branch, an unknown name).
    pub fn resolve(&self, rev: &str) -> Option<String> {
        let rev = crate::check_name("revision", rev).ok()?;
        let spec = format!("{rev}^{{commit}}");
        let o = self.run(&["rev-parse", "--verify", "--quiet", spec.as_str()]).ok()?;
        o.ok().then(|| o.text().trim().to_string()).filter(|s| !s.is_empty())
    }

    /// Whether HEAD points at a commit (false in a new repository).
    pub fn has_commits(&self) -> bool {
        self.resolve("HEAD").is_some()
    }

    /// A config value (`user.name`), if set.
    pub fn config(&self, key: &str) -> Option<String> {
        let key = crate::check_name("config key", key).ok()?;
        let o = self.run(&["config", "--get", key]).ok()?;
        o.ok().then(|| o.text().trim().to_string())
    }

    /// Every config value whose key matches the regular expression `pattern` (keys lower-cased by
    /// git), in one run; the last value of a key wins.
    pub fn config_matching(&self, pattern: &str) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        if pattern.starts_with('-') || pattern.contains(['\0', '\n']) {
            return out;
        }
        let Ok(o) = self.run(&["config", "-z", "--get-regexp", pattern]) else { return out };
        for rec in String::from_utf8_lossy(&o.stdout).split('\0') {
            if let Some((k, v)) = rec.split_once('\n') {
                out.insert(k.to_string(), v.to_string());
            } else if !rec.is_empty() {
                out.insert(rec.to_string(), String::new());
            }
        }
        out
    }

    /// A config value from one scope only (`local`, `global`), if set there.
    pub fn config_in(&self, scope: ConfigScope, key: &str) -> Option<String> {
        let key = check_config_key(key).ok()?;
        let o = self.run(&["config", scope.flag(), "--get", key]).ok()?;
        o.ok().then(|| o.text().trim().to_string())
    }

    /// Set (or with `None` remove) a config value in `scope`.
    pub fn set_config(&self, scope: ConfigScope, key: &str, value: Option<&str>) -> Result<()> {
        let key = check_config_key(key)?;
        match value {
            Some(v) => {
                if v.len() > 4096 || v.contains(['\0', '\n', '\r']) {
                    return Err(GitError::Arg(format!("`{key}` can't hold that value")));
                }
                // `--` so a value starting with `-` is a value.
                self.run_ok(&["config", scope.flag(), "--", key, v])?;
            }
            None => {
                let o = self.run(&["config", scope.flag(), "--unset-all", key])?;
                // 5: it wasn't set.
                if !o.ok() && o.code != Some(5) {
                    return Err(failed(&["config", "--unset-all", key], &o));
                }
            }
        }
        Ok(())
    }
}

/// Which config file a setting is read from or written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigScope {
    /// This repository (`.git/config`).
    Local,
    /// The user's (`~/.gitconfig`).
    Global,
}

impl ConfigScope {
    pub fn parse(s: &str) -> Option<ConfigScope> {
        match s.trim().to_ascii_lowercase().as_str() {
            "local" | "repository" | "repo" => Some(ConfigScope::Local),
            "global" | "user" => Some(ConfigScope::Global),
            _ => None,
        }
    }
    fn flag(self) -> &'static str {
        match self {
            ConfigScope::Local => "--local",
            ConfigScope::Global => "--global",
        }
    }
}

/// `section.name` or `section.sub.name`: letters, digits, `-`, `.` (and anything in a subsection
/// but line breaks); never an option.
pub fn check_config_key(key: &str) -> Result<&str> {
    let k = crate::check_name("config key", key)?;
    let ok = k.contains('.')
        && k.split('.').next().is_some_and(|sec| !sec.is_empty() && sec.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && k.rsplit('.').next().is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    if ok { Ok(k) } else { Err(GitError::Arg(format!("`{k}` is not a config key"))) }
}
