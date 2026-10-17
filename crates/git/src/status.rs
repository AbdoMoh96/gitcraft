//! The work tree and index: `git status --porcelain=v2 --branch -z`.

use serde::Serialize;

use crate::{GitError, Repo, Result};

/// How one side (index or work tree) of a file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
    Unmerged,
}

impl Change {
    fn from_code(c: char) -> Change {
        match c {
            'M' => Change::Modified,
            'A' => Change::Added,
            'D' => Change::Deleted,
            'R' => Change::Renamed,
            'C' => Change::Copied,
            'T' => Change::TypeChanged,
            'U' => Change::Unmerged,
            '?' => Change::Untracked,
            '!' => Change::Ignored,
            _ => Change::Unmodified,
        }
    }
    /// One-letter badge for file lists.
    pub fn letter(self) -> &'static str {
        match self {
            Change::Unmodified => "",
            Change::Modified => "M",
            Change::Added | Change::Untracked => "A",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Copied => "C",
            Change::TypeChanged => "T",
            Change::Ignored => "I",
            Change::Unmerged => "U",
        }
    }
}

/// One changed file.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    /// The name before a rename or copy.
    pub orig_path: Option<String>,
    pub index: Change,
    pub worktree: Change,
    /// The unmerged state (`UU`, `AA`, `DU`…) when the file is in conflict.
    pub conflict: Option<String>,
}

impl FileStatus {
    pub fn is_staged(&self) -> bool {
        self.conflict.is_none() && !matches!(self.index, Change::Unmodified | Change::Untracked | Change::Ignored)
    }
    pub fn is_unstaged(&self) -> bool {
        self.conflict.is_some() || !matches!(self.worktree, Change::Unmodified | Change::Ignored)
    }
    pub fn is_untracked(&self) -> bool {
        self.worktree == Change::Untracked
    }
}

/// An operation in progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RepoState {
    #[default]
    Clean,
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
    Bisecting,
}

impl RepoState {
    pub fn label(self) -> &'static str {
        match self {
            RepoState::Clean => "",
            RepoState::Merging => "Merging",
            RepoState::Rebasing => "Rebasing",
            RepoState::CherryPicking => "Cherry-picking",
            RepoState::Reverting => "Reverting",
            RepoState::Bisecting => "Bisecting",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The checked-out branch (`None` when HEAD is detached).
    pub branch: Option<String>,
    /// HEAD's commit (`None` before the first commit).
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileStatus>,
    pub state: RepoState,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|f| f.is_staged())
    }
    pub fn unstaged(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|f| f.is_unstaged())
    }
    pub fn conflicted(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|f| f.conflict.is_some())
    }
    pub fn is_clean(&self) -> bool {
        self.files.is_empty()
    }
    pub fn file(&self, path: &str) -> Option<&FileStatus> {
        self.files.iter().find(|f| f.path == path)
    }
}

/// Parse `git status --porcelain=v2 --branch -z` output.
pub fn parse(raw: &[u8]) -> Result<Status> {
    let text = String::from_utf8_lossy(raw);
    let mut st = Status::default();
    let mut tokens = text.split('\0');
    while let Some(tok) = tokens.next() {
        if tok.is_empty() {
            continue;
        }
        if let Some(h) = tok.strip_prefix("# ") {
            branch_header(&mut st, h);
            continue;
        }
        let kind = tok.as_bytes().first().copied().unwrap_or(b' ');
        match kind {
            b'1' => {
                let f: Vec<&str> = tok.splitn(9, ' ').collect();
                let (Some(xy), Some(path)) = (f.get(1), f.get(8)) else { return Err(GitError::Parse(format!("status entry `{tok}`"))) };
                st.files.push(entry(xy, path, None, None));
            }
            b'2' => {
                let f: Vec<&str> = tok.splitn(10, ' ').collect();
                let (Some(xy), Some(path)) = (f.get(1), f.get(9)) else { return Err(GitError::Parse(format!("status entry `{tok}`"))) };
                let orig = tokens.next().map(str::to_string);
                st.files.push(entry(xy, path, orig, None));
            }
            b'u' => {
                let f: Vec<&str> = tok.splitn(11, ' ').collect();
                let (Some(xy), Some(path)) = (f.get(1), f.get(10)) else { return Err(GitError::Parse(format!("status entry `{tok}`"))) };
                st.files.push(entry(xy, path, None, Some(xy.to_string())));
            }
            b'?' => {
                let path = tok.get(2..).unwrap_or("");
                st.files.push(FileStatus {
                    path: path.to_string(),
                    orig_path: None,
                    index: Change::Unmodified,
                    worktree: Change::Untracked,
                    conflict: None,
                });
            }
            // Ignored files (`!`) are only listed when asked for; skip them.
            _ => {}
        }
    }
    Ok(st)
}

