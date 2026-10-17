//! Authors' pictures from Gravatar: fetched on background threads, kept on disk for a week, and
//! painted as circles. Anyone without a picture (or while it loads, or offline) gets their
//! initials on a colour of their own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::epaint::RectShape;
use egui::{Align2, Color32, ColorImage, CornerRadius, FontId, Painter, Pos2, Rect, TextureHandle, TextureId, TextureOptions, pos2, vec2};
use sha2::{Digest, Sha256};

use crate::util;

/// Pixels asked for (a 2× circle of the largest size we paint).
const SIZE: u32 = 128;
/// Downloads running at once.
const WORKERS: usize = 4;
/// People looked up at most per run.
const MAX_PEOPLE: usize = 4096;
/// Largest picture accepted, in bytes and pixels a side.
const MAX_BYTES: u64 = 1 << 20;
const MAX_SIDE: u32 = 1024;
/// How long a downloaded picture, or the knowledge that there is none, is kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);

enum Entry {
    Loading,
    Ready(TextureHandle),
    Missing,
}

/// A finished lookup: the e-mail and its picture, if any.
type Done = (String, Option<ColorImage>);

pub struct Avatars {
    /// The disk cache; `None` turns lookups off for good.
    dir: Option<PathBuf>,
    enabled: bool,
    entries: RefCell<HashMap<String, Entry>>,
    requests: RefCell<Option<Sender<String>>>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
}

impl Avatars {
    /// Pictures cached in `dir`; `None` never looks anything up.
    pub fn new(dir: Option<PathBuf>) -> Self {
        let (done_tx, done_rx) = channel();
        let enabled = dir.is_some();
        Avatars { dir, enabled, entries: RefCell::default(), requests: RefCell::default(), done_tx, done_rx }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on && self.dir.is_some();
    }

    /// Turn finished downloads into textures.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut any = false;
        while let Ok((email, img)) = self.done_rx.try_recv() {
            let e = match img {
                Some(img) => Entry::Ready(ctx.load_texture(format!("avatar:{email}"), img, TextureOptions::LINEAR)),
                None => Entry::Missing,
            };
            if let Ok(mut m) = self.entries.try_borrow_mut() {
                m.insert(email, e);
            }
            any = true;
        }
        if any {
            ctx.request_repaint();
        }
    }

    /// `email`'s picture, when it has loaded; asks for it the first time.
    pub fn get(&self, email: &str) -> Option<TextureId> {
        if !self.enabled {
            return None;
        }
        let key = email.trim().to_lowercase();
        if key.is_empty() || !key.contains('@') {
            return None;
        }
        let mut m = self.entries.try_borrow_mut().ok()?;
        match m.get(&key) {
            Some(Entry::Ready(t)) => return Some(t.id()),
            Some(_) => return None,
            None if m.len() >= MAX_PEOPLE => return None,
            None => {}
        }
        if self.request(key.clone()) {
            m.insert(key, Entry::Loading);
        }
        None
    }

    fn request(&self, email: String) -> bool {
        let Some(dir) = self.dir.clone() else { return false };
        let Ok(mut req) = self.requests.try_borrow_mut() else { return false };
        if req.is_none() {
            *req = start_workers(dir, self.done_tx.clone());
        }
        req.as_ref().is_some_and(|tx| tx.send(email).is_ok())
    }

    /// Paint `email`'s picture as a circle of radius `r`, faded by `tint`; false when there is none.
    pub fn paint(&self, p: &Painter, c: Pos2, r: f32, email: &str, tint: Color32) -> bool {
        let Some(id) = self.get(email) else { return false };
        let rect = Rect::from_center_size(c, vec2(r * 2.0, r * 2.0));
        let round = CornerRadius::same(r.round().clamp(0.0, 255.0) as u8);
        p.add(RectShape::filled(rect, round, tint).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
        true
    }

    /// The picture, or the person's initials on their colour.
    pub fn paint_or_initials(&self, p: &Painter, c: Pos2, r: f32, email: &str, name: &str, tint: Color32) {
        if !self.paint(p, c, r, email, tint) {
            initials(p, c, r, name, tint);
        }
    }
}

