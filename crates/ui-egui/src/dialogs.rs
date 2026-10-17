//! Dialogs: the names and choices a command needs, and confirmations for destructive ones.
//! Enter submits, Escape (or a click outside) cancels.

use std::path::PathBuf;

use egui::{Id, RichText};
use gitcraft_engine::{Selected, Session};
use serde_json::{Value, json};

use crate::theme::{Appearance, Tokens};
use crate::{GitApp, util};

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Open { path: String },
    Clone { url: String, into: String },
    Init { path: String, branch: String },
    CreateBranch { name: String, from: Option<String>, checkout: bool },
    RenameBranch { from: String, to: String },
    CreateTag { name: String, oid: Option<String>, message: String },
    AddRemote { name: String, url: String },
    Stash { message: String, untracked: bool },
    Confirm { title: String, body: String, command: String, params: Value, action: String },
    Shortcuts,
    Preferences,
}

fn home() -> String {
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default()
}

impl Dialog {
    /// The dialog for a command id (or `shortcuts` / `preferences`), filled from the selection
    /// and `v` (`{"from": …}`, `{"oid": …}`, `{"name": …}`).
    pub fn named(name: &str, s: &Session, v: &Value) -> Option<Dialog> {
        let st = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let selected = match &s.sel.row {
            Selected::Commit(o) => Some(o.clone()),
            _ => None,
        };
        Some(match name {
            "repo.open" => Dialog::Open { path: st("path").unwrap_or_default() },
            "repo.clone" => Dialog::Clone { url: st("url").unwrap_or_default(), into: st("into").unwrap_or_else(home) },
            "repo.init" => Dialog::Init { path: st("path").unwrap_or_default(), branch: "main".into() },
            "branch.create" => Dialog::CreateBranch { name: st("name").unwrap_or_default(), from: st("from").or(selected), checkout: true },
            "branch.rename" => {
                let from = st("from").or_else(|| s.data.status.branch.clone())?;
                Dialog::RenameBranch { to: from.clone(), from }
            }
            "tag.create" => Dialog::CreateTag { name: String::new(), oid: st("oid").or(selected), message: String::new() },
            "remote.add" => Dialog::AddRemote { name: if s.data.remotes.is_empty() { "origin".into() } else { String::new() }, url: String::new() },
            "stash.save" => Dialog::Stash { message: String::new(), untracked: true },
            "shortcuts" | "ui.shortcuts" => Dialog::Shortcuts,
            "preferences" | "ui.preferences" => Dialog::Preferences,
            _ => return None,
        })
    }

    pub fn confirm(title: &str, body: &str, command: &str, params: Value, action: &str) -> Dialog {
        Dialog::Confirm { title: title.into(), body: body.into(), command: command.into(), params, action: action.into() }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Dialog::Open { .. } => "repo.open",
            Dialog::Clone { .. } => "repo.clone",
            Dialog::Init { .. } => "repo.init",
            Dialog::CreateBranch { .. } => "branch.create",
            Dialog::RenameBranch { .. } => "branch.rename",
            Dialog::CreateTag { .. } => "tag.create",
            Dialog::AddRemote { .. } => "remote.add",
            Dialog::Stash { .. } => "stash.save",
            Dialog::Confirm { .. } => "confirm",
            Dialog::Shortcuts => "shortcuts",
            Dialog::Preferences => "preferences",
        }
    }
}

/// What the dialog asks for when it closes.
enum Close {
    Keep,
    Cancel,
    Run(String, Value),
    Open(PathBuf),
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, focus: bool) -> egui::Response {
    ui.label(RichText::new(label).small().strong());
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
    if focus {
        r.request_focus();
    }
    ui.add_space(4.0);
    r
}

fn buttons(ui: &mut egui::Ui, action: &str, ok: bool, danger: bool) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    let mut submit = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fill = if danger { t.danger } else { crate::theme::APP_COLOR };
            let b = egui::Button::new(RichText::new(action).color(egui::Color32::WHITE).strong()).fill(fill);
            if ui.add_enabled(ok, b).clicked() {
                submit = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    let enter = ok && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    (submit || enter, cancel)
}

