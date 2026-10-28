//! The toolbar (repository and branch menus, undo/redo, pull, push, branch, stash, pop, search)
//! and the status bar.

use egui::{Align, Frame, Layout, Margin, RichText, Stroke};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{Dialog, GitApp, util};

/// A toolbar tool for a command: disabled with its reason, a spinner while it runs.
fn tool(app: &mut GitApp, ui: &mut egui::Ui, icon: Icon, caption: &str, id: &str) -> bool {
    let label = app.session.registry.get(id).map(|s| s.label).unwrap_or(caption);
    let key = app.session.registry.get(id).map(|s| s.shortcut).unwrap_or("");
    let tip = if key.is_empty() { label.to_string() } else { format!("{label} ({})", util::shortcut_label(key)) };
    let busy = app.busy(id);
    let why = if busy { Some("running") } else { app.disabled(id) };
    let r = icons::tool(ui, icon, caption, &tip, why);
    if busy {
        let c = r.rect.center() + egui::vec2(14.0, -12.0);
        ui.put(egui::Rect::from_center_size(c, egui::vec2(12.0, 12.0)), egui::Spinner::new().size(11.0));
    }
    r.clicked()
}

/// A menu button: an optional icon, the label, and a drawn chevron (egui's fonts have no arrows
/// that fit).
fn menu(ui: &mut egui::Ui, icon: Option<Icon>, label: RichText, add: impl FnOnce(&mut egui::Ui)) {
    let t = Tokens::get(ui.ctx());
    let pad = if icon.is_some() { "      " } else { "" };
    let label = RichText::new(format!("{pad}{}      ", label.text())).strong();
    let r = ui.menu_button(label, add).response;
    if let Some(i) = icon {
        icons::paint(
            ui.painter(),
            egui::Rect::from_center_size(r.rect.left_center() + egui::vec2(15.0, 0.0), egui::vec2(15.0, 15.0)),
            i,
            t.text,
            t.accent,
        );
    }
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(r.rect.right_center() - egui::vec2(13.0, 0.0), egui::vec2(11.0, 11.0)),
        Icon::ChevronDown,
        t.dim,
        t.dim,
    );
}

fn separator(ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 36.0), egui::Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], Stroke::new(1.0, t.border));
}

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("toolbar")
        .exact_size(56.0)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                repo_menu(app, ui);
                if app.session.repo.is_some() {
                    branch_menu(app, ui);
                }
                separator(ui);
                if tool(app, ui, Icon::Undo, "Undo", "edit.undo") {
                    let _ = app.run("edit.undo", json!({}));
                }
                if tool(app, ui, Icon::Redo, "Redo", "edit.redo") {
                    let _ = app.run("edit.redo", json!({}));
                }
                separator(ui);
                if tool(app, ui, Icon::Fetch, "Fetch", "remote.fetch") {
                    let _ = app.run("remote.fetch", json!({}));
                }
                if tool(app, ui, Icon::Pull, "Pull", "remote.pull") {
                    let _ = app.run("remote.pull", json!({}));
                }
                if tool(app, ui, Icon::Push, "Push", "remote.push") {
                    let _ = app.run("remote.push", json!({}));
                }
                actions_menu(app, ui);
                separator(ui);
                if tool(app, ui, Icon::Branch, "Branch", "branch.create") {
                    app.dialog = Dialog::named("branch.create", &app.session, &json!({}));
                }
                if tool(app, ui, Icon::Stash, "Stash", "stash.save") {
                    let _ = app.run("stash.save", json!({}));
                }
                if tool(app, ui, Icon::Pop, "Pop", "stash.pop") {
                    let _ = app.run("stash.pop", json!({}));
                }
                separator(ui);
                if tool(app, ui, Icon::Terminal, "Terminal", "ui.terminal") {
                    let _ = app.run("ui.terminal", json!({}));
                }
                if !app.conflict_warnings().is_empty() {
                    separator(ui);
                    crate::conflict_alert::show(app, ui);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let dark = ui.ctx().global_style().visuals.dark_mode;
                    if icons::button(ui, if dark { Icon::Sun } else { Icon::Moon }, "Switch light and dark", None, 18.0).clicked() {
                        let _ = app.ui_command("ui.theme", &json!({"value": if dark { "light" } else { "dark" }}));
                    }
                    if icons::button(ui, Icon::Palette, &format!("Command palette ({})", util::shortcut_label("Mod+P")), None, 18.0).clicked() {
                        let _ = app.ui_command("ui.commandPalette", &json!({}));
                    }
                    if app.session.repo.is_some() {
                        search(app, ui);
                    }
                });
            });
        });
}

