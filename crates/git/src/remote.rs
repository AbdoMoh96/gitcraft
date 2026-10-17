//! Remotes: `git remote -v`.

use serde::Serialize;

use crate::{Repo, Result};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

/// Parse `git remote -v` (`origin\thttps://… (fetch)`).
pub fn parse(raw: &[u8]) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in String::from_utf8_lossy(raw).lines() {
        let Some((name, rest)) = line.split_once('\t') else { continue };
        let (url, kind) = match rest.rsplit_once(' ') {
            Some((u, k)) => (u, k),
            None => (rest, "(fetch)"),
        };
        let i = match out.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                out.push(Remote { name: name.to_string(), ..Remote::default() });
                out.len() - 1
            }
        };
        if let Some(r) = out.get_mut(i) {
            if kind == "(push)" {
                r.push_url = url.to_string();
            } else {
                r.fetch_url = url.to_string();
            }
        }
    }
    out
}

impl Repo {
    pub fn remotes(&self) -> Result<Vec<Remote>> {
        Ok(parse(&self.run_ok(&["remote", "-v"])?.stdout))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_fetch_and_push() {
        let r = super::parse(b"origin\thttps://example.com/a.git (fetch)\norigin\tssh://x/a.git (push)\nup\t/tmp/up (fetch)\nup\t/tmp/up (push)\n");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].fetch_url, "https://example.com/a.git");
        assert_eq!(r[0].push_url, "ssh://x/a.git");
        assert_eq!(r[1].name, "up");
    }
}