pub fn show(app: &mut GitApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let first = !ctx.memory(|m| m.data.get_temp::<bool>(Id::new("dialog_seen")).unwrap_or(false));
    ctx.memory_mut(|m| m.data.insert_temp(Id::new("dialog_seen"), true));
    let modal = egui::Modal::new(Id::new("gitcraft_dialog")).show(ctx, |ui| {
        ui.set_width(440.0);
        body(app, ui, &mut d, first)
    });
    let close = if modal.should_close() { Close::Cancel } else { modal.inner };
    match close {
        Close::Keep => app.dialog = Some(d),
        Close::Cancel => {}
        Close::Run(id, p) => {
            let _ = app.run(&id, p);
        }
        Close::Open(p) => {
            let _ = app.open_repo(&p);
        }
    }
    if app.dialog.is_none() {
        ctx.memory_mut(|m| m.data.remove::<bool>(Id::new("dialog_seen")));
    }
}

fn body(app: &mut GitApp, ui: &mut egui::Ui, d: &mut Dialog, first: bool) -> Close {
    let t = Tokens::get(ui.ctx());
    match d {
        Dialog::Open { path } => {
            ui.heading("Open Repository");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(path).hint_text("Folder inside a git repository").desired_width(330.0));
                if first {
                    r.request_focus();
                }
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Open a repository")
                {
                    *path = p.display().to_string();
                }
            });
            if !app.prefs.recent.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("RECENT").small().weak());
                for p in app.prefs.recent.clone().iter().take(6) {
                    if ui.link(p).clicked() {
                        return Close::Open(PathBuf::from(p));
                    }
                }
            }
            let (ok, cancel) = buttons(ui, "Open", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Open(PathBuf::from(path.trim()));
            }
        }
        Dialog::Clone { url, into } => {
            ui.heading("Clone Repository");
            ui.add_space(6.0);
            field(ui, "URL", url, "https://host/owner/repo.git or git@host:owner/repo.git", first);
            ui.label(RichText::new("WHERE").small().strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(into).desired_width(330.0));
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Clone into")
                {
                    *into = p.display().to_string();
                }
            });
            let name = url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".git").to_string();
            if !name.is_empty() {
                ui.label(RichText::new(format!("Clones into {}", PathBuf::from(into.as_str()).join(&name).display())).small().weak());
            }
            let (ok, cancel) = buttons(ui, "Clone", !url.trim().is_empty() && !name.is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run(
                    "repo.clone".into(),
                    json!({"url": url.trim(), "path": PathBuf::from(into.as_str()).join(&name).display().to_string()}),
                );
            }
        }
        Dialog::Init { path, branch } => {
            ui.heading("Init Repository");
            ui.add_space(6.0);
            ui.label(RichText::new("FOLDER").small().strong());
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(path).hint_text("New or existing folder").desired_width(330.0));
                if first {
                    r.request_focus();
                }
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Init a repository in")
                {
                    *path = p.display().to_string();
                }
            });
            field(ui, "INITIAL BRANCH", branch, "main", false);
            let (ok, cancel) = buttons(ui, "Create Repository", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("repo.init".into(), json!({"path": path.trim(), "branch": branch.trim()}));
            }
        }
        Dialog::CreateBranch { name, from, checkout } => {
            ui.heading("Create Branch");
            ui.add_space(6.0);
            field(ui, "NAME", name, "feature/my-change", first);
            let at = from.as_deref().map(|f| f.get(..7).unwrap_or(f).to_string()).unwrap_or_else(|| "HEAD".into());
            ui.label(RichText::new(format!("Starts at {at}")).small().weak());
            ui.checkbox(checkout, "Check it out");
            let (ok, cancel) = buttons(ui, "Create Branch", !name.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("branch.create".into(), json!({"name": name.trim(), "from": from, "checkout": *checkout}));
            }
        }
        Dialog::RenameBranch { from, to } => {
            ui.heading(format!("Rename {from}"));
            ui.add_space(6.0);
            field(ui, "NEW NAME", to, "", first);
            let (ok, cancel) = buttons(ui, "Rename", !to.trim().is_empty() && to.trim() != from, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("branch.rename".into(), json!({"from": from, "to": to.trim()}));
            }
        }
        Dialog::CreateTag { name, oid, message } => {
            ui.heading("Create Tag");
            ui.add_space(6.0);
            field(ui, "NAME", name, "v1.0.0", first);
            ui.label(RichText::new("MESSAGE (an annotated tag; leave empty for a lightweight one)").small().strong());
            ui.add(egui::TextEdit::multiline(message).desired_rows(3).desired_width(f32::INFINITY));
            let at = oid.as_deref().map(|f| f.get(..7).unwrap_or(f).to_string()).unwrap_or_else(|| "HEAD".into());
            ui.label(RichText::new(format!("Tags {at}")).small().weak());
            let (ok, cancel) = buttons(ui, "Create Tag", !name.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("tag.create".into(), json!({"name": name.trim(), "oid": oid, "message": message}));
            }
        }
        Dialog::AddRemote { name, url } => {
            ui.heading("Add Remote");
            ui.add_space(6.0);
            field(ui, "NAME", name, "origin", first);
            field(ui, "URL", url, "https://host/owner/repo.git", false);
            let (ok, cancel) = buttons(ui, "Add Remote", !name.trim().is_empty() && !url.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("remote.add".into(), json!({"name": name.trim(), "url": url.trim()}));
            }
        }
        Dialog::Stash { message, untracked } => {
            ui.heading("Stash Changes");
            ui.add_space(6.0);
            field(ui, "MESSAGE", message, "optional", first);
            ui.checkbox(untracked, "Include untracked files");
            let (ok, cancel) = buttons(ui, "Stash", true, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("stash.save".into(), json!({"message": message, "untracked": *untracked}));
            }
        }
        Dialog::Confirm { title, body, command, params, action } => {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                crate::icons::paint(ui.painter(), r, crate::icons::Icon::Warning, t.warning, t.warning);
                ui.heading(title.as_str());
            });
            ui.add_space(6.0);
            ui.label(body.as_str());
            let (ok, cancel) = buttons(ui, action, true, true);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run(command.clone(), params.clone());
            }
        }
        Dialog::Shortcuts => {
            ui.heading("Keyboard Shortcuts");
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("shortcuts").striped(true).num_columns(2).show(ui, |ui| {
                    ui.label(RichText::new(util::shortcut_label("Mod+P")).monospace());
                    ui.label("Command palette");
                    ui.end_row();
                    for c in app.session.registry.all().iter().filter(|c| !c.shortcut.is_empty()) {
                        ui.label(RichText::new(util::shortcut_label(c.shortcut)).monospace());
                        ui.label(c.label);
                        ui.end_row();
                    }
                });
            });
            if buttons(ui, "Close", true, false) != (false, false) {
                return Close::Cancel;
            }
        }
        Dialog::Preferences => {
            ui.heading("Preferences");
            ui.add_space(6.0);
            ui.label(RichText::new("INTERFACE THEME").small().strong());
            ui.horizontal(|ui| {
                for a in Appearance::ALL {
                    ui.radio_value(&mut app.prefs.appearance, a, a.label());
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("DIFF CONTEXT LINES").small().strong());
            let mut c = app.prefs.context_lines;
            if ui.add(egui::Slider::new(&mut c, 0..=20)).changed() {
                app.prefs.context_lines = c;
                app.session.view.context = c;
            }
            ui.add_space(6.0);
            ui.label(RichText::new("COMMITS LOADED IN THE GRAPH").small().strong());
            let mut n = app.prefs.commit_limit;
            if ui.add(egui::Slider::new(&mut n, 100..=50_000).logarithmic(true)).changed() {
                app.prefs.commit_limit = n;
            }
            ui.checkbox(&mut app.prefs.reopen_last, "Reopen the last repository at start");
            ui.add_space(6.0);
            let git = app.session.git.version().unwrap_or_else(|e| e.to_string());
            ui.label(RichText::new(git).small().weak());
            let (ok, cancel) = buttons(ui, "Done", true, false);
            if ok || cancel {
                if app.session.view.commit_limit != app.prefs.commit_limit {
                    app.session.view.commit_limit = app.prefs.commit_limit;
                    let _ = app.session.refresh();
                }
                return Close::Cancel;
            }
        }
    }
    Close::Keep
}
