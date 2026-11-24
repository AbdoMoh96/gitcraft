//! Preferences as a full page: a list of sections on the left ("Exit Preferences" at the top),
//! the chosen section's settings on the right. Repository settings follow under the open
//! repository's name. Settings that have their own dialog (profiles, identity, signing, SSH keys,
//! accounts, Git Flow, LFS, hooks) open it from their section.

use egui::{Frame, Margin, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{Appearance, Tokens};
use crate::{Dialog, GitApp, util};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    General,
    Profiles,
    Ssh,
    Integrations,
    Tools,
    Appearance,
    Signing,
    Diff,
    Terminal,
    Shortcuts,
    About,
    GitFlow,
    Hooks,
    Lfs,
}

impl Section {
    pub const GLOBAL: [Section; 11] = [
        Section::General,
        Section::Profiles,
        Section::Ssh,
        Section::Integrations,
        Section::Tools,
        Section::Appearance,
        Section::Signing,
        Section::Diff,
        Section::Terminal,
        Section::Shortcuts,
        Section::About,
    ];
    pub const REPO: [Section; 3] = [Section::GitFlow, Section::Hooks, Section::Lfs];

    pub fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Profiles => "Profiles",
            Section::Ssh => "SSH",
            Section::Integrations => "Accounts and AI",
            Section::Tools => "External Tools",
            Section::Appearance => "Appearance",
            Section::Signing => "Commit Signing",
            Section::Diff => "Diff and Graph",
            Section::Terminal => "Terminal",
            Section::Shortcuts => "Keyboard Shortcuts",
            Section::About => "About",
            Section::GitFlow => "Git Flow",
            Section::Hooks => "Git Hooks",
            Section::Lfs => "Git LFS",
        }
    }

    /// For the control channel: `ui.preferences {"section": "terminal"}`.
    pub fn code(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Profiles => "profiles",
            Section::Ssh => "ssh",
            Section::Integrations => "integrations",
            Section::Tools => "tools",
            Section::Appearance => "appearance",
            Section::Signing => "signing",
            Section::Diff => "diff",
            Section::Terminal => "terminal",
            Section::Shortcuts => "shortcuts",
            Section::About => "about",
            Section::GitFlow => "gitflow",
            Section::Hooks => "hooks",
            Section::Lfs => "lfs",
        }
    }

    pub fn parse(s: &str) -> Option<Section> {
        Section::GLOBAL.into_iter().chain(Section::REPO).find(|x| x.code().eq_ignore_ascii_case(s.trim()))
    }

    fn needs_repo(self) -> bool {
        Section::REPO.contains(&self) || self == Section::Signing
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(20.0).strong());
    ui.add_space(10.0);
}

fn group(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).small().strong());
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

/// A button that opens one of the app's dialogs.
fn open_dialog(app: &mut GitApp, ui: &mut egui::Ui, label: &str, name: &str) {
    if ui.button(label).clicked() {
        app.dialog = Dialog::named(name, &app.session, &json!({}));
        if name == "integrations.connect" {
            let _ = app.run("integrations.connect", json!({}));
        }
    }
}

/// Leave the page; settings that need work when they change get it here.
pub fn close(app: &mut GitApp) {
    app.prefs_page = None;
    if app.session.view.commit_limit != app.prefs.commit_limit {
        app.session.view.commit_limit = app.prefs.commit_limit;
        let _ = app.session.refresh();
    }
}

