//! The control channel's methods, answered on the UI thread (`docs/control-protocol.md`).
//!
//! Requests arrive from the app's loopback server as [`ControlRequest`]s; each gets one reply
//! `{"ok": true, "result": …}` or `{"ok": false, "error": "…"}`.

use std::sync::mpsc::Sender;

use serde_json::{Value, json};

use crate::GitApp;

pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Value>,
}

impl ControlRequest {
    pub fn new(method: impl Into<String>, params: Value) -> (Self, std::sync::mpsc::Receiver<Value>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { method: method.into(), params, reply: tx }, rx)
    }
}

pub fn reply(r: &Result<Value, String>) -> Value {
    match r {
        Ok(v) => json!({"ok": true, "result": v}),
        Err(e) => json!({"ok": false, "error": e}),
    }
}

/// Every method the channel answers (besides `ui.screenshot`, which waits for a frame).
pub const METHODS: &[&str] = &["engine.execute", "engine.commands", "repo.inspect", "ui.parity", "ui.run", "ui.state"];

/// Answer one request; `None` when the reply comes later (a background command).
pub fn handle(app: &mut GitApp, method: &str, params: Value, tx: Sender<Value>) -> Option<Value> {
    match method {
        "engine.execute" | "ui.run" => {
            let Some(id) = params.get("command").and_then(Value::as_str).map(str::to_string) else {
                return Some(reply(&Err("missing `command`".into())));
            };
            let p = params.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}));
            let slow = app.session.registry.get(&id).is_some_and(|s| s.slow);
            if slow {
                // Replies when the background command finishes.
                let _ = app.run_with_reply(&id, p, Some(tx));
                None
            } else {
                Some(reply(&app.run(&id, p)))
            }
        }
        "engine.commands" => Some(reply(&Ok(app.session.registry.describe()))),
        "repo.inspect" => Some(reply(&Ok(app.session.inspect()))),
        "ui.parity" => Some(reply(&Ok(gitcraft_engine::catalog::parity(&app.session.registry)))),
        "ui.state" => Some(reply(&app.ui_command("ui.state", &json!({})))),
        m if m.starts_with("ui.") => Some(reply(&app.ui_command(m, &params))),
        other => Some(reply(&Err(format!("unknown method `{other}`")))),
    }
}

/// A screenshot as a control reply: base64 PNG.
pub fn screenshot_reply(img: &egui::ColorImage) -> Value {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&p.to_array());
    }
    let mut png = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut png);
    let r = image::ImageEncoder::write_image(enc, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8);
    match r {
        Ok(()) => reply(&Ok(json!({"png": crate::util::base64(&png), "width": w, "height": h}))),
        Err(e) => reply(&Err(format!("screenshot: {e}"))),
    }
}
