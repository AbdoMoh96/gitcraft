//! GitCraft's egui front end: a Git client window (toolbar, branch panel, commit graph, commit
//! panel, diff view, dialogs, command palette) over `gitcraft-engine`.
//!
//! The UI is thin: it reads `Session` state and acts through [`GitApp::run`] (command ids), so
//! the toolbar, menus, shortcuts, palette, control channel and MCP all reach the same behaviour.
//! Network commands (`CommandSpec::slow`) run on a background thread so the window stays live.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod avatars;
pub mod commit_panel;
pub mod control;
pub mod dialogs;
pub mod diff_view;
pub mod graph_view;
pub mod icons;
pub mod palette;
pub mod refs_panel;
pub mod start;
pub mod theme;
pub mod toasts;
pub mod toolbar;
pub mod util;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use gitcraft_engine::{CmdError, CmdResult, Session};
use serde_json::{Value, json};

pub use control::ControlRequest;
pub use dialogs::Dialog;
pub use theme::Appearance;

/// What the host app provides: things that need the platform (a folder picker).
#[derive(Default)]
pub struct Services {
    /// Ask the user for a folder (blocking); `None` when cancelled or unavailable.
    pub pick_folder: Option<Box<dyn Fn(&str) -> Option<PathBuf>>>,
    /// Folder for downloaded author pictures; `None` turns Gravatar off (tests, headless shots).
    pub avatar_cache: Option<PathBuf>,
}

/// Preferences kept between runs (`ui.json` in the settings folder).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub appearance: Appearance,
    /// Repositories opened, newest first.
    pub recent: Vec<String>,
    /// Open the last repository at start.
    pub reopen_last: bool,
    pub context_lines: u32,
    pub commit_limit: usize,
    /// Show authors' Gravatar pictures (their e-mail's hash is sent to gravatar.com).
    pub gravatar: bool,
    /// The terminal to open (`kitty --directory {dir}`); empty: the system's.
    pub terminal: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            appearance: Appearance::System,
            recent: Vec::new(),
            reopen_last: true,
            context_lines: 3,
            commit_limit: 2000,
            gravatar: true,
            terminal: String::new(),
        }
    }
}

/// Recent repositories kept.
pub const MAX_RECENT: usize = 12;

impl Prefs {
    pub fn from_json(bytes: &[u8]) -> Option<Prefs> {
        serde_json::from_slice(bytes).ok()
    }
    pub fn add_recent(&mut self, path: &Path) {
        let p = path.display().to_string();
        self.recent.retain(|r| *r != p);
        self.recent.insert(0, p);
        self.recent.truncate(MAX_RECENT);
    }
}

/// A network command running in the background.
pub struct Job {
    pub id: &'static str,
    pub label: &'static str,
    pub started: f64,
    rx: Receiver<CmdResult>,
    /// A control-channel request waiting for the result.
    reply: Option<Sender<Value>>,
}

/// The status line.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub at: f64,
}

/// Lines picked in the diff view for line staging.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinePick {
    pub path: String,
    pub hunk: usize,
    pub lines: std::collections::BTreeSet<usize>,
}

pub struct GitApp {
    pub session: Session,
    pub prefs: Prefs,
    pub services: Services,
    pub dialog: Option<Dialog>,
    pub palette: Option<palette::Palette>,
    pub jobs: Vec<Job>,
    pub toast: Option<Toast>,
    /// Notifications sliding in at the bottom left.
    pub toasts: toasts::Toasts,
    pub ref_filter: String,
    pub pick: LinePick,
    /// Scroll the graph to the selected row on the next frame.
    pub scroll_to_selection: bool,
    pub quit_requested: bool,
    control: Option<Receiver<ControlRequest>>,
    /// Control requests waiting for a screenshot.
    screenshot_waiters: Vec<Sender<Value>>,
    last_check: f64,
    was_focused: bool,
    /// Cached blame for the diff view: (path, generation) → result.
    pub blame: Option<((String, u64), Result<gitcraft_engine::git::Blame, String>)>,
    /// Cached details of the selected commit: (oid, generation) → result.
    pub details: Option<((String, u64), Result<Value, String>)>,
    /// Text to put on the clipboard on the next frame.
    pending_copy: Option<String>,
    /// A commit message being edited in the commit panel.
    pub reword: Option<Reword>,
    /// Gravatar pictures of commit authors.
    pub avatars: avatars::Avatars,
    /// The menu after dropping a branch on another one in the graph.
    pub drop_menu: Option<graph_view::DropMenu>,
    /// The list of every label on a hovered graph pill.
    pub label_popup: Option<graph_view::LabelPopup>,
}

