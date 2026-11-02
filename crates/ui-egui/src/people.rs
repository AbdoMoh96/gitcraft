//! People: a nickname and a picture of your choosing for anyone who appears in commits — you or
//! anyone else. Kept in preferences by e-mail address, so they follow the person into every
//! repository. A picture is their Gravatar (the default), their initials, one of the built-in
//! cats, or an image file imported into the settings folder.
//!
//! The person page opens in the center when a name or picture in the commit panel is clicked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Color32, Frame, Margin, RichText, Sense, Stroke, vec2};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::GitApp;
use crate::theme::Tokens;

macro_rules! cats {
    ($($n:literal),*) => { [$(include_bytes!(concat!("../../../assets/cats/", stringify!($n), ".png")) as &[u8]),*] };
}

/// The built-in cat pictures (`assets/cats/N.png`), numbered from 1.
pub const CATS: [&[u8]; 40] = cats![
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
    39, 40
];

/// Most people remembered.
pub const MAX_PEOPLE: usize = 4096;
/// Longest nickname, in characters.
pub const MAX_NICKNAME: usize = 60;
/// Largest image file imported, in bytes, and largest side.
const MAX_IMPORT_BYTES: u64 = 20 << 20;
const MAX_IMPORT_SIDE: u32 = 8192;
/// Side of an imported picture as kept.
const KEPT_SIDE: u32 = 256;

/// What someone is shown as.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Person {
    /// Shown instead of their name (empty: their name).
    pub nickname: String,
    pub picture: Picture,
}

impl Person {
    pub fn is_default(&self) -> bool {
        self.nickname.is_empty() && self.picture == Picture::Default
    }
}

/// A person's picture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Picture {
    /// Their Gravatar, or their initials when they have none.
    #[default]
    Default,
    /// Their initials, never a picture.
    Initials,
    /// A built-in cat, 1 to [`CATS`]`.len()`.
    Cat(u8),
    /// An imported image: a file name in the pictures folder.
    File(String),
}

impl Picture {
    /// `default`, `initials`, `cat:N` or `file:NAME`; anything else is the default.
    pub fn from_code(s: &str) -> Picture {
        match s.trim() {
            "initials" => Picture::Initials,
            s => {
                if let Some(n) = s.strip_prefix("cat:").and_then(|n| n.parse::<u8>().ok()).filter(|&n| n >= 1 && usize::from(n) <= CATS.len()) {
                    Picture::Cat(n)
                } else if let Some(f) = s.strip_prefix("file:").filter(|f| valid_file_name(f)) {
                    Picture::File(f.to_string())
                } else {
                    Picture::Default
                }
            }
        }
    }

    pub fn code(&self) -> String {
        match self {
            Picture::Default => "default".into(),
            Picture::Initials => "initials".into(),
            Picture::Cat(n) => format!("cat:{n}"),
            Picture::File(f) => format!("file:{f}"),
        }
    }
}

impl From<String> for Picture {
    fn from(s: String) -> Picture {
        Picture::from_code(&s)
    }
}

impl From<Picture> for String {
    fn from(p: Picture) -> String {
        p.code()
    }
}

/// An imported picture's file name: what [`import`] writes (hex digits and `.png`), nothing that
/// could leave the pictures folder.
fn valid_file_name(f: &str) -> bool {
    f.strip_suffix(".png").is_some_and(|stem| !stem.is_empty() && stem.len() <= 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The key a person is kept under: their e-mail address, trimmed and lower-cased; `None` when it
/// isn't one.
pub fn key(email: &str) -> Option<String> {
    let k = email.trim().to_lowercase();
    let ok = !k.is_empty() && k.len() <= 254 && k.contains('@') && !k.chars().any(|c| c.is_control() || c.is_whitespace() || c == '<' || c == '>');
    ok.then_some(k)
}

/// A nickname as typed: one line, trimmed, at most [`MAX_NICKNAME`] characters.
pub fn clean_nickname(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(MAX_NICKNAME).collect::<String>().trim().to_string()
}

/// Copy the image at `src` into `dir` as a square PNG and return its file name.
pub fn import(src: &Path, dir: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", src.display()));
    }
    if meta.len() > MAX_IMPORT_BYTES {
        return Err("the image is larger than 20 MB".into());
    }
    let bytes = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let png = square_png(&bytes)?;
    let hash: String = Sha256::digest(&png).iter().take(16).map(|b| format!("{b:02x}")).collect();
    let name = format!("{hash}.png");
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join(&name), &png).map_err(|e| format!("saving the picture: {e}"))?;
    Ok(name)
}

