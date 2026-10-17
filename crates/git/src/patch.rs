//! Patches for staging, unstaging and discarding part of a file: one hunk, or chosen lines of it.
//!
//! A patch is built from a parsed [`FileDiff`] and fed to `git apply` (`--cached` for the index,
//! `--reverse` to take changes back out). Unchosen lines are turned into context or left out so
//! the rest of the file is untouched; `--recount` lets git fix the hunk header counts.

use crate::diff::{FileDiff, Hunk, LineKind};
use crate::{GitError, Repo, Result};

/// Where a partial patch goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Stage work-tree changes (from the unstaged diff).
    Stage,
    /// Unstage index changes (from the staged diff).
    Unstage,
    /// Throw away work-tree changes (from the unstaged diff).
    Discard,
}

/// The patch text for hunk `hunk` of `file`, keeping only the lines at `lines` (indices into the
/// hunk's lines) or all of them. `reverse` says the patch will be applied with `--reverse`.
pub fn build(file: &FileDiff, hunk: usize, lines: Option<&[usize]>, reverse: bool) -> Result<String> {
    if file.binary {
        return Err(GitError::Arg("binary files are staged whole".into()));
    }
    if file.truncated {
        return Err(GitError::Arg("the diff is too long to stage in parts; stage the whole file".into()));
    }
    let h = file.hunks.get(hunk).ok_or_else(|| GitError::Arg(format!("no hunk {hunk}")))?;
    let mut out = String::new();
    for l in &file.header {
        out.push_str(l);
        out.push('\n');
    }
    // A header without `---`/`+++` (a pure rename or mode change) can't carry a hunk.
    if !file.header.iter().any(|l| l.starts_with("+++ ")) {
        return Err(GitError::Arg("this change has no lines to stage".into()));
    }
    let body = hunk_body(h, lines, reverse);
    if !body.iter().any(|(k, _)| matches!(k, LineKind::Add | LineKind::Del)) {
        return Err(GitError::Arg("no changed lines chosen".into()));
    }
    let old_len = body.iter().filter(|(k, _)| matches!(k, LineKind::Context | LineKind::Del)).count();
    let new_len = body.iter().filter(|(k, _)| matches!(k, LineKind::Context | LineKind::Add)).count();
    let heading = if h.heading.is_empty() { String::new() } else { format!(" {}", h.heading) };
    out.push_str(&format!("@@ -{},{} +{},{} @@{heading}\n", h.old_start, old_len, h.new_start, new_len));
    for (k, text) in body {
        let prefix = match k {
            LineKind::Context => ' ',
            LineKind::Add => '+',
            LineKind::Del => '-',
            LineKind::NoNewline => '\\',
        };
        out.push(prefix);
        if k == LineKind::NoNewline {
            out.push(' ');
        }
        out.push_str(text);
        out.push('\n');
    }
    Ok(out)
}

/// The hunk's lines with the unchosen changes neutralised: when applied forwards an unchosen
/// addition is dropped and an unchosen deletion stays as context; applied in reverse the roles
/// swap. A "no newline" marker follows its line, or goes with it.
fn hunk_body<'a>(h: &'a Hunk, chosen: Option<&[usize]>, reverse: bool) -> Vec<(LineKind, &'a str)> {
    let mut out: Vec<(LineKind, &str)> = Vec::new();
    let mut prev_kept = true;
    for (i, l) in h.lines.iter().enumerate() {
        let picked = chosen.is_none_or(|c| c.contains(&i));
        let kind = match (l.kind, picked) {
            (LineKind::NoNewline, _) => {
                if prev_kept {
                    out.push((LineKind::NoNewline, l.text.as_str()));
                }
                continue;
            }
            (k, true) => Some(k),
            (LineKind::Context, false) => Some(LineKind::Context),
            (LineKind::Add, false) => reverse.then_some(LineKind::Context),
            (LineKind::Del, false) => (!reverse).then_some(LineKind::Context),
        };
        prev_kept = kind.is_some();
        if let Some(k) = kind {
            out.push((k, l.text.as_str()));
        }
    }
    out
}

impl Repo {
    /// Apply part of a diff: hunk `hunk` of `file`, optionally only some of its lines.
    pub fn apply_partial(&self, file: &FileDiff, hunk: usize, lines: Option<&[usize]>, target: Target) -> Result<()> {
        let reverse = matches!(target, Target::Unstage | Target::Discard);
        let patch = build(file, hunk, lines, reverse)?;
        let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
        match target {
            Target::Stage => args.push("--cached"),
            Target::Unstage => args.extend(["--cached", "--reverse"]),
            Target::Discard => args.push("--reverse"),
        }
        args.push("-");
        self.run_input(&args, patch.as_bytes()).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    const D: &str = "diff --git a/f.txt b/f.txt
index 1..2 100644
--- a/f.txt
+++ b/f.txt
@@ -1,3 +1,3 @@
 a
-b
+B
 c
";

    #[test]
    fn whole_hunk_round_trips() {
        let f = &parse(D.as_bytes())[0];
        let p = build(f, 0, None, false).unwrap();
        assert!(p.ends_with("@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"), "{p}");
    }

    #[test]
    fn chosen_lines_forward_and_reverse() {
        let f = &parse(D.as_bytes())[0];
        // Only the addition, forwards: the deletion stays as context.
        let p = build(f, 0, Some(&[2]), false).unwrap();
        assert!(p.ends_with("@@ -1,3 +1,4 @@\n a\n b\n+B\n c\n"), "{p}");
        // Only the deletion, forwards: the addition is dropped.
        let p = build(f, 0, Some(&[1]), false).unwrap();
        assert!(p.ends_with("@@ -1,3 +1,2 @@\n a\n-b\n c\n"), "{p}");
        // Only the addition, in reverse: the deletion is dropped.
        let p = build(f, 0, Some(&[2]), true).unwrap();
        assert!(p.ends_with("@@ -1,2 +1,3 @@\n a\n+B\n c\n"), "{p}");
        // Nothing changed chosen.
        assert!(build(f, 0, Some(&[0]), false).is_err());
        assert!(build(f, 5, None, false).is_err());
    }
}