/// A person's initials in a circle of their colour.
pub fn initials(p: &Painter, c: Pos2, r: f32, name: &str, tint: Color32) {
    let fade = |x: Color32| x.gamma_multiply(f32::from(tint.a()) / 255.0);
    p.circle_filled(c, r, fade(util::person_color(name)));
    p.text(c, Align2::CENTER_CENTER, util::initials(name), FontId::proportional((r * 0.9).max(6.0)), fade(Color32::WHITE));
}

/// Gravatar's id for an e-mail address: the SHA-256 of it, trimmed and lower-cased, in hex.
pub fn gravatar_hash(email: &str) -> String {
    let d = Sha256::digest(email.trim().to_lowercase().as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn start_workers(dir: PathBuf, done: Sender<Done>) -> Option<Sender<String>> {
    let (tx, rx) = channel::<String>();
    let rx = Arc::new(Mutex::new(rx));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .user_agent(concat!("GitCraft/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut started = 0;
    for i in 0..WORKERS {
        let (rx, done, dir, agent) = (rx.clone(), done.clone(), dir.clone(), agent.clone());
        let spawned = std::thread::Builder::new().name(format!("gitcraft avatars {i}")).spawn(move || {
            loop {
                let next = match rx.lock() {
                    Ok(r) => r.recv(),
                    Err(_) => return,
                };
                let Ok(email) = next else { return };
                let img = lookup(&agent, &dir, &email);
                if done.send((email, img)).is_err() {
                    return;
                }
            }
        });
        if spawned.is_ok() {
            started += 1;
        }
    }
    (started > 0).then_some(tx)
}

fn fresh(path: &Path) -> bool {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < KEEP)
}

/// The picture for `email`: from the disk cache when fresh, else downloaded (and cached).
fn lookup(agent: &ureq::Agent, dir: &Path, email: &str) -> Option<ColorImage> {
    let hash = gravatar_hash(email);
    let file = dir.join(format!("{hash}.img"));
    let none = dir.join(format!("{hash}.none"));
    if fresh(&none) {
        return None;
    }
    if fresh(&file)
        && let Ok(bytes) = std::fs::read(&file)
        && let Some(img) = decode(&bytes)
    {
        return Some(img);
    }
    let url = format!("https://gravatar.com/avatar/{hash}?s={SIZE}&d=404");
    let mut resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(e) => {
            log::debug!("gravatar {hash}: {e}");
            return None;
        }
    };
    let _ = std::fs::create_dir_all(dir);
    if resp.status().as_u16() == 404 {
        let _ = std::fs::write(&none, b"");
        return None;
    }
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.body_mut().with_config().limit(MAX_BYTES).read_to_vec().ok()?;
    let img = decode(&bytes)?;
    let _ = std::fs::write(&file, &bytes);
    Some(img)
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() == 0 || img.height() == 0 || img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        return None;
    }
    let rgba = img.to_rgba8();
    let size = [usize::try_from(rgba.width()).ok()?, usize::try_from(rgba.height()).ok()?];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_sha256_of_the_normalised_address() {
        // sha256("test@example.com")
        assert_eq!(gravatar_hash("  Test@Example.com "), "973dfe463ec85785f5f95af5ba3906eedb2d931c24e69824a89ea65dba4e813b");
    }

    #[test]
    fn junk_never_panics() {
        assert!(decode(b"").is_none());
        assert!(decode(b"\x89PNG\r\n\x1a\njunk").is_none());
        assert!(decode(&[0xff, 0xd8, 0xff, 0, 1, 2]).is_none());
        let a = Avatars::new(None);
        assert!(a.get("a@b.c").is_none());
        assert!(a.get("").is_none());
    }
}
