//! GitCraft's git layer.
//!
//! GitCraft drives the real `git` command line (as the incumbent does with the git it bundles):
//! every read is a porcelain command whose machine-readable output is parsed here, and every write
//! is a git command with checked arguments. Nothing in this crate opens a window or a terminal;
//! git never prompts (`GIT_TERMINAL_PROMPT=0`, no editor).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod blame;
pub mod conflict;
pub mod diff;
pub mod log;
pub mod ops;
pub mod patch;
pub mod rebase;
pub mod refs;
pub mod remote;
mod runner;
pub mod stash;
pub mod status;
pub mod testing;

pub use blame::{Blame, BlameCommit, BlameLine};
pub use conflict::Chunk;
pub use diff::{DiffLine, FileDiff, Hunk, LineKind};
pub use log::{Commit, LogQuery, Signature};
pub use ops::{Outcome, PullMode, ResetMode, Reworded};
pub use rebase::{TodoAction, TodoItem};
pub use refs::{Ref, RefKind};
pub use remote::Remote;
pub use runner::{Git, Output, Repo};
pub use stash::Stash;
pub use status::{Change, FileStatus, RepoState, Status};

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum GitError {
    /// The git program could not be started.
    #[error("git could not be run ({0}); install git or set GITCRAFT_GIT to its path")]
    NotFound(String),
    #[error("not a git repository: {0}")]
    NotARepo(String),
    /// Git ran and failed; `stderr` is its message.
    #[error("{stderr}")]
    Failed { cmd: String, code: Option<i32>, stderr: String },
    #[error("bad argument: {0}")]
    Arg(String),
    #[error("unexpected git output: {0}")]
    Parse(String),
    #[error("{0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// Longest name, path or message we pass to git.
pub const MAX_ARG: usize = 64 * 1024;

/// A name we pass to git as a positional argument (branch, remote, tag, revision): non-empty,
/// no leading `-` (it would be read as an option), no NUL or line breaks, bounded.
pub fn check_name<'a>(what: &str, s: &'a str) -> Result<&'a str> {
    let t = s.trim();
    if t.is_empty() {
        return Err(GitError::Arg(format!("{what} is empty")));
    }
    if t.starts_with('-') {
        return Err(GitError::Arg(format!("{what} `{t}` may not start with `-`")));
    }
    if t.len() > 1024 || t.contains(['\0', '\n', '\r']) {
        return Err(GitError::Arg(format!("{what} is not a valid name")));
    }
    Ok(t)
}

/// A path inside the work tree, passed after `--` (as a literal pathspec: [`Repo::run_paths`]).
pub fn check_path(s: &str) -> Result<&str> {
    if s.is_empty() || s.len() > 4096 || s.contains('\0') {
        return Err(GitError::Arg("not a valid path".into()));
    }
    Ok(s)
}

/// A full or abbreviated object id (hex, 4–64 digits).
pub fn check_oid(s: &str) -> Result<&str> {
    let t = s.trim();
    if (4..=64).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(t)
    } else {
        Err(GitError::Arg(format!("`{t}` is not a commit id")))
    }
}

/// Undo git's C-style quoting of a path (`"a\tb\303\251"`), as diff headers print unusual names.
pub fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) else { return s.to_string() };
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let b = inner.as_bytes();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        let Some(&n) = b.get(i + 1) else { break };
        i += 2;
        match n {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'0'..=b'7' => {
                let mut v: u32 = u32::from(n - b'0');
                for _ in 0..2 {
                    match b.get(i) {
                        Some(&d @ b'0'..=b'7') => {
                            v = v * 8 + u32::from(d - b'0');
                            i += 1;
                        }
                        _ => break,
                    }
                }
                out.push((v & 0xff) as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked() {
        assert!(check_name("branch", "feature/x").is_ok());
        assert!(check_name("branch", "--force").is_err());
        assert!(check_name("branch", "").is_err());
        assert!(check_name("branch", "a\nb").is_err());
        assert!(check_oid("abc123").is_ok());
        assert!(check_oid("xyz").is_err());
        assert!(check_oid("-abc").is_err());
    }

    #[test]
    fn quoted_paths() {
        assert_eq!(unquote("plain.txt"), "plain.txt");
        assert_eq!(unquote("\"a\\tb\""), "a\tb");
        assert_eq!(unquote("\"caf\\303\\251\""), "café");
        assert_eq!(unquote("\"q\\\"x\""), "q\"x");
        // Truncated escapes never panic.
        assert_eq!(unquote("\"abc\\\""), "abc");
    }
}