/// The page, in place of the toolbar and the repository view.
pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(mut current) = app.prefs_page else { return };
    let repo_name = app
        .session
        .repo
        .as_ref()
        .map(|_| app.tab_list().get(app.tabs.active).map(|(title, _)| title.clone()).unwrap_or_else(|| "this repository".into()));
    if current.needs_repo() && repo_name.is_none() {
        current = Section::General;
    }
    let mut leave = false;
    egui::Panel::left("prefs_nav")
        .exact_size(230.0)
        .resizable(false)
        .frame(Frame::NONE.fill(t.panel).inner_margin(Margin::symmetric(0, 8)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let row = |ui: &mut egui::Ui, label: &str, on: bool, icon: Option<Icon>| -> bool {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                if on {
                    ui.painter().rect_filled(rect, 0.0, t.selection);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 0.0, t.hover);
                }
                let x = rect.left() + if icon.is_some() { 38.0 } else { 22.0 };
                if let Some(i) = icon {
                    icons::paint(
                        ui.painter(),
                        egui::Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(12.0, 12.0)),
                        i,
                        t.text,
                        t.accent,
                    );
                }
                ui.painter().text(
                    pos2(x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(13.0),
                    if on { t.text } else { t.dim },
                );
                resp.clicked()
            };
            if row(ui, "Exit Preferences", false, Some(Icon::Close)) {
                leave = true;
            }
            ui.add_space(8.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for s in Section::GLOBAL {
                    if s.needs_repo() && repo_name.is_none() {
                        continue;
                    }
                    if row(ui, s.label(), current == s, None) {
                        current = s;
                    }
                }
                if let Some(name) = &repo_name {
                    ui.add_space(10.0);
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                    ui.painter().hline(rect.left() + 14.0..=rect.right() - 14.0, rect.top(), Stroke::new(1.0, t.border));
                    ui.painter().text(
                        pos2(rect.left() + 22.0, rect.top() + 13.0),
                        egui::Align2::LEFT_CENTER,
                        "This repository",
                        egui::FontId::proportional(11.0),
                        t.dim,
                    );
                    ui.painter().text(
                        pos2(rect.left() + 22.0, rect.top() + 29.0),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(12.5),
                        t.text,
                    );
                    for s in Section::REPO {
                        if row(ui, s.label(), current == s, None) {
                            current = s;
                        }
                    }
                }
            });
        });
    egui::CentralPanel::default().frame(Frame::NONE.fill(t.bg).inner_margin(Margin::symmetric(32, 22))).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(680.0);
            section(app, ui, current);
        });
    });
    if app.prefs_page.is_some() {
        app.prefs_page = Some(current);
    }
    if leave || (app.dialog.is_none() && app.palette.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
        close(app);
    }
}

