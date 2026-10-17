//! Unified diffs: parsing `git diff` output into files, hunks and numbered lines, and the diffs
//! the views ask for (work tree, index, a commit's file).

use serde::Serialize;

use crate::status::Change;
use crate::{GitError, Repo, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Del,
    /// `\ No newline at end of file`, about the line before it.
    NoNewline,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    /// The line without its `+`/`-`/` ` prefix or line break (a `\r` is kept).
    pub text: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    /// The function or section name git prints after the range.
    pub heading: String,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    pub fn header(&self) -> String {
        let h = if self.heading.is_empty() { String::new() } else { format!(" {}", self.heading) };
        format!("@@ -{},{} +{},{} @@{h}", self.old_start, self.old_len, self.new_start, self.new_len)
    }
    pub fn added(&self) -> usize {
        self.lines.iter().filter(|l| l.kind == LineKind::Add).count()
    }
    pub fn deleted(&self) -> usize {
        self.lines.iter().filter(|l| l.kind == LineKind::Del).count()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// `None` for an added file.
    pub old_path: Option<String>,
    /// `None` for a deleted file.
    pub new_path: Option<String>,
    /// The `diff --git` line and extended headers up to and including `+++` (patches reuse them).
    pub header: Vec<String>,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// Lines were left out ([`MAX_LINES`]); hunks can't be staged from a truncated diff.
    pub truncated: bool,
}

impl FileDiff {
    pub fn path(&self) -> &str {
        self.new_path.as_deref().or(self.old_path.as_deref()).unwrap_or("")
    }
    pub fn added(&self) -> usize {
        self.hunks.iter().map(Hunk::added).sum()
    }
    pub fn deleted(&self) -> usize {
        self.hunks.iter().map(Hunk::deleted).sum()
    }
}

/// The most lines kept for one file.
pub const MAX_LINES: usize = 100_000;

/// A file a commit changed.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    pub orig_path: Option<String>,
    pub change: Change,
}

/// Parse unified diff text (one or more files).
pub fn parse(raw: &[u8]) -> Vec<FileDiff> {
    let text = String::from_utf8_lossy(raw);
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut in_hunk = false;
    let (mut old_no, mut new_no) = (0u32, 0u32);
    let mut lines = 0usize;
    for line in text.split('\n') {
        if line.starts_with("diff --git ") || line.starts_with("diff --cc ") || line.starts_with("diff --combined ") {
            if let Some(f) = cur.take() {
                files.push(f);
            }
            let mut f = FileDiff { header: vec![line.to_string()], ..FileDiff::default() };
            if let Some((a, b)) = git_line_paths(line) {
                f.old_path = Some(a);
                f.new_path = Some(b);
            }
            cur = Some(f);
            in_hunk = false;
            lines = 0;
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            let f = cur.get_or_insert_with(FileDiff::default);
            if let Some(h) = parse_hunk_header(rest) {
                old_no = h.old_start;
                new_no = h.new_start;
                f.hunks.push(h);
                in_hunk = true;
            }
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if !in_hunk {
            header_line(f, line);
            continue;
        }
        if lines >= MAX_LINES {
            f.truncated = true;
            continue;
        }
        let Some(h) = f.hunks.last_mut() else { continue };
        let (kind, body) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Add, line.get(1..).unwrap_or("")),
            Some(b'-') => (LineKind::Del, line.get(1..).unwrap_or("")),
            Some(b' ') => (LineKind::Context, line.get(1..).unwrap_or("")),
            Some(b'\\') => (LineKind::NoNewline, line.get(1..).unwrap_or("").trim()),
            // The empty string after the final line break, or anything unexpected: the hunk ended.
            _ => {
                in_hunk = false;
                continue;
            }
        };
        let (o, n) = match kind {
            LineKind::Context => (Some(old_no), Some(new_no)),
            LineKind::Add => (None, Some(new_no)),
            LineKind::Del => (Some(old_no), None),
            LineKind::NoNewline => (None, None),
        };
        if matches!(kind, LineKind::Context | LineKind::Del) {
            old_no = old_no.saturating_add(1);
        }
        if matches!(kind, LineKind::Context | LineKind::Add) {
            new_no = new_no.saturating_add(1);
        }
        h.lines.push(DiffLine { kind, text: body.to_string(), old_no: o, new_no: n });
        lines += 1;
    }
    if let Some(f) = cur.take() {
        files.push(f);
    }
    files
}

fn header_line(f: &mut FileDiff, line: &str) {
    if line.is_empty() {
        return;
    }
    f.header.push(line.to_string());
    if let Some(p) = line.strip_prefix("--- ") {
        f.old_path = side_path(p, "a/");
    } else if let Some(p) = line.strip_prefix("+++ ") {
        f.new_path = side_path(p, "b/");
    } else if let Some(p) = line.strip_prefix("rename from ").or_else(|| line.strip_prefix("copy from ")) {
        f.old_path = Some(crate::unquote(p));
    } else if let Some(p) = line.strip_prefix("rename to ").or_else(|| line.strip_prefix("copy to ")) {
        f.new_path = Some(crate::unquote(p));
    } else if line.starts_with("new file mode") {
        f.old_path = None;
    } else if line.starts_with("deleted file mode") {
        f.new_path = None;
    } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
        f.binary = true;
    }
}

