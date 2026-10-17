//! Branches, remote branches and tags: `git for-each-ref`.

use serde::Serialize;

use crate::{Repo, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ref {
    /// `refs/heads/main`.
    pub full: String,
    /// `main`, `origin/main`, `v1.0`.
    pub name: String,
    pub kind: RefKind,
    /// The object the ref points at.
    pub target: String,
    /// The commit an annotated tag points at (same as `target` otherwise).
    pub commit: String,
    /// A local branch's upstream (`origin/main`).
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream branch was deleted on the remote.
    pub gone: bool,
    /// The checked-out branch.
    pub head: bool,
}

impl Ref {
    /// The remote of a remote branch (`origin` for `origin/main`).
    pub fn remote(&self) -> Option<&str> {
        (self.kind == RefKind::Remote).then(|| self.name.split_once('/').map(|(r, _)| r)).flatten()
    }
    /// A remote branch's name on its remote (`main` for `origin/main`).
    pub fn remote_branch(&self) -> Option<&str> {
        (self.kind == RefKind::Remote).then(|| self.name.split_once('/').map(|(_, b)| b)).flatten()
    }
}

const FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(*objectname)%1f%(upstream:short)%1f%(upstream:track)%1f%(HEAD)%1f%(symref)";

/// Parse `git for-each-ref` output in [`FORMAT`]. Symbolic refs (`origin/HEAD`) are left out.
pub fn parse(raw: &[u8]) -> Vec<Ref> {
    String::from_utf8_lossy(raw).lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Ref> {
    let f: Vec<&str> = line.split('\x1f').collect();
    let full = f.first()?.to_string();
    let target = f.get(1)?.to_string();
    if !f.get(6).unwrap_or(&"").is_empty() {
        return None;
    }
    let peeled = f.get(2).copied().unwrap_or("");
    let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
        (RefKind::Local, n)
    } else if let Some(n) = full.strip_prefix("refs/remotes/") {
        (RefKind::Remote, n)
    } else if let Some(n) = full.strip_prefix("refs/tags/") {
        (RefKind::Tag, n)
    } else {
        (RefKind::Other, full.as_str())
    };
    let upstream = f.get(3).filter(|u| !u.is_empty()).map(|u| u.to_string());
    let (ahead, behind, gone) = track(f.get(4).copied().unwrap_or(""));
    Some(Ref {
        name: name.to_string(),
        kind,
        commit: if peeled.is_empty() { target.clone() } else { peeled.to_string() },
        target,
        upstream,
        ahead,
        behind,
        gone,
        head: f.get(5).is_some_and(|h| *h == "*"),
        full,
    })
}

/// `[ahead 1, behind 2]`, `[gone]`, ``.
fn track(t: &str) -> (u32, u32, bool) {
    let inner = t.trim().trim_start_matches('[').trim_end_matches(']');
    let mut r = (0, 0, false);
    for part in inner.split(',').map(str::trim) {
        if let Some(n) = part.strip_prefix("ahead ") {
            r.0 = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            r.1 = n.parse().unwrap_or(0);
        } else if part == "gone" {
            r.2 = true;
        }
    }
    r
}

impl Repo {
    /// Every branch, remote branch and tag, sorted by kind then name.
    pub fn refs(&self) -> Result<Vec<Ref>> {
        let o = self.run_ok(&["for-each-ref", FORMAT, "refs/heads", "refs/remotes", "refs/tags"])?;
        let mut v = parse(&o.stdout);
        v.sort_by(|a, b| (a.kind as u8, &a.name).cmp(&(b.kind as u8, &b.name)));
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kinds_tracking_and_tags() {
        let raw = b"refs/heads/main\x1faaaa\x1f\x1forigin/main\x1f[ahead 1, behind 2]\x1f*\x1f\n\
refs/heads/old\x1fbbbb\x1f\x1forigin/old\x1f[gone]\x1f \x1f\n\
refs/remotes/origin/HEAD\x1faaaa\x1f\x1f\x1f\x1f \x1frefs/remotes/origin/main\n\
refs/remotes/origin/feature/x\x1fcccc\x1f\x1f\x1f\x1f \x1f\n\
refs/tags/v1\x1fdddd\x1feeee\x1f\x1f\x1f \x1f\n";
        let r = parse(raw);
        assert_eq!(r.len(), 4);
        assert!(r[0].head && r[0].kind == RefKind::Local);
        assert_eq!((r[0].ahead, r[0].behind), (1, 2));
        assert!(r[1].gone);
        assert_eq!(r[2].remote(), Some("origin"));
        assert_eq!(r[2].remote_branch(), Some("feature/x"));
        assert_eq!(r[3].kind, RefKind::Tag);
        assert_eq!(r[3].commit, "eeee");
    }

    #[test]
    fn junk_never_panics() {
        assert!(parse(b"\n\x1f\n\xff").len() <= 3);
    }
}
