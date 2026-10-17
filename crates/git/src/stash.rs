//! Stashes: `git stash list`.

use serde::Serialize;

use crate::{Repo, Result};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    /// 0 for `stash@{0}`, the newest.
    pub index: usize,
    pub oid: String,
    pub time: i64,
    /// `On main: message` or `WIP on main: …`.
    pub message: String,
}

impl Stash {
    pub fn reference(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}

const FORMAT: &str = "--format=%gd%x1f%H%x1f%ct%x1f%gs";

pub fn parse(raw: &[u8]) -> Vec<Stash> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter_map(|rec| {
            let rec = rec.trim_matches('\n');
            let mut f = rec.splitn(4, '\x1f');
            let sel = f.next()?;
            let index = sel.strip_prefix("stash@{")?.strip_suffix('}')?.parse().ok()?;
            let oid = f.next()?.to_string();
            let time = f.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            let message = f.next().unwrap_or("").to_string();
            Some(Stash { index, oid, time, message })
        })
        .collect()
}

impl Repo {
    pub fn stashes(&self) -> Result<Vec<Stash>> {
        if self.resolve("refs/stash").is_none() {
            return Ok(Vec::new());
        }
        Ok(parse(&self.run_ok(&["stash", "list", "-z", FORMAT])?.stdout))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_list() {
        let s = super::parse(b"stash@{0}\x1faaaa\x1f10\x1fOn main: wip\0stash@{1}\x1fbbbb\x1f5\x1fWIP on main: 123 x\0");
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].index, 1);
        assert_eq!(s[0].message, "On main: wip");
        assert_eq!(s[0].reference(), "stash@{0}");
        assert!(super::parse(b"stash@{x}\x1f\0junk").is_empty());
    }
}
