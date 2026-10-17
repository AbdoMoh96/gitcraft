//! Commit history: `git log` with a NUL-separated machine format.

use serde::Serialize;

use crate::{Repo, Result};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    /// The first line of the message.
    pub summary: String,
    /// The rest of the message.
    pub body: String,
}

impl Commit {
    pub fn short(&self) -> &str {
        self.oid.get(..7).unwrap_or(&self.oid)
    }
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
    pub fn message(&self) -> String {
        if self.body.trim().is_empty() { self.summary.clone() } else { format!("{}\n\n{}", self.summary, self.body.trim_end()) }
    }
}

/// What to list.
#[derive(Clone, Debug, PartialEq)]
pub struct LogQuery {
    /// Starting points; empty means every branch, remote branch, tag and HEAD (not the stash).
    pub revs: Vec<String>,
    pub limit: usize,
    pub skip: usize,
    /// Only commits touching this path.
    pub path: Option<String>,
    /// Follow the path through renames (needs `path`).
    pub follow: bool,
    /// Only commits whose message matches (case-insensitive, fixed string).
    pub grep: Option<String>,
    /// Only commits by this author (substring of name or email).
    pub author: Option<String>,
}

impl Default for LogQuery {
    fn default() -> Self {
        LogQuery { revs: Vec::new(), limit: 2000, skip: 0, path: None, follow: false, grep: None, author: None }
    }
}

/// The most commits one query may return.
pub const MAX_LIMIT: usize = 200_000;

const FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%s%x1f%b";

/// Parse the output of `git log -z` with [`FORMAT`].
pub fn parse(raw: &[u8]) -> Vec<Commit> {
    let text = String::from_utf8_lossy(raw);
    text.split('\0').filter_map(parse_one).collect()
}

fn sig(f: &mut std::str::SplitN<'_, char>) -> Signature {
    Signature {
        name: f.next().unwrap_or("").to_string(),
        email: f.next().unwrap_or("").to_string(),
        time: f.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0),
    }
}

fn parse_one(rec: &str) -> Option<Commit> {
    let rec = rec.trim_start_matches('\n');
    if rec.is_empty() {
        return None;
    }
    let mut f = rec.splitn(10, '\x1f');
    let oid = f.next()?.trim().to_string();
    if oid.is_empty() || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let parents = f.next().unwrap_or("").split_whitespace().map(str::to_string).collect();
    let author = sig(&mut f);
    let committer = sig(&mut f);
    let summary = f.next().unwrap_or("").to_string();
    let body = f.next().unwrap_or("").trim_end().to_string();
    Some(Commit { oid, parents, author, committer, summary, body })
}

impl Repo {
    /// Commits newest first, every child before its parents (date order).
    pub fn log(&self, q: &LogQuery) -> Result<Vec<Commit>> {
        let limit = format!("--max-count={}", q.limit.clamp(1, MAX_LIMIT));
        let skip = format!("--skip={}", q.skip);
        let mut args: Vec<String> = vec!["log".into(), "-z".into(), "--date-order".into(), FORMAT.into(), limit, skip];
        if let Some(g) = q.grep.as_deref().filter(|g| !g.is_empty()) {
            args.extend(["-i".into(), "-F".into(), format!("--grep={g}")]);
        }
        if let Some(a) = q.author.as_deref().filter(|a| !a.is_empty()) {
            args.extend(["-F".into(), format!("--author={a}")]);
        }
        if q.revs.is_empty() {
            if !self.has_any_ref() {
                return Ok(Vec::new());
            }
            args.extend(["--exclude=refs/stash".into(), "--all".into()]);
        } else {
            for r in &q.revs {
                args.push(crate::check_name("revision", r)?.to_string());
            }
        }
        if let Some(p) = &q.path {
            if q.follow {
                args.push("--follow".into());
            }
            args.extend(["--".into(), crate::check_path(p)?.to_string()]);
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = if q.path.is_some() { self.run_paths(&refs)? } else { self.run_ok(&refs)? };
        Ok(parse(&o.stdout))
    }

    /// One commit.
    pub fn find_commit(&self, rev: &str) -> Result<Commit> {
        let rev = crate::check_name("revision", rev)?;
        let o = self.run_ok(&["log", "-z", "-1", FORMAT, rev, "--"])?;
        parse(&o.stdout).into_iter().next().ok_or_else(|| crate::GitError::Parse(format!("no commit `{rev}`")))
    }

    /// Whether any branch, tag or HEAD commit exists (git log --all fails otherwise on old gits).
    fn has_any_ref(&self) -> bool {
        self.has_commits() || self.run(&["for-each-ref", "--count=1", "--format=x"]).map(|o| !o.stdout.is_empty()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let raw = b"aaaa1111\x1fbbbb2222 cccc3333\x1fAda\x1fada@x\x1f100\x1fBob\x1fbob@x\x1f200\x1fMerge it\x1fbody line\n\nmore\n\0\nbbbb2222\x1f\x1fAda\x1fada@x\x1f50\x1fAda\x1fada@x\x1f50\x1fRoot\x1f\0";
        let c = parse(raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["bbbb2222", "cccc3333"]);
        assert!(c[0].is_merge());
        assert_eq!(c[0].author.time, 100);
        assert_eq!(c[0].committer.name, "Bob");
        assert_eq!(c[0].body, "body line\n\nmore");
        assert_eq!(c[1].parents.len(), 0);
        assert_eq!(c[1].message(), "Root");
    }

    #[test]
    fn junk_is_skipped() {
        assert!(parse(b"\0\0not hex\x1f\0").is_empty());
        assert_eq!(parse(b"abcd").len(), 1);
    }
}
