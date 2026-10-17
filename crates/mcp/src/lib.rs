//! GitCraft's MCP server (Model Context Protocol, JSON-RPC 2.0 over stdio).
//!
//! Two backends: [`Headless`] runs an in-process session (no window); [`backend::Remote`] talks to
//! a running GitCraft app through its control channel (`gitcraft --control PORT`), so agents can
//! also drive and screenshot the real UI.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod backend;
pub mod server;
pub mod tools;

pub use backend::{Backend, Remote};
pub use server::Server;

use gitcraft_engine::Session;
use serde_json::{Value, json};

/// Methods that need the desktop app.
pub const NEEDS_APP: &str = "needs the desktop app: start `gitcraft --control 7990` and run `gitcraft-cli mcp --connect 127.0.0.1:7990`";

/// An in-process session.
#[derive(Default)]
pub struct Headless {
    pub session: Session,
}

impl Headless {
    pub fn new(session: Session) -> Self {
        Headless { session }
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "engine.execute" | "command" => {
                let id = params.get("command").and_then(Value::as_str).ok_or("missing `command`")?.to_string();
                let p = params.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
                self.session.run(&id, &p).map_err(|e| e.to_string())
            }
            "engine.commands" => Ok(self.session.registry.describe()),
            "repo.inspect" => Ok(self.session.inspect()),
            "ui.parity" => Ok(gitcraft_engine::catalog::parity(&self.session.registry)),
            m if m.starts_with("ui.") => Err(format!("`{m}` {NEEDS_APP}")),
            other => self.session.run(other, &params).map_err(|e| e.to_string()),
        }
    }
    fn has_ui(&self) -> bool {
        false
    }
    fn describe(&self) -> String {
        "headless session (no window)".into()
    }
}

#[cfg(test)]
mod tests;
