//! Authors' pictures: the one picked for them on their person page ([`crate::people`]: a cat or an
//! imported image), else their Gravatar, fetched on background threads, kept on disk for a week.
//! Painted as circles. Anyone without a picture (or while it loads, or offline) gets their
//! initials on a colour of their own.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::epaint::RectShape;
use egui::{Align2, Color32, ColorImage, CornerRadius, FontId, Painter, Pos2, Rect, TextureHandle, TextureId, TextureOptions, pos2, vec2};
use sha2::{Digest, Sha256};

use crate::people::{self, CATS, Person, Picture};
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
    /// Nicknames and pictures picked for people, by [`people::key`].
    people: BTreeMap<String, Person>,
    /// Where imported pictures are.
    pictures: Option<PathBuf>,
    /// Cats and imported pictures, decoded (`None`: unreadable).
    local: RefCell<HashMap<Picture, Option<TextureHandle>>>,
}

impl Avatars {
    /// Pictures cached in `dir`; `None` never looks anything up.
    pub fn new(dir: Option<PathBuf>) -> Self {
        let (done_tx, done_rx) = channel();
        let enabled = dir.is_some();
        Avatars {
            dir,
            enabled,
            entries: RefCell::default(),
            requests: RefCell::default(),
            done_tx,
            done_rx,
            people: BTreeMap::new(),
            pictures: None,
            local: RefCell::default(),
        }
    }

    /// The folder imported pictures are in.
    pub fn set_pictures_dir(&mut self, dir: Option<PathBuf>) {
        self.pictures = dir;
        self.local.get_mut().clear();
    }

    /// The nicknames and pictures picked for people.
    pub fn set_people(&mut self, people: &BTreeMap<String, Person>) {
        self.people = people.clone();
        // An imported picture may have been replaced under the same name: decode again.
        self.local.get_mut().retain(|k, _| matches!(k, Picture::Cat(_)));
    }

    /// The name to show for someone: their nickname, else `name`.
    pub fn display_name<'a>(&'a self, email: &str, name: &'a str) -> &'a str {
        people::display_name(&self.people, email, name)
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

    /// `email`'s picture, when it has loaded; asks Gravatar for it the first time.
    pub fn get(&self, ctx: &egui::Context, email: &str) -> Option<TextureId> {
        let key = people::key(email)?;
        match self.people.get(&key).map(|p| &p.picture) {
            Some(Picture::Initials) => return None,
            Some(pic @ (Picture::Cat(_) | Picture::File(_))) => {
                // An imported picture gone missing falls back to Gravatar.
                if let Some(t) = self.local(ctx, pic) {
                    return Some(t);
                }
            }
            _ => {}
        }
        if !self.enabled {
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

    /// A cat or an imported picture, decoded once.
    fn local(&self, ctx: &egui::Context, pic: &Picture) -> Option<TextureId> {
        let mut m = self.local.try_borrow_mut().ok()?;
        if let Some(t) = m.get(pic) {
            return t.as_ref().map(TextureHandle::id);
        }
        let img = match pic {
            Picture::Cat(n) => usize::from(*n).checked_sub(1).and_then(|i| CATS.get(i)).and_then(|b| decode(b)),
            Picture::File(f) => {
                let path = self.pictures.as_ref()?.join(f);
                let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_BYTES * 4);
                small.then(|| std::fs::read(&path).ok()).flatten().and_then(|b| decode(&b))
            }
            _ => None,
        };
        let t = img.map(|img| ctx.load_texture(format!("picture:{}", pic.code()), img, TextureOptions::LINEAR));
        let id = t.as_ref().map(TextureHandle::id);
        m.insert(pic.clone(), t);
        id
    }

    /// Paint built-in cat `n` as a circle; false when it can't be shown.
    pub fn paint_cat(&self, p: &Painter, c: Pos2, r: f32, n: u8) -> bool {
        let Some(id) = self.local(p.ctx(), &Picture::Cat(n)) else { return false };
        circle(p, c, r, id, Color32::WHITE);
        true
    }

    /// Paint `email`'s picture as a circle of radius `r`, faded by `tint`; false when there is none.
    pub fn paint(&self, p: &Painter, c: Pos2, r: f32, email: &str, tint: Color32) -> bool {
        let Some(id) = self.get(p.ctx(), email) else { return false };
        circle(p, c, r, id, tint);
        true
    }

    /// The picture, or the person's initials on their colour.
    pub fn paint_or_initials(&self, p: &Painter, c: Pos2, r: f32, email: &str, name: &str, tint: Color32) {
        if !self.paint(p, c, r, email, tint) {
            initials(p, c, r, name, tint);
        }
    }
}

fn circle(p: &Painter, c: Pos2, r: f32, id: TextureId, tint: Color32) {
    let rect = Rect::from_center_size(c, vec2(r * 2.0, r * 2.0));
    let round = CornerRadius::same(r.round().clamp(0.0, 255.0) as u8);
    p.add(RectShape::filled(rect, round, tint).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
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
        let ctx = egui::Context::default();
        let mut a = Avatars::new(None);
        assert!(a.get(&ctx, "a@b.c").is_none());
        assert!(a.get(&ctx, "").is_none());
        // A picture that isn't there, a cat out of range: no picture, no panic.
        let mut people = BTreeMap::new();
        people.insert("a@b.c".to_string(), Person { nickname: String::new(), picture: Picture::File("00ff.png".into()) });
        people.insert("c@d.e".to_string(), Person { nickname: String::new(), picture: Picture::Cat(200) });
        a.set_people(&people);
        assert!(a.get(&ctx, "a@b.c").is_none());
        assert!(a.get(&ctx, "c@d.e").is_none());
    }

    #[test]
    fn every_cat_decodes_and_paints_instead_of_gravatar() {
        for (i, b) in CATS.iter().enumerate() {
            assert!(decode(b).is_some(), "cat {}", i + 1);
        }
        let ctx = egui::Context::default();
        let mut a = Avatars::new(None);
        let mut people = BTreeMap::new();
        people.insert("ada@example.com".to_string(), Person { nickname: "Countess".into(), picture: Picture::Cat(7) });
        a.set_people(&people);
        assert!(a.get(&ctx, " Ada@Example.com").is_some(), "a cat even with Gravatar off");
        assert_eq!(a.display_name("ADA@example.com", "Ada Lovelace"), "Countess");
        assert_eq!(a.display_name("bob@example.com", "Bob"), "Bob");
    }
}