fn entry(xy: &str, path: &str, orig: Option<String>, conflict: Option<String>) -> FileStatus {
    let mut c = xy.chars();
    let x = c.next().unwrap_or('.');
    let y = c.next().unwrap_or('.');
    let (index, worktree) = if conflict.is_some() { (Change::Unmerged, Change::Unmerged) } else { (Change::from_code(x), Change::from_code(y)) };
    FileStatus { path: path.to_string(), orig_path: orig, index, worktree, conflict }
}

fn branch_header(st: &mut Status, h: &str) {
    if let Some(v) = h.strip_prefix("branch.oid ") {
        st.head = (v != "(initial)").then(|| v.to_string());
    } else if let Some(v) = h.strip_prefix("branch.head ") {
        st.branch = (v != "(detached)").then(|| v.to_string());
    } else if let Some(v) = h.strip_prefix("branch.upstream ") {
        st.upstream = Some(v.to_string());
    } else if let Some(v) = h.strip_prefix("branch.ab ") {
        for part in v.split_whitespace() {
            if let Some(n) = part.strip_prefix('+') {
                st.ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix('-') {
                st.behind = n.parse().unwrap_or(0);
            }
        }
    }
}

impl Repo {
    /// The work tree, index and branch.
    pub fn status(&self) -> Result<Status> {
        let o = self.run_ok(&["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all", "--ignore-submodules=dirty"])?;
        let mut st = parse(&o.stdout)?;
        st.state = self.state();
        Ok(st)
    }

    /// The operation in progress, from the files git keeps while it runs.
    pub fn state(&self) -> RepoState {
        let g = &self.git_dir;
        if g.join("rebase-merge").is_dir() || g.join("rebase-apply").is_dir() {
            RepoState::Rebasing
        } else if g.join("MERGE_HEAD").is_file() {
            RepoState::Merging
        } else if g.join("CHERRY_PICK_HEAD").is_file() {
            RepoState::CherryPicking
        } else if g.join("REVERT_HEAD").is_file() {
            RepoState::Reverting
        } else if g.join("BISECT_LOG").is_file() {
            RepoState::Bisecting
        } else {
            RepoState::Clean
        }
    }

    /// The message git prepared for the commit that ends a merge, revert or cherry-pick.
    pub fn prepared_message(&self) -> Option<String> {
        std::fs::read_to_string(self.git_dir.join("MERGE_MSG"))
            .ok()
            .map(|m| m.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n").trim().to_string())
            .filter(|m| !m.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_entry_kind() {
        let raw = b"# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaaa bbbb src/main.rs\0\
1 A. N... 000000 100644 100644 0000 cccc new file.txt\0\
2 R. N... 100644 100644 100644 aaaa aaaa R100 renamed.txt\0old.txt\0\
u UU N... 100644 100644 100644 100644 a b c conflict.txt\0\
? untracked dir/x.md\0";
        let st = parse(raw).unwrap();
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert_eq!(st.files.len(), 5);
        assert_eq!(st.files[0].worktree, Change::Modified);
        assert!(st.files[0].is_unstaged() && !st.files[0].is_staged());
        assert_eq!(st.files[1].path, "new file.txt");
        assert!(st.files[1].is_staged());
        assert_eq!(st.files[2].orig_path.as_deref(), Some("old.txt"));
        assert_eq!(st.files[3].conflict.as_deref(), Some("UU"));
        assert_eq!(st.files[4].path, "untracked dir/x.md");
        assert!(st.files[4].is_untracked());
        assert_eq!(st.staged().count(), 2);
        assert_eq!(st.unstaged().count(), 3);
    }

    #[test]
    fn initial_and_detached() {
        let st = parse(b"# branch.oid (initial)\0# branch.head (detached)\0").unwrap();
        assert_eq!(st.head, None);
        assert_eq!(st.branch, None);
    }

    #[test]
    fn junk_never_panics() {
        for raw in [&b"1"[..], b"2 R.", b"u", b"?", b"# branch.ab +x -y", b"\0\0\0", b"1 \xff\xfe"] {
            let _ = parse(raw);
        }
    }
}
