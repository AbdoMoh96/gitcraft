//! GitCraft engine: the session over one repository and its commands.
//!
//! **Everything is a command.** Each user-visible action is a [`CommandSpec`] with a stable id
//! (`stage.file`, `branch.create`), a label, where it lives in the UI, a default shortcut, a
//! params description and a `run` function. The toolbar, panels, context menus, shortcuts, the
//! command palette, the CLI, the JSON control channel and the MCP server all call
//! [`Session::run`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod catalog;
pub mod cmd;
mod session;

use std::collections::HashMap;

use serde_json::Value;

pub use gitcraft_git as git;
pub use gitcraft_graph as graph;
pub use session::{Area, Columns, CommitDraft, DiffMode, FileSel, RepoData, Selected, Selection, Session, ViewState};

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum CmdError {
    #[error("unknown command `{0}`")]
    Unknown(String),
    #[error("bad parameters: {0}")]
    Params(String),
    #[error("not available: {0}")]
    Disabled(String),
    #[error("{0}")]
    Failed(String),
}

impl From<gitcraft_git::GitError> for CmdError {
    fn from(e: gitcraft_git::GitError) -> Self {
        match e {
            gitcraft_git::GitError::Arg(m) => CmdError::Params(m),
            other => CmdError::Failed(other.to_string()),
        }
    }
}

pub type CmdResult = Result<Value, CmdError>;

/// A registered command.
#[derive(Clone)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Where it lives in the UI: `Toolbar`, `Commit Panel`, `Left Panel › Local`…
    pub location: &'static str,
    /// Default shortcut in a portable form (`Mod+Enter`, `Mod+Shift+S`). `Mod` = Ctrl, or ⌘ on macOS.
    pub shortcut: &'static str,
    /// Parameter description for agents (`{"name": string}`).
    pub params: &'static str,
    /// Changes the repository (the session reloads it afterwards).
    pub mutates: bool,
    /// Talks to a remote and may take a while: the app runs it in the background.
    pub slow: bool,
    /// `None` = enabled; `Some(reason)` = disabled.
    pub enabled: fn(&Session) -> Option<&'static str>,
    pub run: fn(&mut Session, &Value) -> CmdResult,
}

fn always(_: &Session) -> Option<&'static str> {
    None
}

impl CommandSpec {
    pub const fn new(id: &'static str, label: &'static str, location: &'static str, run: fn(&mut Session, &Value) -> CmdResult) -> Self {
        CommandSpec { id, label, location, shortcut: "", params: "{}", mutates: true, slow: false, enabled: always, run }
    }
    pub const fn key(mut self, s: &'static str) -> Self {
        self.shortcut = s;
        self
    }
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    /// Doesn't change the repository (selection, view, queries).
    pub const fn pure(mut self) -> Self {
        self.mutates = false;
        self
    }
    /// Network: run in the background.
    pub const fn slow(mut self) -> Self {
        self.slow = true;
        self
    }
    pub const fn when(mut self, f: fn(&Session) -> Option<&'static str>) -> Self {
        self.enabled = f;
        self
    }
}

/// All commands by id.
pub struct Registry {
    specs: Vec<CommandSpec>,
    by_id: HashMap<&'static str, usize>,
}

impl Registry {
    pub fn new(specs: Vec<CommandSpec>) -> Self {
        let mut by_id = HashMap::new();
        for (i, s) in specs.iter().enumerate() {
            by_id.insert(s.id, i);
        }
        Registry { specs, by_id }
    }
    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.by_id.get(id).and_then(|i| self.specs.get(*i))
    }
    pub fn all(&self) -> &[CommandSpec] {
        &self.specs
    }
    /// Command list as JSON (for agents: `commands` / `list_commands`).
    pub fn describe(&self) -> Value {
        Value::Array(
            self.specs
                .iter()
                .map(|s| {
                    serde_json::json!({"id": s.id, "label": s.label, "location": s.location, "shortcut": s.shortcut, "params": s.params, "mutates": s.mutates, "slow": s.slow})
                })
                .collect(),
        )
    }
    /// The command bound to a shortcut, if any.
    pub fn by_shortcut(&self, key: &str) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| !s.shortcut.is_empty() && s.shortcut.split(" / ").any(|k| k.eq_ignore_ascii_case(key)))
    }
}

/// Parameter helpers.
pub mod p {
    use serde_json::Value;

    use crate::CmdError;

    pub fn str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
        v.get(k).and_then(Value::as_str)
    }
    pub fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, CmdError> {
        str(v, k).filter(|s| !s.trim().is_empty()).ok_or_else(|| CmdError::Params(format!("`{k}` (string) is required")))
    }
    pub fn u64(v: &Value, k: &str) -> Option<u64> {
        v.get(k).and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0 && f.is_finite()).map(|f| f as u64)))
    }
    pub fn usize(v: &Value, k: &str) -> Option<usize> {
        u64(v, k).map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }
    pub fn bool(v: &Value, k: &str) -> Option<bool> {
        v.get(k).and_then(Value::as_bool)
    }
    /// `paths` (array of strings) or `path` (one string).
    pub fn paths(v: &Value) -> Result<Vec<String>, CmdError> {
        if let Some(a) = v.get("paths").and_then(Value::as_array) {
            let ps: Vec<String> = a.iter().filter_map(Value::as_str).map(str::to_string).collect();
            if ps.is_empty() || ps.len() != a.len() {
                return Err(CmdError::Params("`paths` must be a non-empty array of strings".into()));
            }
            return Ok(ps);
        }
        Ok(vec![req_str(v, "path")?.to_string()])
    }
    /// A list of indices (`lines: [0, 3]`).
    pub fn indices(v: &Value, k: &str) -> Option<Vec<usize>> {
        let a = v.get(k)?.as_array()?;
        Some(a.iter().filter_map(Value::as_u64).map(|n| usize::try_from(n).unwrap_or(usize::MAX)).collect())
    }
}

#[cfg(test)]
mod tests;