fn repo_menu(app: &mut GitApp, ui: &mut egui::Ui) {
    let name = app.session.repo.as_ref().map(|r| r.name()).unwrap_or_else(|| "No repository".into());
    menu(ui, Some(Icon::Folder), RichText::new(name), |ui| {
        ui.set_min_width(240.0);
        if ui.button("Open Repository…").clicked() {
            app.dialog = Dialog::named("repo.open", &app.session, &json!({}));
            ui.close();
        }
        if ui.button("Clone Repository…").clicked() {
            app.dialog = Dialog::named("repo.clone", &app.session, &json!({}));
            ui.close();
        }
        if ui.button("Init Repository…").clicked() {
            app.dialog = Dialog::named("repo.init", &app.session, &json!({}));
            ui.close();
        }
        if !app.prefs.recent.is_empty() {
            ui.separator();
            ui.label(RichText::new("RECENT").small().weak());
            let recent = app.prefs.recent.clone();
            for (i, p) in recent.iter().enumerate().take(8) {
                if ui.button(p).clicked() {
                    let _ = app.ui_command("ui.recent", &json!({"index": i}));
                    ui.close();
                }
            }
        }
        if app.session.repo.is_some() {
            ui.separator();
            if ui.button("Show in File Manager").clicked() {
                let _ = app.ui_command("ui.revealRepo", &json!({}));
                ui.close();
            }
            if ui.button("Refresh").clicked() {
                let _ = app.run("repo.refresh", json!({}));
                ui.close();
            }
            if ui.button("Close Repository").clicked() {
                let _ = app.run("repo.close", json!({}));
                ui.close();
            }
        }
        ui.separator();
        if app.session.repo.is_some() {
            ui.menu_button("Tools", |ui| {
                for (name, label) in [
                    ("patch.apply", "Apply Patch…"),
                    ("patch.create", "Patch of the Uncommitted Changes…"),
                    ("gitflow.init", "Git Flow…"),
                    ("lfs.list", "Git LFS…"),
                    ("hooks.list", "Git Hooks…"),
                ] {
                    if ui.button(label).clicked() {
                        let params = if name == "patch.create" { json!({"oid": null}) } else { json!({}) };
                        app.dialog = Dialog::named(name, &app.session, &params);
                        if name == "patch.create"
                            && let Some(Dialog::SavePatch { oid, .. }) = app.dialog.as_mut()
                        {
                            *oid = None;
                        }
                        ui.close();
                    }
                }
            });
            for (name, label) in [("config.identity", "Author Identity…"), ("config.signing", "Commit Signing…")] {
                if ui.button(label).clicked() {
                    app.dialog = Dialog::named(name, &app.session, &json!({}));
                    ui.close();
                }
            }
        }
        if ui.button("SSH Keys…").clicked() {
            app.dialog = Dialog::named("ssh.keys", &app.session, &json!({}));
            ui.close();
        }
        if ui.button("Preferences…").clicked() {
            app.dialog = Some(Dialog::Preferences);
            ui.close();
        }
        if ui.button("Keyboard Shortcuts").clicked() {
            app.dialog = Some(Dialog::Shortcuts);
            ui.close();
        }
    });
}

