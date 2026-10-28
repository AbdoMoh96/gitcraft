//! Who commits and how: the author identity (this repository or every repository), commit
//! signing (OpenPGP, SSH or X.509 keys) and the user's SSH keys.

use gitcraft_git::{ConfigScope, Repo};
use serde_json::{Value, json};

use super::need_repo;
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

fn scope(v: &Value) -> Result<ConfigScope, CmdError> {
    ConfigScope::parse(p::str(v, "scope").unwrap_or("local")).ok_or_else(|| CmdError::Params("`scope` is local or global".into()))
}

/// An optional string param: absent leaves the setting, empty removes it.
fn setting<'a>(v: &'a Value, k: &str) -> Option<Option<&'a str>> {
    p::str(v, k).map(|x| Some(x.trim()).filter(|x| !x.is_empty()))
}

fn identity_json(repo: &Repo) -> Value {
    let get = |sc, k| repo.config_in(sc, k);
    json!({
        "local": {"name": get(ConfigScope::Local, "user.name"), "email": get(ConfigScope::Local, "user.email")},
        "global": {"name": get(ConfigScope::Global, "user.name"), "email": get(ConfigScope::Global, "user.email")},
        "effective": {"name": repo.config("user.name"), "email": repo.config("user.email")},
    })
}

/// The author name and e-mail commits are made with; sets them when given (in `scope`).
fn identity(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    let sc = scope(v)?;
    let mut changed = false;
    if let Some(name) = setting(v, "name") {
        repo.set_config(sc, "user.name", name)?;
        changed = true;
    }
    if let Some(email) = setting(v, "email") {
        if email.is_some_and(|e| !e.contains('@') || e.contains(['<', '>'])) {
            return Err(CmdError::Params("`email` is an e-mail address".into()));
        }
        repo.set_config(sc, "user.email", email)?;
        changed = true;
    }
    let mut out = identity_json(&repo);
    if changed {
        out["message"] =
            json!(format!("Saved the author identity for {}", if sc == ConfigScope::Local { "this repository" } else { "every repository" }));
    }
    Ok(out)
}

fn signing_json(repo: &Repo) -> Value {
    json!({
        "format": repo.config("gpg.format").unwrap_or_else(|| "openpgp".into()),
        "key": repo.config("user.signingkey"),
        "signCommits": repo.config("commit.gpgsign").is_some_and(|x| x == "true"),
        "signTags": repo.config("tag.gpgsign").is_some_and(|x| x == "true"),
        "program": repo.config("gpg.program"),
    })
}

/// Commit signing: the key format, the key, and whether commits and tags are signed by default.
fn signing(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    let sc = scope(v)?;
    let mut changed = false;
    if let Some(f) = setting(v, "format") {
        if f.is_some_and(|f| !matches!(f, "openpgp" | "ssh" | "x509")) {
            return Err(CmdError::Params("`format` is openpgp, ssh or x509".into()));
        }
        repo.set_config(sc, "gpg.format", f)?;
        changed = true;
    }
    if let Some(k) = setting(v, "key") {
        repo.set_config(sc, "user.signingkey", k)?;
        changed = true;
    }
    for (param, key) in [("signCommits", "commit.gpgsign"), ("signTags", "tag.gpgsign")] {
        if let Some(b) = p::bool(v, param) {
            repo.set_config(sc, key, Some(if b { "true" } else { "false" }))?;
            changed = true;
        }
    }
    let mut out = signing_json(&repo);
    if changed {
        out["message"] = json!("Saved the signing settings");
        // The commit panel follows the new default.
        s.draft.sign = None;
    }
    Ok(out)
}

fn ssh_keys(_: &mut Session, _: &Value) -> CmdResult {
    let dir = gitcraft_git::ssh::ssh_dir().ok_or_else(|| CmdError::Failed("no home folder".into()))?;
    Ok(json!({"dir": dir.display().to_string(), "keys": gitcraft_git::ssh::list_keys(&dir)}))
}

fn ssh_generate(_: &mut Session, v: &Value) -> CmdResult {
    let dir = gitcraft_git::ssh::ssh_dir().ok_or_else(|| CmdError::Failed("no home folder".into()))?;
    let name = p::str(v, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or("id_ed25519");
    if name.contains(['/', '\\']) || name.starts_with('.') || name.starts_with('-') || name.len() > 100 {
        return Err(CmdError::Params("`name` is a file name in the SSH folder".into()));
    }
    let key = gitcraft_git::ssh::generate(
        &dir.join(name),
        p::str(v, "type").unwrap_or("ed25519"),
        p::str(v, "comment").unwrap_or(""),
        p::str(v, "passphrase").unwrap_or(""),
    )?;
    Ok(json!({"message": format!("Created the SSH key {}", key.path), "key": key}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("config.identity", "Author Identity", "Preferences", identity)
            .params(r#"{"scope"?: "local" | "global" (default local), "name"?: string, "email"?: string} (empty removes; none: just read)"#)
            .when(need_repo),
        CommandSpec::new("config.signing", "Commit Signing", "Preferences", signing)
            .params(
                r#"{"scope"?: "local" | "global", "format"?: "openpgp" | "ssh" | "x509", "key"?: string, "signCommits"?: bool, "signTags"?: bool}"#,
            )
            .when(need_repo),
        CommandSpec::new("ssh.keys", "SSH Keys", "Preferences", ssh_keys).pure(),
        CommandSpec::new("ssh.generate", "Generate SSH Key", "Preferences", ssh_generate)
            .params(r#"{"name"?: file name in ~/.ssh (id_ed25519), "type"?: "ed25519" | "rsa", "comment"?: string, "passphrase"?: string}"#)
            .pure(),
    ]
}