/// `bytes` as an image cropped to its middle square and scaled to [`KEPT_SIDE`], as PNG.
fn square_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMPORT_SIDE);
    limits.max_image_height = Some(MAX_IMPORT_SIDE);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| format!("not an image GitCraft can read ({e})"))?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("the image is empty".into());
    }
    let side = w.min(h);
    let sq = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    let out = if side > KEPT_SIDE { sq.resize_exact(KEPT_SIDE, KEPT_SIDE, image::imageops::FilterType::Triangle) } else { sq };
    let mut png = Vec::new();
    out.to_rgba8().write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(png)
}

/// Remove imported pictures nobody uses any more.
fn prune(dir: &Path, people: &BTreeMap<String, Person>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if valid_file_name(&name) && !people.values().any(|p| p.picture == Picture::File(name.clone())) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The person page on screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersonView {
    pub email: String,
    /// The name they commit under (as last seen).
    pub name: String,
    /// The nickname being typed.
    pub nickname: String,
    /// It's you (the author of commits made here).
    pub you: bool,
}

impl GitApp {
    /// Your commit identity: the active profile's, else what git would author a commit as here.
    pub fn my_identity(&self) -> Option<(String, String)> {
        let p = self.prefs.profiles.iter().find(|p| p.name == self.prefs.active_profile && !self.prefs.active_profile.is_empty());
        let ident = self.session.repo.as_ref().and_then(|r| r.run(&["var", "GIT_AUTHOR_IDENT"]).ok()).filter(|o| o.ok()).map(|o| o.text());
        // `Name <email> 1700000000 +0000`
        let (git_name, git_email) = ident
            .as_deref()
            .and_then(|i| {
                let (name, rest) = i.split_once('<')?;
                let (email, _) = rest.split_once('>')?;
                Some((name.trim().to_string(), email.trim().to_string()))
            })
            .unwrap_or_default();
        let name = p.map(|p| p.git_name.clone()).filter(|n| !n.is_empty()).unwrap_or(git_name);
        let email = p.map(|p| p.git_email.clone()).filter(|e| !e.is_empty()).unwrap_or(git_email);
        (!email.is_empty()).then_some((name, email))
    }

    /// The name `email` last committed under in the open repository.
    fn name_in_history(&self, email: &str) -> Option<String> {
        self.session.data.commits.iter().find(|c| key(&c.author.email).as_deref() == Some(email)).map(|c| c.author.name.clone())
    }

    fn person_json(&self, email: &str) -> Value {
        let p = self.prefs.people.get(email).cloned().unwrap_or_default();
        json!({"email": email, "nickname": p.nickname, "picture": p.picture.code()})
    }

    /// `ui.person`: open someone's page (`email`, default you), `set` their `nickname` and
    /// `picture` (`default`, `initials`, `cat:N`) or an `image` file to import, `reset` them to
    /// defaults, `list` everyone with a nickname or picture, or `close` the page.
    pub fn person_command(&mut self, v: &Value) -> Result<Value, String> {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        let action = s("action").unwrap_or("open");
        if action == "list" {
            let all: Vec<Value> = self.prefs.people.keys().map(|e| self.person_json(e)).collect();
            return Ok(json!({"people": all}));
        }
        if action == "close" {
            self.person = None;
            return Ok(json!({"open": false}));
        }
        let me = self.my_identity();
        let email = match s("email") {
            Some(e) => key(e).ok_or("`email` is an e-mail address")?,
            None => me.as_ref().and_then(|m| key(&m.1)).ok_or("`email` is required (git has no author identity here)")?,
        };
        let you = me.as_ref().and_then(|m| key(&m.1)).as_deref() == Some(email.as_str());
        match action {
            "open" => {
                let name = s("name")
                    .map(|n| n.chars().filter(|c| !c.is_control()).take(200).collect::<String>())
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| self.name_in_history(&email))
                    .or_else(|| me.as_ref().filter(|_| you).map(|m| m.0.clone()))
                    .unwrap_or_default();
                let nickname = self.prefs.people.get(&email).map(|p| p.nickname.clone()).unwrap_or_default();
                self.person = Some(PersonView { email: email.clone(), name, nickname, you });
            }
            "set" => {
                let mut p = self.prefs.people.get(&email).cloned().unwrap_or_default();
                if let Some(n) = v.get("nickname") {
                    p.nickname = clean_nickname(n.as_str().ok_or("`nickname` is text")?);
                }
                if let Some(path) = s("image") {
                    let dir = self.services.pictures.clone().ok_or("there is no settings folder to keep pictures in")?;
                    p.picture = Picture::File(import(Path::new(path), &dir)?);
                } else if let Some(code) = v.get("picture") {
                    let code = code.as_str().ok_or("`picture` is default, initials or cat:N")?;
                    let pic = Picture::from_code(code);
                    if pic == Picture::Default && code.trim() != "default" {
                        return Err(format!("`picture` is default, initials or cat:1 to cat:{} (not `{code}`)", CATS.len()));
                    }
                    if matches!(pic, Picture::File(_)) {
                        return Err("pass an `image` file to use a picture of your own".into());
                    }
                    p.picture = pic;
                }
                if !self.prefs.people.contains_key(&email) && self.prefs.people.len() >= MAX_PEOPLE {
                    return Err(format!("at most {MAX_PEOPLE} people"));
                }
                if p.is_default() {
                    self.prefs.people.remove(&email);
                } else {
                    self.prefs.people.insert(email.clone(), p.clone());
                }
                if let Some(view) = self.person.as_mut().filter(|x| x.email == email) {
                    view.nickname = p.nickname.clone();
                }
                self.people_changed();
            }
            "reset" => {
                self.prefs.people.remove(&email);
                if let Some(view) = self.person.as_mut().filter(|x| x.email == email) {
                    view.nickname.clear();
                }
                self.people_changed();
            }
            other => return Err(format!("`action` is open, set, reset, list or close (not `{other}`)")),
        }
        let mut out = self.person_json(&email);
        out["you"] = json!(you);
        Ok(out)
    }

    /// Pictures and nicknames changed: repaint them, forget unused imported pictures.
    pub fn people_changed(&mut self) {
        self.avatars.set_people(&self.prefs.people);
        if let Some(dir) = &self.services.pictures {
            prune(dir, &self.prefs.people);
        }
    }
}

