//! Loopback JSON-lines control server: one request per line, one reply per line. This is the
//! transport the MCP server (`gitcraft-cli mcp --connect`) wraps.
//!
//! Every request needs the window's key (`docs/control-protocol.md`, Keys): once the port is
//! bound, the app writes a random key to `<settings>/control-key.local-<port>` (mode 0600) and
//! removes the file when the window closes.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use gitcraft_ui_egui::ControlRequest;
use serde_json::{Value, json};

/// Longer request lines are refused (and the connection closed).
const MAX_LINE: u64 = 1 << 20;
/// How long a request may wait for the UI (network commands run in the background).
const REPLY_WAIT: Duration = Duration::from_secs(600);

pub struct KeyFile(PathBuf);

impl KeyFile {
    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn random_key() -> Result<String, getrandom::Error> {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn write_key_file(path: &Path, key: &str) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path)?;
    f.write_all(key.as_bytes())
}

/// Equal without an early exit (the key is compared in constant time).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Bind `127.0.0.1:port`, write the key file into `settings` and serve. `None` when the port
/// cannot be bound or there is no random key.
pub fn start(port: u16, ctx: egui::Context, settings: Option<&Path>) -> Option<(Receiver<ControlRequest>, Option<KeyFile>)> {
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            log::error!("control server failed to bind 127.0.0.1:{port}: {e}");
            return None;
        }
    };
    let key = match random_key() {
        Ok(k) => k,
        Err(e) => {
            log::error!("control server off: no random key ({e})");
            return None;
        }
    };
    let key_file = settings.and_then(|dir| {
        let p = gitcraft_control_key::key_path(dir, port);
        match write_key_file(&p, &key) {
            Ok(()) => Some(KeyFile(p)),
            Err(e) => {
                log::error!("control key file not written ({e}): requests will be refused");
                None
            }
        }
    });
    let key: Arc<str> = if key_file.is_some() { Arc::from(key) } else { Arc::from("") };
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new().name("gitcraft control".into()).spawn(move || {
        for conn in listener.incoming().flatten() {
            let (tx, ctx, key) = (tx.clone(), ctx.clone(), key.clone());
            let _ = std::thread::Builder::new().name("gitcraft control conn".into()).spawn(move || serve(conn, tx, ctx, key));
        }
    });
    if let Err(e) = spawned {
        log::error!("control server thread: {e}");
        return None;
    }
    log::info!("control server on 127.0.0.1:{port}");
    Some((rx, key_file))
}

fn serve(conn: TcpStream, tx: Sender<ControlRequest>, ctx: egui::Context, key: Arc<str>) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(3600)));
    let Ok(mut out) = conn.try_clone() else { return };
    let mut reader = BufReader::new(conn);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if !line.ends_with('\n') && line.len() as u64 >= MAX_LINE => return,
            Ok(_) => {}
        }
        let reply = answer(line.trim(), &tx, &ctx, &key);
        let authorized = reply.get("error").and_then(Value::as_str) != Some("unauthorized");
        if out.write_all(format!("{reply}\n").as_bytes()).is_err() || !authorized {
            return;
        }
    }
}

fn answer(line: &str, tx: &Sender<ControlRequest>, ctx: &egui::Context, key: &str) -> Value {
    let Ok(req) = serde_json::from_str::<Value>(line) else { return json!({"ok": false, "error": "bad JSON"}) };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let given = req.get("key").and_then(Value::as_str).unwrap_or("");
    if key.is_empty() || !same(given, key) {
        return json!({"id": id, "ok": false, "error": "unauthorized"});
    }
    let Some(method) = req.get("method").and_then(Value::as_str) else { return json!({"id": id, "ok": false, "error": "missing `method`"}) };
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let (r, rx) = ControlRequest::new(method, params);
    if tx.send(r).is_err() {
        return json!({"id": id, "ok": false, "error": "the window is closing"});
    }
    ctx.request_repaint();
    let mut v = rx.recv_timeout(REPLY_WAIT).unwrap_or_else(|_| json!({"ok": false, "error": "the app did not answer in time"}));
    if let Some(o) = v.as_object_mut() {
        o.insert("id".into(), id);
    }
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_compare_whole() {
        assert!(super::same("abc", "abc"));
        assert!(!super::same("abc", "abd"));
        assert!(!super::same("abc", "ab"));
        assert!(!super::same("", "x"));
    }
}
