//! Who last changed each line: `git blame --porcelain`.

use std::collections::HashMap;

use serde::Serialize;

use crate::{Repo, Result};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameCommit {
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    pub oid: String,
    /// 1-based line number in the file as it is now.
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blame {
    pub lines: Vec<BlameLine>,
    pub commits: HashMap<String, BlameCommit>,
}

/// Parse `git blame --porcelain` output.
pub fn parse(raw: &[u8]) -> Blame {
    let text = String::from_utf8_lossy(raw);
    let mut b = Blame::default();
    let mut cur: Option<(String, u32)> = None;
    for line in text.split('\n') {
        if let Some(content) = line.strip_prefix('\t') {
            if let Some((oid, n)) = cur.take() {
                b.lines.push(BlameLine { oid, line: n, text: content.to_string() });
            }
            continue;
        }
        let mut parts = line.split(' ');
        let first = parts.next().unwrap_or("");
        if first.len() >= 40 && first.bytes().all(|c| c.is_ascii_hexdigit()) {
            let _orig = parts.next();
            let fin = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            b.commits.entry(first.to_string()).or_default();
            cur = Some((first.to_string(), fin));
            continue;
        }
        let Some((oid, _)) = &cur else { continue };
        let Some(c) = b.commits.get_mut(oid) else { continue };
        let value = line.split_once(' ').map(|(_, v)| v).unwrap_or("");
        match first {
            "author" => c.author = value.to_string(),
            "author-mail" => c.email = value.trim_matches(['<', '>']).to_string(),
            "author-time" => c.time = value.parse().unwrap_or(0),
            "summary" => c.summary = value.to_string(),
            _ => {}
        }
    }
    b
}

impl Repo {
    /// Blame `path` at `rev` (the work tree when `None`).
    pub fn blame(&self, path: &str, rev: Option<&str>) -> Result<Blame> {
        let path = crate::check_path(path)?;
        let mut args = vec!["blame", "--porcelain"];
        if let Some(r) = rev {
            args.push(crate::check_name("revision", r)?);
        }
        args.extend(["--", path]);
        Ok(parse(&self.run_ok(&args)?.stdout))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_porcelain() {
        let a = "a".repeat(40);
        let c = "c".repeat(40);
        let raw = format!(
            "{a} 1 1 2\nauthor Ada\nauthor-mail <ada@x>\nauthor-time 100\nsummary First\nfilename f\n\tone\n{a} 2 2\n\ttwo\n{c} 3 3 1\nauthor Bob\nsummary Second\nfilename f\n\tthree\n"
        );
        let b = super::parse(raw.as_bytes());
        assert_eq!(b.lines.len(), 3);
        assert_eq!(b.lines[1].line, 2);
        assert_eq!(b.lines[2].oid, c);
        assert_eq!(b.commits[&a].email, "ada@x");
        assert_eq!(b.commits[&a].time, 100);
        assert_eq!(b.commits[&c].summary, "Second");
        let _ = super::parse(b"\t\n zz\nauthor\n");
    }
}
