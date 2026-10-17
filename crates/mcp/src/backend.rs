//! Where MCP tool calls end up: a control-channel method call.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{Value, json};

/// Something that answers control-channel methods (`engine.execute`, `repo.inspect`,
/// `ui.screenshot`, …). See `gitcraft_ui_egui::control` for the full list.
pub trait Backend {
    /// Call one method. `Ok` carries the `result`, `Err` the error message.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// True when a real UI is attached.
    fn has_ui(&self) -> bool;
    /// Short human description ("headless", "connected to 127.0.0.1:7990").
    fn describe(&self) -> String;
}

/// A running GitCraft app, reached through its loopback control port.
pub struct Remote {
    addr: String,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u64,
    /// The connection leads to a loopback address: requests carry the control key.
    send_key: bool,
    key: Option<String>,
}

/// The control key: `GITCRAFT_CONTROL_KEY` when set, else the app's key file for the port.
fn find_key(addr: &str) -> Result<String, String> {
    if let Some(k) = std::env::var("GITCRAFT_CONTROL_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        return Ok(k);
    }
    let port: Option<u16> = addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok());
    let (Some(dir), Some(port)) = (gitcraft_control_key::settings_dir(), port) else {
        return Err("no control key: set GITCRAFT_CONTROL_KEY".into());
    };
    let path = gitcraft_control_key::key_path(&dir, port);
    match std::fs::read_to_string(&path).map(|k| k.trim().to_string()) {
        Ok(k) if !k.is_empty() => Ok(k),
        _ => Err(format!("no control key: set GITCRAFT_CONTROL_KEY, or start the app with --control {port} (it writes {})", path.display())),
    }
}

/// Whether `addr` names a loopback address (127.0.0.0/8, ::1, `localhost`). Other host names are
/// not resolved: they never count.
pub fn is_loopback_addr(addr: &str) -> bool {
    let loopback = |ip: IpAddr| ip.to_canonical().is_loopback();
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return loopback(sa.ip());
    }
    let host = addr.rsplit_once(':').map_or(addr, |(h, _)| h);
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    host.parse::<IpAddr>().map_or_else(|_| host.eq_ignore_ascii_case("localhost"), loopback)
}

impl Remote {
    /// Connect to `addr` (`127.0.0.1:7990`), failing fast when nothing is listening.
    pub fn connect(addr: &str) -> std::io::Result<Self> {
        let mut r = Remote { addr: addr.to_string(), conn: None, next_id: 1, send_key: false, key: None };
        r.reconnect()?;
        if !r.send_key {
            log::warn!("{addr} is not a loopback address: the control key is not sent and the app refuses requests");
        }
        Ok(r)
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        self.conn = None;
        self.key = None;
        let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, format!("cannot resolve {}", self.addr));
        for sa in self.addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, Duration::from_millis(800)) {
                Ok(s) => {
                    s.set_nodelay(true).ok();
                    // Network commands run in the app's background; leave headroom.
                    s.set_read_timeout(Some(Duration::from_secs(600))).ok();
                    self.send_key = is_loopback_addr(&self.addr) && s.peer_addr().is_ok_and(|p| p.ip().to_canonical().is_loopback());
                    let read = s.try_clone()?;
                    self.conn = Some((BufReader::new(read), s));
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn exchange(&mut self, id: u64, method: &str, params: &Value) -> Result<Value, String> {
        let mut first = true;
        loop {
            if self.conn.is_none()
                && let Err(e) = self.reconnect()
            {
                return Err(format!("GitCraft app at {} is not reachable: {e}", self.addr));
            }
            let mut req = json!({"id": id, "method": method, "params": params});
            if self.send_key {
                let key = match &self.key {
                    Some(k) => k.clone(),
                    None => find_key(&self.addr)?,
                };
                req["key"] = json!(key);
                self.key = Some(key);
            }
            let line = req.to_string();
            let result = (|| -> std::io::Result<String> {
                let Some((reader, writer)) = self.conn.as_mut() else { return Err(std::io::ErrorKind::NotConnected.into()) };
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                let mut reply = String::new();
                if reader.read_line(&mut reply)? == 0 {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
                Ok(reply)
            })();
            match result {
                Ok(r) => return serde_json::from_str(r.trim()).map_err(|e| format!("bad reply from app: {e}")),
                Err(e) => {
                    self.conn = None;
                    if !first {
                        return Err(format!("GitCraft app at {} is not reachable: {e}", self.addr));
                    }
                    first = false;
                }
            }
        }
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let v = self.exchange(id, method, &params)?;
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("error").and_then(Value::as_str).unwrap_or("the app reported an error").to_string())
        }
    }
    fn has_ui(&self) -> bool {
        true
    }
    fn describe(&self) -> String {
        format!("connected to the GitCraft app at {}", self.addr)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn loopback_only() {
        for a in ["127.0.0.1:7990", "[::1]:7990", "localhost:7990", "127.8.9.1:1"] {
            assert!(super::is_loopback_addr(a), "{a}");
        }
        for a in ["10.0.0.1:7990", "example.com:7990", "localhost.example.com:1"] {
            assert!(!super::is_loopback_addr(a), "{a}");
        }
    }
}