fn branch_menu(app: &mut GitApp, ui: &mut egui::Ui) {
    let st = &app.session.data.status;
    let label = match (&st.branch, &st.head) {
        (Some(b), _) => b.clone(),
        (None, Some(h)) => format!("detached at {}", h.get(..7).unwrap_or(h)),
        (None, None) => "no commits".into(),
    };
    let ahead_behind = if st.ahead + st.behind > 0 { format!("  ↑{} ↓{}", st.ahead, st.behind) } else { String::new() };
    menu(ui, Some(Icon::Branch), RichText::new(format!("{label}{ahead_behind}")), |ui| {
        ui.set_min_width(220.0);
        let branches: Vec<(String, bool)> = app.session.data.local_branches().map(|r| (r.name.clone(), r.head)).collect();
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (b, head) in branches {
                let text = if head { RichText::new(format!("{b}  (current)")).strong() } else { RichText::new(b.as_str()) };
                if ui.button(text).clicked() {
                    if !head {
                        let _ = app.run("branch.checkout", json!({"name": b}));
                    }
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui.button("Create Branch…").clicked() {
            app.dialog = Dialog::named("branch.create", &app.session, &json!({}));
            ui.close();
        }
    });
}

/// The Actions menu after Push: every fetch, pull and push variant.
fn actions_menu(app: &mut GitApp, ui: &mut egui::Ui) {
    menu(ui, None, RichText::new("Actions"), |ui| {
        for id in ["remote.fetch", "remote.pull", "remote.pullFastForward", "remote.pullRebase", "remote.push", "remote.forcePush"] {
            let label = app.session.registry.get(id).map(|s| s.label).unwrap_or(id);
            let why = app.disabled(id);
            let r = ui.add_enabled(why.is_none(), egui::Button::new(label));
            if r.clicked() {
                if id == "remote.forcePush" {
                    app.dialog = Some(Dialog::confirm(
                        "Force push?",
                        "This replaces the remote branch with yours (with lease: it refuses when the remote has commits you haven't fetched).",
                        id,
                        json!({}),
                        "Force Push",
                    ));
                } else {
                    let _ = app.run(id, json!({}));
                }
                ui.close();
            }
        }
    });
}

fn search(app: &mut GitApp, ui: &mut egui::Ui) {
    let mut text = app.session.view.search.clone();
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("Search commits").desired_width(200.0));
    if r.changed() {
        let _ = app.run("view.search", json!({"text": text}));
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        // Enter jumps to the first match.
        let q = app.session.view.search.to_lowercase();
        let first = app.session.data.commits.iter().find(|c| gitcraft_engine::cmd::view::matches(c, &q)).map(|c| c.oid.clone());
        if let Some(oid) = first {
            let _ = app.run("graph.select", json!({"oid": oid}));
        }
    }
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    icons::paint(ui.painter(), rect, Icon::Search, t.dim, t.accent);
}

pub fn status_bar(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status")
        .exact_size(24.0)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(10, 3)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                for j in &app.jobs {
                    ui.add(egui::Spinner::new().size(12.0));
                    ui.label(RichText::new(format!("{}… {:.0}s", j.label, util::now_s() - j.started)).small());
                }
                if let Some(toast) = &app.toast {
                    let c = if toast.error { t.danger } else { t.text };
                    let first = toast.text.lines().next().unwrap_or("");
                    ui.label(RichText::new(first).small().color(c)).on_hover_text(&toast.text);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(repo) = &app.session.repo {
                        ui.label(RichText::new(repo.root().display().to_string()).small().weak());
                        let d = &app.session.data;
                        let n = d.commits.len();
                        ui.label(RichText::new(format!("{n}{} commits", if d.more { "+" } else { "" })).small().weak());
                        let state = d.status.state;
                        if state != gitcraft_engine::git::RepoState::Clean {
                            ui.label(RichText::new(state.label()).small().strong().color(t.warning));
                        }
                    }
                });
            });
        });
}
