//! SSH keys for remotes: listing the user's public keys and making a new one with `ssh-keygen`.
//! Nothing here reads a private key; generating never overwrites an existing file and never
//! prompts (the passphrase is given up front, empty for none).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::{GitError, Result};

/// A public key in the SSH folder.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    /// The private key's path (the `.pub` file without its extension).
    pub path: String,
    /// `ssh-ed25519`, `ssh-rsa`, …
    pub kind: String,
    /// The comment at the end of the public key (often an e-mail address).
    pub comment: String,
    /// The whole public key line, to paste into a hosting service.
    pub public: String,
}

/// The user's SSH folder (`~/.ssh`).
pub fn ssh_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".ssh"))
}

/// Parse one public key line (`ssh-ed25519 AAAA… comment`).
pub fn parse_public(line: &str) -> Option<(String, String)> {
    let mut f = line.split_whitespace();
    let kind = f.next()?;
    let body = f.next()?;
    if !(kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.starts_with("sk-")) || body.len() < 16 {
        return None;
    }
    Some((kind.to_string(), f.collect::<Vec<_>>().join(" ")))
}

/// Public keys in `dir` (at most 200, small files only).
pub fn list_keys(dir: &Path) -> Vec<SshKey> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SshKey> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pub"))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 64 * 1024))
        .take(200)
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let line = text.lines().next()?.trim().to_string();
            let (kind, comment) = parse_public(&line)?;
            Some(SshKey { path: p.with_extension("").display().to_string(), kind, comment, public: line })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Make a new key pair at `path` (`ed25519` or `rsa`). Refuses to replace an existing file.
pub fn generate(path: &Path, kind: &str, comment: &str, passphrase: &str) -> Result<SshKey> {
    let kind = match kind.trim() {
        "" | "ed25519" => "ed25519",
        "rsa" => "rsa",
        other => return Err(GitError::Arg(format!("key type `{other}` is ed25519 or rsa"))),
    };
    if comment.contains(['\0', '\n', '\r']) || comment.len() > 512 || passphrase.contains(['\0', '\n', '\r']) || passphrase.len() > 1024 {
        return Err(GitError::Arg("the comment or passphrase is not valid".into()));
    }
    let p = path.to_string_lossy().into_owned();
    if p.is_empty() || p.starts_with('-') || p.contains(['\0', '\n', '\r']) {
        return Err(GitError::Arg("not a valid key path".into()));
    }
    let public = path.with_extension(match path.extension() {
        Some(e) => format!("{}.pub", e.to_string_lossy()),
        None => "pub".into(),
    });
    if path.exists() || public.exists() {
        return Err(GitError::Arg(format!("{} exists already: choose another name", path.display())));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| GitError::Io(format!("{}: {e}", dir.display())))?;
    }
    let mut c = Command::new("ssh-keygen");
    c.args(["-q", "-t", kind]);
    if kind == "rsa" {
        c.args(["-b", "4096"]);
    }
    c.args(["-C", comment, "-N", passphrase, "-f", p.as_str()]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = c.output().map_err(|e| GitError::NotFound(format!("ssh-keygen: {e}")))?;
    if !out.status.success() {
        return Err(GitError::Failed {
            cmd: "ssh-keygen".into(),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    let text = std::fs::read_to_string(&public).map_err(|e| GitError::Io(format!("{}: {e}", public.display())))?;
    let line = text.lines().next().unwrap_or("").trim().to_string();
    let (kind, comment) = parse_public(&line).ok_or_else(|| GitError::Parse("ssh-keygen wrote no public key".into()))?;
    Ok(SshKey { path: p, kind, comment, public: line })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_lists() {
        assert_eq!(parse_public("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA ada@example.com"), Some(("ssh-ed25519".into(), "ada@example.com".into())));
        assert!(parse_public("not a key").is_none());
        assert!(parse_public("").is_none());
        let d = crate::testing::TempDir::new("ssh-list").unwrap();
        std::fs::write(d.path().join("id_test.pub"), "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc me\n").unwrap();
        std::fs::write(d.path().join("junk.pub"), "\u{0}\u{1}").unwrap();
        let keys = list_keys(d.path());
        assert_eq!(keys.len(), 1);
        assert!(keys[0].path.ends_with("id_test"));
    }

    #[test]
    fn generates_a_key_once() {
        let d = crate::testing::TempDir::new("ssh-gen").unwrap();
        let path = d.path().join("keys").join("id_gitcraft");
        match generate(&path, "ed25519", "ada@example.com", "") {
            Ok(k) => {
                assert_eq!(k.kind, "ssh-ed25519");
                assert_eq!(k.comment, "ada@example.com");
                assert!(generate(&path, "ed25519", "again", "").is_err(), "never overwrites");
            }
            // No ssh-keygen on this machine.
            Err(GitError::NotFound(_)) => {}
            Err(e) => panic!("{e}"),
        }
        assert!(generate(&path, "dsa", "", "").is_err());
        assert!(generate(Path::new("-oops"), "ed25519", "", "").is_err());
    }
}