/// The commit panel's message editor for an existing commit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reword {
    pub oid: String,
    pub summary: String,
    pub description: String,
    /// Put the keyboard in the summary on the next frame.
    pub focus: bool,
}

/// How often to look for changes made outside GitCraft while the window has focus.
const CHECK_EVERY_S: f64 = 3.0;

impl GitApp {
    pub fn new(session: Session, prefs: Prefs, services: Services) -> Self {
        let avatars = avatars::Avatars::new(services.avatar_cache.clone().filter(|_| prefs.gravatar));
        let mut s = session;
        s.view.context = prefs.context_lines.min(100);
        s.view.commit_limit = prefs.commit_limit.clamp(100, 100_000);
        GitApp {
            session: s,
            prefs,
            services,
            dialog: None,
            palette: None,
            jobs: Vec::new(),
            toast: None,
            toasts: toasts::Toasts::default(),
            ref_filter: String::new(),
            pick: LinePick::default(),
            scroll_to_selection: true,
            quit_requested: false,
            control: None,
            screenshot_waiters: Vec::new(),
            last_check: 0.0,
            was_focused: true,
            blame: None,
            details: None,
            pending_copy: None,
            reword: None,
            avatars,
            drop_menu: None,
            label_popup: None,
        }
    }

    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control = Some(rx);
        self
    }

    /// Report a result: in the status bar and as a notification.
    pub fn status(&mut self, text: impl Into<String>, error: bool) {
        self.notify(None, text, error);
    }

    /// [`GitApp::status`] with the notification titled (the command's name).
    pub fn notify(&mut self, title: Option<&str>, text: impl Into<String>, error: bool) {
        let text = text.into();
        self.toasts.push(title, text.clone(), error);
        self.toast = Some(Toast { text, error, at: util::now_s() });
    }

    /// Open the repository containing `path`; remembered in the recent list.
    pub fn open_repo(&mut self, path: &Path) -> Result<(), String> {
        match self.session.open(path) {
            Ok(()) => {
                let root = self.session.repo.as_ref().map(|r| r.root().to_path_buf()).unwrap_or_else(|| path.to_path_buf());
                self.prefs.add_recent(&root);
                self.after_open();
                Ok(())
            }
            Err(e) => {
                let m = e.to_string();
                self.status(m.clone(), true);
                Err(m)
            }
        }
    }

    fn after_open(&mut self) {
        self.pick = LinePick::default();
        self.blame = None;
        self.details = None;
        self.scroll_to_selection = true;
        let name = self.session.repo.as_ref().map(|r| r.name()).unwrap_or_default();
        self.status(format!("Opened {name}"), false);
    }

    /// Run a command by id: `ui.*` here, slow ones in the background, the rest in the session.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        self.run_with_reply(id, params, None)
    }

    pub(crate) fn run_with_reply(&mut self, id: &str, params: Value, reply: Option<Sender<Value>>) -> Result<Value, String> {
        if id.starts_with("ui.") {
            let r = self.ui_command(id, &params);
            if let Some(tx) = reply {
                let _ = tx.send(control::reply(&r));
            }
            return r;
        }
        let reg = self.session.registry.clone();
        let Some(spec) = reg.get(id) else {
            let r = Err(CmdError::Unknown(id.into()).to_string());
            if let Some(tx) = reply {
                let _ = tx.send(control::reply(&r));
            }
            return r;
        };
        if spec.slow {
            if let Some(why) = (spec.enabled)(&self.session) {
                let r = Err(format!("{id}: {why}"));
                if let Some(tx) = reply {
                    let _ = tx.send(control::reply(&r));
                }
                return r;
            }
            self.spawn(spec.id, spec.label, params, reply);
            return Ok(json!({"started": spec.id}));
        }
        let had_repo = self.session.repo.as_ref().map(|r| r.root().to_path_buf());
        let r = self.session.run(id, &params);
        self.after_command(id, &r, had_repo.as_deref());
        let r = r.map_err(|e| e.to_string());
        if let Some(tx) = reply {
            let _ = tx.send(control::reply(&r));
        }
        r
    }

    fn after_command(&mut self, id: &str, r: &CmdResult, had_repo: Option<&Path>) {
        match r {
            Ok(v) => {
                if let Some(m) = v.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                    let conflicts = v.get("conflicts").and_then(Value::as_bool).unwrap_or(false);
                    let title = self.session.registry.get(id).map(|s| s.label);
                    self.notify(title, m.to_string(), conflicts);
                }
                let now = self.session.repo.as_ref().map(|r| r.root().to_path_buf());
                if now.is_some() && now.as_deref() != had_repo {
                    if let Some(p) = &now {
                        self.prefs.add_recent(p);
                    }
                    self.after_open();
                }
                // A remote branch pulled: show where the branch is now, or ask how to combine.
                if id == "branch.pullRemote" {
                    if v.get("diverged").and_then(Value::as_bool).unwrap_or(false) {
                        let get = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                        let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
                        self.dialog = Some(Dialog::Diverged {
                            local: get("name"),
                            remote: get("remote"),
                            ahead: n("ahead"),
                            behind: n("behind"),
                            unique: v.get("unique").and_then(Value::as_u64).unwrap_or(1),
                        });
                    } else {
                        let _ = self.session.run("graph.selectHead", &json!({}));
                        self.scroll_to_selection = true;
                    }
                }
                if id.starts_with("graph.select") || id == "commit.create" {
                    self.scroll_to_selection = true;
                }
                if !id.starts_with("view.") && !id.starts_with("graph.") {
                    self.pick.lines.clear();
                }
            }
            Err(e) => {
                let title = self.session.registry.get(id).map(|s| format!("{} failed", s.label));
                self.notify(title.as_deref(), e.to_string(), true);
            }
        }
    }

    fn spawn(&mut self, id: &'static str, label: &'static str, params: Value, reply: Option<Sender<Value>>) {
        let (tx, rx) = channel();
        let repo = self.session.repo.clone();
        let git = self.session.git.clone();
        let started = std::thread::Builder::new().name(format!("gitcraft {id}")).spawn(move || {
            let mut s = match repo {
                Some(r) => Session::detached(r),
                None => Session::new(git),
            };
            let _ = tx.send(s.run(id, &params));
        });
        match started {
            Ok(_) => {
                // Progress shows in the status bar only; the result gets a notification.
                self.toast = Some(Toast { text: format!("{label}…"), error: false, at: util::now_s() });
                self.jobs.push(Job { id, label, started: util::now_s(), rx, reply });
            }
            Err(e) => self.status(format!("{label}: {e}"), true),
        }
    }

    /// Collect finished background commands.
    fn poll_jobs(&mut self) {
        let mut done = Vec::new();
        self.jobs.retain(|j| match j.rx.try_recv() {
            Ok(r) => {
                done.push((j.id, r, j.reply.clone()));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push((j.id, Err(CmdError::Failed("the background task stopped".into())), j.reply.clone()));
                false
            }
        });
        for (id, r, reply) in done {
            let reg = self.session.registry.clone();
            if let Some(spec) = reg.get(id) {
                if let Err(e) = self.session.refresh() {
                    log::warn!("reload after {id}: {e}");
                }
                self.session.note(spec, &r);
            }
            // A clone opens what it made.
            if let Ok(v) = &r
                && let Some(p) = v.get("open").and_then(Value::as_str)
            {
                let _ = self.open_repo(Path::new(p));
            }
            let had = self.session.repo.as_ref().map(|x| x.root().to_path_buf());
            self.after_command(id, &r, had.as_deref());
            if let Some(tx) = reply {
                let _ = tx.send(control::reply(&r.map_err(|e| e.to_string())));
            }
        }
    }

    /// The `ui.*` commands.
    pub fn ui_command(&mut self, id: &str, v: &Value) -> Result<Value, String> {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        match id {
            "ui.commandPalette" => {
                self.palette = if self.palette.is_some() { None } else { Some(palette::Palette::default()) };
                Ok(json!({"open": self.palette.is_some()}))
            }
            "ui.shortcuts" => {
                self.dialog = Some(Dialog::Shortcuts);
                Ok(json!({}))
            }
            "ui.preferences" => {
                self.dialog = Some(Dialog::Preferences);
                Ok(json!({}))
            }
            "ui.theme" => {
                let a = match s("value") {
                    Some(c) => Appearance::from_code(c).ok_or("`value` is system, light or dark")?,
                    None => match self.prefs.appearance {
                        Appearance::Dark => Appearance::Light,
                        _ => Appearance::Dark,
                    },
                };
                self.prefs.appearance = a;
                Ok(json!({"value": a.code()}))
            }
            "ui.recent" => {
                let i = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let p = self.prefs.recent.get(i).cloned().ok_or("no such recent repository")?;
                self.open_repo(Path::new(&p))?;
                Ok(json!({"path": p}))
            }
            "ui.terminal" => {
                let root = self.session.repo.as_ref().map(|r| r.root().to_path_buf()).ok_or("no repository is open")?;
                // Unit tests run every UI command: they must not open windows.
                let prog = if cfg!(test) {
                    util::terminal_command(&root, &self.prefs.terminal).map(|c| c.0).unwrap_or_default()
                } else {
                    util::open_terminal(&root, &self.prefs.terminal)?
                };
                Ok(json!({"path": root.display().to_string(), "terminal": prog}))
            }
            "ui.revealRepo" => {
                let root = self.session.repo.as_ref().map(|r| r.root().to_path_buf()).ok_or("no repository is open")?;
                util::reveal(&root)?;
                Ok(json!({"path": root.display().to_string()}))
            }
            "ui.copySha" => {
                let oid = s("oid").map(str::to_string).or_else(|| match &self.session.sel.row {
                    gitcraft_engine::Selected::Commit(o) => Some(o.clone()),
                    _ => None,
                });
                let oid = oid.ok_or("select a commit")?;
                self.pending_copy = Some(oid.clone());
                self.status(format!("Copied {}", oid.get(..7).unwrap_or(&oid)), false);
                Ok(json!({"oid": oid}))
            }
            "ui.editMessage" => {
                if let Some(why) = self.disabled("commit.reword") {
                    return Err(why.into());
                }
                let oid = s("oid").map(str::to_string).or_else(|| match &self.session.sel.row {
                    gitcraft_engine::Selected::Commit(o) => Some(o.clone()),
                    _ => self.session.data.status.head.clone(),
                });
                let oid = oid.ok_or("select a commit")?;
                let c = self.session.data.commit(&oid).cloned().ok_or("the commit is not in the graph")?;
                self.run("graph.select", json!({"oid": c.oid}))?;
                self.reword = Some(Reword { oid: c.oid.clone(), summary: c.summary.clone(), description: c.body.trim().to_string(), focus: true });
                Ok(json!({"oid": c.oid}))
            }
            "ui.dropMenu" => {
                let from = s("from").ok_or("`from` is required")?.to_string();
                let onto = s("onto").ok_or("`onto` is required")?.to_string();
                if !self.session.data.local_branches().any(|r| r.name == onto) || from == onto {
                    return Err(format!("`{onto}` is not another local branch"));
                }
                let at = egui::pos2(
                    v.get("x").and_then(Value::as_f64).unwrap_or(420.0) as f32,
                    v.get("y").and_then(Value::as_f64).unwrap_or(160.0) as f32,
                );
                let m = graph_view::DropMenu::new(self, from, onto, at);
                let options: Vec<&str> = m.options.iter().map(|o| o.1).collect();
                let r = json!({"options": options});
                self.drop_menu = Some(m);
                Ok(r)
            }
            "ui.filterRefs" => {
                self.ref_filter = s("text").unwrap_or("").chars().take(200).collect();
                Ok(json!({"text": self.ref_filter}))
            }
            "ui.dialog" => {
                let name = s("name").ok_or("`name` is required")?;
                self.dialog = Some(Dialog::named(name, &self.session, v).ok_or_else(|| format!("no dialog `{name}`"))?);
                Ok(json!({"open": name}))
            }
            "ui.closeDialog" => {
                self.dialog = None;
                self.palette = None;
                Ok(json!({}))
            }
            "ui.quit" => {
                self.quit_requested = true;
                Ok(json!({}))
            }
            "ui.state" => Ok(json!({
                "dialog": self.dialog.as_ref().map(Dialog::name),
                "palette": self.palette.is_some(),
                "jobs": self.jobs.iter().map(|j| j.id).collect::<Vec<_>>(),
                "status": self.toast.as_ref().map(|t| json!({"text": t.text, "error": t.error})),
                "appearance": self.prefs.appearance.code(),
                "recent": self.prefs.recent,
            })),
            other => Err(format!("unknown command `{other}`")),
        }
    }

    /// Per-frame work before layout: background jobs, control requests, shortcuts, outside changes.
    pub fn logic(&mut self, ctx: &egui::Context) {
        ctx.set_theme(self.prefs.appearance.preference());
        self.poll_jobs();
        self.poll_control(ctx);
        self.avatars.poll(ctx);
        if let Some(text) = self.pending_copy.take() {
            ctx.copy_text(text);
        }
        // Folders dropped on the window open as repositories.
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.first() {
            let _ = self.open_repo(p);
        }
        self.shortcuts(ctx);
        let now = util::now_s();
        let focused = ctx.input(|i| i.focused);
        if focused && (!self.was_focused || now - self.last_check > CHECK_EVERY_S) && self.jobs.is_empty() {
            self.last_check = now;
            match self.session.refresh_if_changed() {
                Ok(true) => {
                    self.blame = None;
                    self.details = None;
                }
                Ok(false) => {}
                Err(e) => log::warn!("checking for changes: {e}"),
            }
        }
        self.was_focused = focused;
        if !self.jobs.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else if focused {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(CHECK_EVERY_S));
        }
    }

    fn poll_control(&mut self, ctx: &egui::Context) {
        let mut reqs = Vec::new();
        if let Some(rx) = &self.control {
            while let Ok(r) = rx.try_recv() {
                reqs.push(r);
            }
        }
        for r in reqs {
            match r.method.as_str() {
                "ui.screenshot" => {
                    self.screenshot_waiters.push(r.reply);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                }
                _ => {
                    let out = control::handle(self, &r.method, r.params.clone(), r.reply.clone());
                    if let Some(v) = out {
                        let _ = r.reply.send(v);
                    }
                }
            }
            ctx.request_repaint();
        }
        if !self.screenshot_waiters.is_empty() {
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let reply = control::screenshot_reply(&img);
                for w in self.screenshot_waiters.drain(..) {
                    let _ = w.send(reply.clone());
                }
            } else {
                ctx.request_repaint();
            }
        }
    }

    /// Shortcuts from the registry. While a text field has the keyboard only `Mod+…` commands that
    /// make sense there (commit, palette) run.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() || self.palette.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let mut fire: Vec<&'static str> = Vec::new();
        let reg = self.session.registry.clone();
        let palette_key = util::parse_shortcut("Mod+P");
        if let Some((m, k)) = palette_key
            && ctx.input_mut(|i| i.consume_key(m, k))
        {
            fire.push("ui.commandPalette");
        }
        for spec in reg.all() {
            if spec.shortcut.is_empty() {
                continue;
            }
            if typing && (self.reword.is_some() || !matches!(spec.id, "commit.create" | "commit.stageAllAndCommit")) {
                continue;
            }
            for k in spec.shortcut.split(" / ") {
                if let Some((m, key)) = util::parse_shortcut(k)
                    && ctx.input_mut(|i| i.consume_key(m, key))
                {
                    fire.push(spec.id);
                }
            }
        }
        for id in fire {
            match id {
                // Shortcuts for commands that need a name open their dialog.
                "branch.create" | "repo.open" => {
                    let _ = self.ui_command("ui.dialog", &json!({"name": id}));
                }
                _ => {
                    if id != "ui.commandPalette" && reg.get(id).and_then(|s| (s.enabled)(&self.session)).is_some() {
                        continue;
                    }
                    let _ = self.run(id, json!({}));
                }
            }
        }
    }

    /// Lay out the window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        toolbar::show(self, ui);
        toolbar::status_bar(self, ui);
        if self.session.repo.is_none() {
            egui::CentralPanel::default().show(ui, |ui| start::show(self, ui));
        } else {
            if self.session.view.left_panel {
                refs_panel::show(self, ui);
            }
            commit_panel::show(self, ui);
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::Tokens::get(&ctx).bg)).show(ui, |ui| {
                if self.session.sel.file.is_some() {
                    diff_view::show(self, ui);
                } else {
                    graph_view::show(self, ui);
                }
            });
        }
        dialogs::show(self, &ctx);
        palette::show(self, &ctx);
        self.toasts.show(&ctx, 24.0);
    }

    /// Whether a command is enabled, and why not.
    pub fn disabled(&self, id: &str) -> Option<&'static str> {
        if matches!(id, "ui.terminal" | "ui.revealRepo") {
            return self.session.repo.is_none().then_some("no repository is open");
        }
        match self.session.registry.get(id) {
            Some(s) => (s.enabled)(&self.session),
            None => Some("unknown command"),
        }
    }

    pub fn busy(&self, id: &str) -> bool {
        self.jobs.iter().any(|j| j.id == id)
    }
}

impl GitApp {
    /// Put text on the clipboard (on the next frame).
    pub fn copy(&mut self, text: impl Into<String>) {
        self.pending_copy = Some(text.into());
    }
}

#[cfg(test)]
mod tests;