fn side_path(p: &str, prefix: &str) -> Option<String> {
    let p = p.trim_end_matches('\t');
    if p == "/dev/null" {
        return None;
    }
    let u = crate::unquote(p);
    Some(u.strip_prefix(prefix).map(str::to_string).unwrap_or(u))
}

/// `diff --git a/x b/x` → (`x`, `x`). Only unambiguous when both sides are the same or quoted;
/// the `---`/`+++` and rename headers that follow override it.
fn git_line_paths(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    if rest.starts_with('"') {
        let end = rest.get(1..)?.find("\" ")? + 1;
        let a = crate::unquote(rest.get(..=end)?);
        let b = crate::unquote(rest.get(end + 2..)?);
        return Some((a.strip_prefix("a/")?.to_string(), b.strip_prefix("b/")?.to_string()));
    }
    let n = rest.len();
    if n % 2 == 1 {
        let half = (n - 1) / 2;
        let (a, b) = (rest.get(..half)?, rest.get(half + 1..)?);
        if let (Some(a), Some(b)) = (a.strip_prefix("a/"), b.strip_prefix("b/"))
            && a == b
        {
            return Some((a.to_string(), b.to_string()));
        }
    }
    None
}

/// `-1,3 +1,4 @@ fn main()` (the leading `@@ ` already removed).
fn parse_hunk_header(rest: &str) -> Option<Hunk> {
    let (ranges, heading) = rest.split_once(" @@").unwrap_or((rest, ""));
    let mut parts = ranges.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |r: &str| -> Option<(u32, u32)> {
        match r.split_once(',') {
            Some((s, l)) => Some((s.parse().ok()?, l.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (old_start, old_len) = range(old)?;
    let (new_start, new_len) = range(new)?;
    Some(Hunk { old_start, old_len, new_start, new_len, heading: heading.trim().to_string(), lines: Vec::new() })
}

/// Parse `git diff-tree -r -z --name-status` output.
pub fn parse_name_status(raw: &[u8]) -> Vec<CommitFile> {
    let text = String::from_utf8_lossy(raw);
    let mut out = Vec::new();
    let mut it = text.split('\0').filter(|t| !t.is_empty());
    while let Some(code) = it.next() {
        let letter = code.chars().next().unwrap_or('M');
        let change = match letter {
            'A' => Change::Added,
            'D' => Change::Deleted,
            'R' => Change::Renamed,
            'C' => Change::Copied,
            'T' => Change::TypeChanged,
            'U' => Change::Unmerged,
            _ => Change::Modified,
        };
        if matches!(letter, 'R' | 'C') {
            let (Some(from), Some(to)) = (it.next(), it.next()) else { break };
            out.push(CommitFile { path: to.to_string(), orig_path: Some(from.to_string()), change });
        } else {
            let Some(p) = it.next() else { break };
            out.push(CommitFile { path: p.to_string(), orig_path: None, change });
        }
    }
    out
}

const DIFF_OPTS: [&str; 4] = ["--no-color", "--no-ext-diff", "--submodule=short", "--find-renames"];

fn context_arg(context: u32) -> String {
    format!("-U{}", context.min(10_000))
}

impl Repo {
    /// The work tree's changes to `path` not yet staged (an untracked file shows as all added).
    pub fn diff_unstaged(&self, path: &str, context: u32) -> Result<FileDiff> {
        let path = crate::check_path(path)?;
        let u = context_arg(context);
        let untracked = self.status()?.file(path).is_some_and(|f| f.is_untracked());
        let o = if untracked {
            let mut args = vec!["diff", "--no-index", u.as_str()];
            args.extend(DIFF_OPTS);
            args.extend(["--", null_device(), path]);
            let o = self.run(&args)?;
            // `--no-index` exits 1 when the files differ.
            if !matches!(o.code, Some(0 | 1)) {
                return Err(crate::runner::failed(&args, &o));
            }
            o
        } else {
            let mut args = vec!["diff", u.as_str()];
            args.extend(DIFF_OPTS);
            args.extend(["--", path]);
            self.run_paths(&args)?
        };
        Ok(first_or_empty(parse(&o.stdout), path))
    }

    /// The staged changes to `path`.
    pub fn diff_staged(&self, path: &str, context: u32) -> Result<FileDiff> {
        let path = crate::check_path(path)?;
        let u = context_arg(context);
        let mut args = vec!["diff", "--cached", u.as_str()];
        args.extend(DIFF_OPTS);
        args.extend(["--", path]);
        Ok(first_or_empty(parse(&self.run_paths(&args)?.stdout), path))
    }

    /// What commit `oid` changed in `path`, against its first parent.
    pub fn diff_commit_file(&self, oid: &str, path: &str, context: u32) -> Result<FileDiff> {
        let oid = crate::check_name("commit", oid)?;
        let path = crate::check_path(path)?;
        let u = context_arg(context);
        let parent = format!("{oid}^1");
        let has_parent = self.resolve(&parent).is_some();
        let mut args = if has_parent { vec!["diff", u.as_str(), parent.as_str(), oid] } else { vec!["diff-tree", "-p", "--root", u.as_str(), oid] };
        args.extend(DIFF_OPTS);
        args.extend(["--", path]);
        Ok(first_or_empty(parse(&self.run_paths(&args)?.stdout), path))
    }

    /// The files commit `oid` changed, against its first parent.
    pub fn commit_files(&self, oid: &str) -> Result<Vec<CommitFile>> {
        let oid = crate::check_name("commit", oid)?;
        let parent = format!("{oid}^1");
        let o = if self.resolve(&parent).is_some() {
            self.run_ok(&["diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id", parent.as_str(), oid])?
        } else {
            self.run_ok(&["diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id", "--root", oid])?
        };
        Ok(parse_name_status(&o.stdout))
    }

    /// Diff between any two revisions, every file.
    pub fn diff_revs(&self, from: &str, to: &str, context: u32) -> Result<Vec<FileDiff>> {
        let (from, to) = (crate::check_name("revision", from)?, crate::check_name("revision", to)?);
        let u = context_arg(context);
        let mut args = vec!["diff", u.as_str(), from, to];
        args.extend(DIFF_OPTS);
        args.push("--");
        Ok(parse(&self.run_ok(&args)?.stdout))
    }

    /// A file's text at a revision (`HEAD`, a commit id, or `:0` / `:2` / `:3` for index stages).
    pub fn show_file(&self, rev: &str, path: &str) -> Result<Vec<u8>> {
        let path = crate::check_path(path)?;
        if rev.starts_with('-') {
            return Err(GitError::Arg("revision may not start with `-`".into()));
        }
        let spec = format!("{rev}:{path}");
        Ok(self.run_ok(&["show", spec.as_str()])?.stdout)
    }
}

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn first_or_empty(v: Vec<FileDiff>, path: &str) -> FileDiff {
    v.into_iter().next().unwrap_or_else(|| FileDiff { old_path: Some(path.into()), new_path: Some(path.into()), ..FileDiff::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ mod a;
 line one
-line two
+line 2
+line 2b
 line three
 line four
@@ -10 +11,0 @@
-gone
\\ No newline at end of file
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
";

    #[test]
    fn parses_files_hunks_and_numbers() {
        let f = parse(SAMPLE.as_bytes());
        assert_eq!(f.len(), 4);
        let a = &f[0];
        assert_eq!(a.path(), "src/lib.rs");
        assert_eq!(a.hunks.len(), 2);
        let h = &a.hunks[0];
        assert_eq!((h.old_start, h.old_len, h.new_start, h.new_len), (1, 4, 1, 5));
        assert_eq!(h.heading, "mod a;");
        assert_eq!(h.lines.len(), 6);
        assert_eq!(h.lines[1].kind, LineKind::Del);
        assert_eq!(h.lines[1].old_no, Some(2));
        assert_eq!(h.lines[2].new_no, Some(2));
        assert_eq!(h.lines[4].old_no, Some(3));
        assert_eq!(h.lines[4].new_no, Some(4));
        assert_eq!(a.hunks[1].old_len, 1);
        assert_eq!(a.hunks[1].new_len, 0);
        assert_eq!(a.hunks[1].lines[1].kind, LineKind::NoNewline);
        assert_eq!((a.added(), a.deleted()), (2, 2));
        assert_eq!(f[1].old_path, None);
        assert_eq!(f[1].new_path.as_deref(), Some("new.txt"));
        assert_eq!(f[2].old_path.as_deref(), Some("old name.txt"));
        assert_eq!(f[2].new_path.as_deref(), Some("new name.txt"));
        assert!(f[3].binary);
    }

    #[test]
    fn name_status() {
        let v = parse_name_status(b"M\0a.txt\0R087\0old\0new\0A\0b\0D\0c\0");
        assert_eq!(v.len(), 4);
        assert_eq!(v[1].orig_path.as_deref(), Some("old"));
        assert_eq!(v[1].change, Change::Renamed);
        assert_eq!(v[3].change, Change::Deleted);
        assert_eq!(parse_name_status(b"R100\0only").len(), 0);
    }

    #[test]
    fn junk_never_panics() {
        for s in ["@@ -x +y @@", "@@ ", "diff --git \"a/x", "diff --git a", "+no hunk", "@@ -1 +1 @@\n+\n-\n\\", "diff --git \"\" \"\""] {
            let _ = parse(s.as_bytes());
        }
        assert_eq!(parse_hunk_header("-5 +6 @@").map(|h| (h.old_len, h.new_len)), Some((1, 1)));
    }
}