/// Folder kept imported pictures in, under the settings folder.
pub fn pictures_dir(settings: &Path) -> PathBuf {
    settings.join("pictures")
}

/// The person page.
pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(mut view) = app.person.take() else { return };
    let current = app.prefs.people.get(&view.email).cloned().unwrap_or_default();
    let mut set: Option<Value> = None;
    let mut close = false;
    let shown = if current.nickname.is_empty() { view.name.clone() } else { current.nickname.clone() };
    Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 5)).stroke(Stroke::new(1.0, t.border)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let title = if shown.is_empty() { view.email.clone() } else { shown.clone() };
            ui.label(RichText::new(format!("Person · {title}")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").on_hover_text("Esc").clicked() {
                    close = true;
                }
            });
        });
    });
    egui::ScrollArea::vertical().id_salt("person").auto_shrink([false, false]).show(ui, |ui| {
        Frame::NONE.inner_margin(Margin::same(20)).show(ui, |ui| {
            ui.set_max_width(720.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(112.0, 112.0), Sense::hover());
                app.avatars.paint_or_initials(ui.painter(), r.center(), 54.0, &view.email, &view.name, Color32::WHITE);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if shown.is_empty() { "Unknown" } else { shown.as_str() }).strong().size(20.0));
                        if view.you {
                            ui.label(RichText::new("You").small().color(t.accent));
                        }
                    });
                    if !current.nickname.is_empty() && !view.name.is_empty() {
                        ui.label(RichText::new(&view.name).color(t.dim));
                    }
                    ui.label(RichText::new(&view.email).small().color(t.dim));
                    let commits = app.session.data.commits.iter().filter(|c| key(&c.author.email).as_deref() == Some(view.email.as_str())).count();
                    ui.label(
                        RichText::new(format!("{commits} commit{} in this repository", if commits == 1 { "" } else { "s" })).small().color(t.dim),
                    );
                });
            });
            ui.add_space(16.0);
            ui.label(RichText::new("NICKNAME").small().strong());
            ui.label(
                RichText::new("Shown instead of their name in the graph and the commit panel, in every repository. Only on this computer.")
                    .small()
                    .color(t.dim),
            );
            ui.horizontal(|ui| {
                let hint = if view.name.is_empty() { "Nickname".to_string() } else { view.name.clone() };
                let r = ui.add(egui::TextEdit::singleline(&mut view.nickname).hint_text(hint).char_limit(MAX_NICKNAME).desired_width(260.0));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let dirty = clean_nickname(&view.nickname) != current.nickname;
                if (ui.add_enabled(dirty, egui::Button::new("Save")).clicked() || (enter && dirty)) && !view.nickname.contains('\n') {
                    set = Some(json!({"nickname": view.nickname}));
                }
                if !current.nickname.is_empty() && ui.button("Clear").clicked() {
                    set = Some(json!({"nickname": ""}));
                }
            });
            ui.add_space(16.0);
            ui.label(RichText::new("PICTURE").small().strong());
            ui.horizontal_wrapped(|ui| {
                let gravatar = if app.prefs.gravatar { "Default (Gravatar)" } else { "Default (Gravatar is off in Preferences)" };
                if ui.selectable_label(current.picture == Picture::Default, gravatar).on_hover_text("Their Gravatar, or their initials").clicked() {
                    set = Some(json!({"picture": "default"}));
                }
                if ui.selectable_label(current.picture == Picture::Initials, "Initials").clicked() {
                    set = Some(json!({"picture": "initials"}));
                }
                let can_pick = app.services.pick_file.is_some() && app.services.pictures.is_some();
                let up =
                    ui.add_enabled(can_pick, egui::Button::new("Upload Image…")).on_hover_text("PNG, JPEG, GIF, WebP or BMP; cropped to a square");
                if up.clicked()
                    && let Some(pick) = &app.services.pick_file
                    && let Some(path) = pick("Choose a picture")
                {
                    set = Some(json!({"image": path.display().to_string()}));
                }
                if matches!(current.picture, Picture::File(_)) {
                    ui.label(RichText::new("Using an uploaded image").small().color(t.accent));
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Or pick a cat:").small().color(t.dim));
            ui.add_space(4.0);
            let cell = 56.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for n in 1..=CATS.len() {
                    let Ok(n8) = u8::try_from(n) else { continue };
                    let picked = current.picture == Picture::Cat(n8);
                    let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                    let p = ui.painter();
                    if picked {
                        p.circle_filled(r.center(), cell / 2.0, t.accent);
                    } else if resp.hovered() {
                        p.circle_filled(r.center(), cell / 2.0, t.hover);
                    }
                    if !app.avatars.paint_cat(p, r.center(), cell / 2.0 - 3.0, n8) {
                        p.circle_filled(r.center(), cell / 2.0 - 3.0, t.panel);
                    }
                    if resp.on_hover_text(format!("Cat {n}")).clicked() {
                        set = Some(json!({"picture": format!("cat:{n}")}));
                    }
                }
            });
            ui.add_space(20.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!current.is_default(), egui::Button::new("Restore Defaults"))
                    .on_hover_text("Their own name and their Gravatar (or initials)")
                    .clicked()
                {
                    set = Some(json!({"action": "reset"}));
                }
                ui.label(RichText::new("Nicknames and pictures stay on this computer; nothing is written to git.").small().color(t.dim));
            });
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
        close = true;
    }
    let email = view.email.clone();
    if !close {
        app.person = Some(view);
    }
    if let Some(mut params) = set {
        if params.get("action").is_none() {
            params["action"] = json!("set");
        }
        params["email"] = json!(email);
        match app.person_command(&params) {
            Ok(_) => app.status("Saved", false),
            Err(e) => app.status(e, true),
        }
    }
}