fn section(app: &mut GitApp, ui: &mut egui::Ui, s: Section) {
    match s {
        Section::General => {
            heading(ui, "General");
            ui.checkbox(&mut app.prefs.reopen_last, "Reopen the last repository at start");
            ui.horizontal(|ui| {
                ui.label("Fetch automatically every");
                ui.add(egui::DragValue::new(&mut app.prefs.auto_fetch_minutes).range(0..=240).suffix(" min"));
                ui.label(RichText::new("(0: never)").small().weak());
            });
            ui.checkbox(&mut app.prefs.conflict_check, "Warn when merging the target or upstream branch would conflict")
                .on_hover_text("Merges in memory in the background after each change; nothing on disk is touched");
            group(ui, "WORKSPACES");
            note(ui, "Named groups of repositories opened together in tabs.");
            open_dialog(app, ui, "Workspaces…", "ui.workspaces");
        }
        Section::Profiles => {
            heading(ui, "Profiles");
            note(ui, "A profile keeps its own tabs, favourites and commit author.");
            let active = if app.prefs.active_profile.is_empty() { "none".to_string() } else { app.prefs.active_profile.clone() };
            ui.label(format!("In use: {active}"));
            for p in app.prefs.profiles.iter().filter(|p| !p.name.is_empty()) {
                let who = if p.git_name.is_empty() { "git's configuration".to_string() } else { format!("{} <{}>", p.git_name, p.git_email) };
                ui.label(RichText::new(format!("• {}  —  {who}", p.name)).small());
            }
            ui.add_space(4.0);
            open_dialog(app, ui, "Manage Profiles…", "ui.profile");
            if app.session.repo.is_some() {
                group(ui, "AUTHOR");
                note(ui, "The name and e-mail written into commits.");
                open_dialog(app, ui, "Author Identity…", "config.identity");
            }
        }
        Section::Ssh => {
            heading(ui, "SSH");
            note(ui, "Create a key, copy its public half, and see the keys you have.");
            open_dialog(app, ui, "SSH Keys…", "ssh.keys");
        }
        Section::Integrations => {
            heading(ui, "Accounts and AI");
            note(ui, "Hosting accounts for pull requests and issues, and the command that writes commit messages.");
            open_dialog(app, ui, "Accounts and AI…", "integrations.connect");
        }
        Section::Tools => {
            heading(ui, "External Tools");
            group(ui, "EDITOR");
            ui.add(egui::TextEdit::singleline(&mut app.prefs.editor).hint_text("System default (e.g. code -g {file})").desired_width(f32::INFINITY))
                .on_hover_text("The command that opens a file; {file} is its path");
            group(ui, "TERMINAL");
            ui.add(
                egui::TextEdit::singleline(&mut app.prefs.terminal)
                    .hint_text("System default (e.g. kitty --directory {dir})")
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text("The command that opens a terminal; {dir} is the repository's folder");
        }
        Section::Appearance => {
            heading(ui, "Appearance");
            group(ui, "INTERFACE THEME");
            ui.horizontal(|ui| {
                for a in Appearance::ALL {
                    ui.radio_value(&mut app.prefs.appearance, a, a.label());
                }
            });
            group(ui, "ZOOM");
            let mut z = app.prefs.zoom * 100.0;
            if ui
                .add(egui::Slider::new(&mut z, 50.0..=300.0).step_by(5.0).suffix("%"))
                .on_hover_text(util::shortcut_label("Mod+=") + " / " + &util::shortcut_label("Mod+-"))
                .changed()
            {
                app.prefs.zoom = z / 100.0;
            }
            group(ui, "AUTHOR PICTURES");
            if ui
                .checkbox(&mut app.prefs.gravatar, "Show author pictures from Gravatar")
                .on_hover_text("Sends a hash of each author's e-mail address to gravatar.com")
                .changed()
            {
                app.avatars.set_enabled(app.prefs.gravatar);
            }
            group(ui, "COMMIT PANEL");
            ui.checkbox(&mut app.prefs.tree_view, "Group changed files by folder");
        }
        Section::Signing => {
            heading(ui, "Commit Signing");
            note(ui, "Sign commits and tags with a GPG or SSH key.");
            open_dialog(app, ui, "Commit Signing…", "config.signing");
        }
        Section::Diff => {
            heading(ui, "Diff and Graph");
            group(ui, "DIFF CONTEXT LINES");
            let mut c = app.prefs.context_lines;
            if ui.add(egui::Slider::new(&mut c, 0..=20)).changed() {
                app.prefs.context_lines = c;
                app.session.view.context = c;
            }
            group(ui, "COMMITS LOADED IN THE GRAPH");
            let mut n = app.prefs.commit_limit;
            if ui.add(egui::Slider::new(&mut n, 100..=50_000).logarithmic(true)).changed() {
                app.prefs.commit_limit = n;
            }
            note(ui, "Applied when you leave Preferences.");
        }
        Section::Terminal => {
            heading(ui, "Terminal");
            ui.checkbox(&mut app.prefs.builtin_terminal, format!("Open the terminal inside GitCraft ({})", util::shortcut_label("Mod+`")));
            note(ui, "When off, the Terminal button opens the program set in External Tools.");
        }
        Section::Shortcuts => {
            heading(ui, "Keyboard Shortcuts");
            egui::Grid::new("prefs_shortcuts").striped(true).num_columns(2).show(ui, |ui| {
                ui.label(RichText::new(util::shortcut_label("Mod+P")).monospace());
                ui.label("Command palette");
                ui.end_row();
                for c in app.session.registry.all().iter().filter(|c| !c.shortcut.is_empty()) {
                    ui.label(RichText::new(util::shortcut_label(c.shortcut)).monospace());
                    ui.label(c.label);
                    ui.end_row();
                }
            });
        }
        Section::About => {
            heading(ui, "About");
            let git = app.session.git.version().unwrap_or_else(|e| e.to_string());
            ui.label(format!("GitCraft {}", env!("CARGO_PKG_VERSION")));
            ui.label(RichText::new(git).weak());
        }
        Section::GitFlow => {
            heading(ui, "Git Flow");
            note(ui, "Branch names and prefixes for features, releases and hotfixes.");
            open_dialog(app, ui, "Git Flow…", "gitflow.init");
        }
        Section::Hooks => {
            heading(ui, "Git Hooks");
            note(ui, "See and edit the scripts Git runs around commits and pushes.");
            open_dialog(app, ui, "Git Hooks…", "hooks.list");
        }
        Section::Lfs => {
            heading(ui, "Git LFS");
            note(ui, "Large files tracked outside the repository's history.");
            open_dialog(app, ui, "Git LFS…", "lfs.list");
        }
    }
}