/// How long the hover card takes to grow in, in seconds.
const CARD_GROW_S: f32 = 0.18;

/// How far the hover card for `id` has grown in (0 to 1): call every frame with whether its
/// target is hovered, so it starts small again each time the pointer comes back.
pub fn card_growth(ctx: &egui::Context, id: egui::Id, hovered: bool) -> f32 {
    ctx.animate_bool_with_time(id.with("person card"), hovered, CARD_GROW_S)
}

/// The card shown under the pointer over someone's picture: a large picture growing in, their
/// nickname and full name, address and `when` (a date line). `grow` is from [`card_growth`].
pub fn hover_card(ui: &mut egui::Ui, avatars: &crate::avatars::Avatars, email: &str, name: &str, when: Option<&str>, hint: Option<&str>, grow: f32) {
    let t = Tokens::get(ui.ctx());
    let k = grow.clamp(0.0, 1.0);
    // Ease out: fast at first, settling at full size.
    let e = 1.0 - (1.0 - k) * (1.0 - k);
    ui.set_opacity(0.35 + 0.65 * e);
    ui.horizontal(|ui| {
        // A fixed box so the card itself doesn't change size while the picture grows.
        let full = 48.0;
        let (r, _) = ui.allocate_exact_size(vec2(full * 2.0, full * 2.0), Sense::hover());
        avatars.paint_or_initials(ui.painter(), r.center(), full * (0.55 + 0.45 * e), email, name, Color32::WHITE);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.add_space(10.0);
            let shown = avatars.display_name(email, name);
            ui.label(RichText::new(if shown.is_empty() { email } else { shown }).strong().size(16.0));
            if shown != name && !name.is_empty() {
                ui.label(RichText::new(name).color(t.text));
            }
            ui.label(RichText::new(email).small().color(t.dim));
            if let Some(w) = when {
                ui.label(RichText::new(w).small().color(t.dim));
            }
            if let Some(h) = hint {
                ui.add_space(2.0);
                ui.label(RichText::new(h).small().color(t.accent));
            }
        });
    });
}

/// A name that opens `email`'s page when clicked.
pub fn clickable_name(app: &mut GitApp, ui: &mut egui::Ui, email: &str, name: &str, text: RichText) -> bool {
    let r =
        ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_text("Nickname and picture…").on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.clicked() {
        open(app, email, name);
        return true;
    }
    false
}

/// Open `email`'s page.
pub fn open(app: &mut GitApp, email: &str, name: &str) {
    if let Err(e) = app.person_command(&json!({"email": email, "name": name})) {
        app.status(e, true);
    }
}

/// The name to show for someone: their nickname, else `name`.
pub fn display_name<'a>(people: &'a BTreeMap<String, Person>, email: &str, name: &'a str) -> &'a str {
    key(email).and_then(|k| people.get(&k)).map(|p| p.nickname.as_str()).filter(|n| !n.is_empty()).unwrap_or(name)
}
