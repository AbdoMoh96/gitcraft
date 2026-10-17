//! The right panel. On the work-in-progress row: unstaged and staged files, conflicts, and the
//! commit message. On a commit: its message, author, parents and changed files.

use egui::{Color32, FontId, Frame, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};
use gitcraft_engine::git::{Change, FileStatus, RepoState};
use gitcraft_engine::{Area, Selected};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{Dialog, GitApp, Reword, util};

/// Summary length the counter counts down from.
const SUMMARY_LIMIT: i64 = 72;

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("commit_panel")
        .default_size(380.0)
        .size_range(280.0..=720.0)
        .resizable(true)
        .frame(Frame::NONE.fill(t.panel).inner_margin(Margin::same(10)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| match app.session.sel.row.clone() {
            Selected::Commit(oid) => details(app, ui, &oid),
            _ => wip(app, ui),
        });
}

pub fn change_color(c: Change, t: Tokens) -> Color32 {
    match c {
        Change::Added | Change::Untracked | Change::Copied => t.added,
        Change::Deleted => t.removed,
        Change::Unmerged => t.warning,
        _ => t.modified,
    }
}

/// One file row: change badge, file name and folder; hover shows `buttons`. Returns (clicked, the
/// index of a hover button that was clicked, response).
fn file_row(
    ui: &mut egui::Ui,
    path: &str,
    change: Change,
    selected: bool,
    conflict: bool,
    buttons: &[(Icon, &str)],
) -> (bool, Option<usize>, egui::Response) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 4.0, t.selection);
    } else if resp.hovered() {
        p.rect_filled(rect, 4.0, t.hover);
    }
    let badge = Rect::from_center_size(pos2(rect.left() + 11.0, rect.center().y), vec2(16.0, 16.0));
    let col = if conflict { t.warning } else { change_color(change, t) };
    p.rect_filled(badge, 3.0, col.gamma_multiply(0.18));
    p.text(badge.center(), egui::Align2::CENTER_CENTER, if conflict { "!" } else { change.letter() }, FontId::monospace(11.0), col);
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (format!("{d}/"), n.to_string()),
        None => (String::new(), path.to_string()),
    };
    let hovered = ui.rect_contains_pointer(rect);
    let right = rect.right() - if hovered { 26.0 * buttons.len() as f32 } else { 4.0 };
    let clip = Rect::from_min_max(pos2(rect.left() + 24.0, rect.top()), pos2(right, rect.bottom()));
    let g = p.layout_no_wrap(name, FontId::proportional(12.5), t.text);
    let gw = g.size().x;
    p.with_clip_rect(clip).galley(pos2(clip.left(), rect.center().y - g.size().y / 2.0), g, t.text);
    if !dir.is_empty() {
        let d = p.layout_no_wrap(dir, FontId::proportional(11.5), t.dim);
        p.with_clip_rect(clip).galley(pos2(clip.left() + gw + 6.0, rect.center().y - d.size().y / 2.0), d, t.dim);
    }
    let mut hit = None;
    if hovered {
        for (i, (icon, tip)) in buttons.iter().enumerate() {
            let b = Rect::from_center_size(pos2(rect.right() - 13.0 - 26.0 * (buttons.len() - 1 - i) as f32, rect.center().y), vec2(22.0, 20.0));
            let r = ui.interact(b, ui.id().with((path, i, "filebtn")), Sense::click()).on_hover_text(*tip);
            if r.hovered() {
                ui.painter().rect_filled(b, 4.0, t.chrome);
            }
            icons::paint(ui.painter(), b.shrink(3.0), *icon, t.text, t.accent);
            if r.clicked() {
                hit = Some(i);
            }
        }
    }
    let clicked = resp.clicked() && hit.is_none();
    (clicked, hit, resp.on_hover_text(path))
}

fn section(ui: &mut egui::Ui, title: &str, count: usize, button: Option<(&str, Option<&'static str>)>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{title} ({count})")).strong().size(12.5));
        if let Some((label, why)) = button {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = ui.add_enabled(why.is_none(), egui::Button::new(RichText::new(label).small()));
                clicked = b.clicked();
            });
        }
    });
    clicked
}

fn wip(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let st = app.session.data.status.clone();
    let mut run: Option<(&'static str, Value)> = None;
    let mut dialog: Option<Dialog> = None;

    let total = st.files.len();
    ui.horizontal(|ui| {
        let branch = st.branch.clone().unwrap_or_else(|| "detached HEAD".into());
        ui.label(RichText::new(format!("{total} file change{} on {branch}", if total == 1 { "" } else { "s" })).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Trash, "Discard all changes", app.disabled("discard.all"), 16.0).clicked() {
                dialog = Some(Dialog::confirm(
                    "Discard all changes?",
                    "Every staged and unstaged change is thrown away and untracked files are deleted. This can't be undone.",
                    "discard.all",
                    json!({}),
                    "Discard All",
                ));
            }
        });
    });
    if st.state != RepoState::Clean {
        let conflicts = st.conflicted().count();
        Frame::NONE.fill(t.warning_bg).corner_radius(6).inner_margin(8).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Warning, t.warning, t.warning);
                let what = if conflicts > 0 {
                    format!("{}: {conflicts} conflicted file{}", st.state.label(), if conflicts == 1 { "" } else { "s" })
                } else {
                    format!("{}: ready to continue", st.state.label())
                };
                ui.label(RichText::new(what).color(t.warning).strong());
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(app.disabled("op.continue").is_none() && conflicts == 0, egui::Button::new("Continue")).clicked() {
                    run = Some(("op.continue", json!({})));
                }
                if st.state == RepoState::Rebasing && ui.button("Skip").clicked() {
                    run = Some(("rebase.skip", json!({})));
                }
                if ui.button("Abort").clicked() {
                    run = Some(("op.abort", json!({})));
                }
            });
        });
    }
    ui.add_space(4.0);
    let avail = ui.available_height();
    let list_h = ((avail - 250.0) / 2.0).max(70.0);
    let sel_file = app.session.sel.file.clone();

    let unstaged: Vec<FileStatus> = st.unstaged().cloned().collect();
    if section(ui, "Unstaged Files", unstaged.len(), Some(("Stage all changes", app.disabled("stage.all")))) {
        run = Some(("stage.all", json!({})));
    }
    Frame::NONE.fill(t.bg).corner_radius(4).inner_margin(2).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("unstaged").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
            if unstaged.is_empty() {
                ui.label(RichText::new("No unstaged changes").small().weak());
            }
            for f in &unstaged {
                let conflict = f.conflict.is_some();
                let selected = sel_file.as_ref().is_some_and(|s| s.path == f.path && s.area == Area::Unstaged);
                let buttons: &[(Icon, &str)] =
                    if conflict { &[(Icon::Check, "Mark resolved")] } else { &[(Icon::Trash, "Discard changes"), (Icon::Plus, "Stage file")] };
                let (clicked, hit, resp) = file_row(ui, &f.path, f.worktree, selected, conflict, buttons);
                if clicked {
                    run = Some(("file.select", json!({"path": f.path, "area": "unstaged"})));
                }
                match (conflict, hit) {
                    (true, Some(_)) => run = Some(("conflict.markResolved", json!({"path": f.path}))),
                    (false, Some(0)) => {
                        dialog = Some(Dialog::confirm(
                            &format!("Discard changes to {}?", f.path),
                            if f.is_untracked() {
                                "The file is deleted. This can't be undone."
                            } else {
                                "Its unstaged changes are thrown away. This can't be undone."
                            },
                            "discard.file",
                            json!({"path": f.path}),
                            "Discard",
                        ))
                    }
                    (false, Some(_)) => run = Some(("stage.file", json!({"path": f.path}))),
                    _ => {}
                }
                resp.context_menu(|ui| {
                    if conflict {
                        if ui.button("Take Current (Ours)").clicked() {
                            run = Some(("conflict.takeOurs", json!({"path": f.path})));
                            ui.close();
                        }
                        if ui.button("Take Incoming (Theirs)").clicked() {
                            run = Some(("conflict.takeTheirs", json!({"path": f.path})));
                            ui.close();
                        }
                        if ui.button("Mark Resolved").clicked() {
                            run = Some(("conflict.markResolved", json!({"path": f.path})));
                            ui.close();
                        }
                    } else {
                        if ui.button("Stage File").clicked() {
                            run = Some(("stage.file", json!({"path": f.path})));
                            ui.close();
                        }
                        if ui.button("Discard Changes…").clicked() {
                            dialog = Some(Dialog::confirm(
                                &format!("Discard changes to {}?", f.path),
                                "This can't be undone.",
                                "discard.file",
                                json!({"path": f.path}),
                                "Discard",
                            ));
                            ui.close();
                        }
                    }
                    if ui.button("Copy Path").clicked() {
                        app.copy(f.path.clone());
                        ui.close();
                    }
                    if !f.is_untracked() && ui.button("File History").clicked() {
                        run = Some(("view.history", json!({"path": f.path})));
                        ui.close();
                    }
                });
            }
        });
    });
    ui.add_space(6.0);
    let staged: Vec<FileStatus> = st.staged().cloned().collect();
    if section(ui, "Staged Files", staged.len(), Some(("Unstage all changes", app.disabled("unstage.all")))) {
        run = Some(("unstage.all", json!({})));
    }
    Frame::NONE.fill(t.bg).corner_radius(4).inner_margin(2).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("staged").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
            if staged.is_empty() {
                ui.label(RichText::new("Stage files to commit them").small().weak());
            }
            for f in &staged {
                let selected = sel_file.as_ref().is_some_and(|s| s.path == f.path && s.area == Area::Staged);
                let (clicked, hit, resp) = file_row(ui, &f.path, f.index, selected, false, &[(Icon::Minus, "Unstage file")]);
                if clicked {
                    run = Some(("file.select", json!({"path": f.path, "area": "staged"})));
                }
                if hit.is_some() {
                    run = Some(("unstage.file", json!({"path": f.path})));
                }
                resp.context_menu(|ui| {
                    if ui.button("Unstage File").clicked() {
                        run = Some(("unstage.file", json!({"path": f.path})));
                        ui.close();
                    }
                    if ui.button("Copy Path").clicked() {
                        app.copy(f.path.clone());
                        ui.close();
                    }
                });
            }
        });
    });
    ui.add_space(8.0);
    commit_form(app, ui, staged.len(), &mut run);
    if let Some(d) = dialog {
        app.dialog = Some(d);
    }
    if let Some((id, p)) = run {
        let _ = app.run(id, p);
    }
}

fn commit_form(app: &mut GitApp, ui: &mut egui::Ui, staged: usize, run: &mut Option<(&'static str, Value)>) {
    let t = Tokens::get(ui.ctx());
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Commit").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut amend = app.session.draft.amend;
            if ui.add_enabled(app.disabled("commit.amend").is_none(), egui::Checkbox::new(&mut amend, "Amend")).changed() {
                *run = Some(("commit.amend", json!({"value": amend})));
            }
        });
    });
    let mut summary = app.session.draft.summary.clone();
    let left = SUMMARY_LIMIT - summary.chars().count() as i64;
    let r = ui.add(egui::TextEdit::singleline(&mut summary).hint_text("Summary").desired_width(f32::INFINITY).font(FontId::proportional(13.5)));
    let rect = r.rect;
    ui.painter().text(
        pos2(rect.right() - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        left.to_string(),
        FontId::proportional(10.5),
        if left < 0 { t.danger } else { t.dim },
    );
    let mut desc = app.session.draft.description.clone();
    ui.add(egui::TextEdit::multiline(&mut desc).hint_text("Description").desired_rows(4).desired_width(f32::INFINITY));
    if summary != app.session.draft.summary || desc != app.session.draft.description {
        app.session.draft.summary = summary;
        app.session.draft.description = desc;
    }
    let merging = app.session.data.status.state == RepoState::Merging;
    let can = (staged > 0 || app.session.draft.amend || merging) && (!app.session.draft.summary.trim().is_empty() || merging);
    let label = if app.session.draft.amend {
        "Amend Previous Commit".to_string()
    } else if staged == 0 && !merging {
        "Stage files to commit".to_string()
    } else if merging && staged == 0 {
        "Commit Merge".to_string()
    } else {
        format!("Commit changes to {staged} file{}", if staged == 1 { "" } else { "s" })
    };
    let b = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
        .fill(if can { crate::theme::APP_COLOR } else { t.dim })
        .min_size(vec2(ui.available_width(), 32.0));
    if ui.add_enabled(can, b).on_hover_text(util::shortcut_label("Mod+Enter")).clicked() {
        *run = Some(("commit.create", json!({})));
    }
}

fn details(app: &mut GitApp, ui: &mut egui::Ui, oid: &str) {
    let t = Tokens::get(ui.ctx());
    let key = (oid.to_string(), app.session.generation());
    if app.details.as_ref().map(|(k, _)| k) != Some(&key) {
        let r = app.session.run("commit.show", &json!({"oid": oid})).map_err(|e| e.to_string());
        app.details = Some((key, r));
    }
    let Some((_, Ok(v))) = app.details.clone() else {
        if let Some((_, Err(e))) = &app.details {
            ui.label(RichText::new(e).color(t.danger));
        }
        return;
    };
    let c = &v["commit"];
    let s = |x: &Value| x.as_str().unwrap_or("").to_string();
    let mut run: Option<(&'static str, Value)> = None;
    egui::ScrollArea::vertical().id_salt("details").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("commit").small().weak());
            if ui.link(RichText::new(s(&c["short"])).monospace()).on_hover_text("Copy the full id").clicked() {
                app.copy(s(&c["oid"]));
            }
            let parents: Vec<String> = c["parents"].as_array().map(|a| a.iter().map(&s).collect()).unwrap_or_default();
            if !parents.is_empty() {
                ui.label(RichText::new(if parents.len() > 1 { "parents" } else { "parent" }).small().weak());
                for p in parents {
                    if ui.link(RichText::new(p.get(..7).unwrap_or(&p)).monospace()).clicked() {
                        run = Some(("graph.select", json!({"oid": p})));
                    }
                }
            }
        });
        ui.add_space(4.0);
        let full = s(&c["oid"]);
        if app.reword.as_ref().is_some_and(|r| r.oid == full) {
            reword_form(app, ui, &mut run);
        } else {
            message_box(app, ui, &full, &s(&c["summary"]), &s(&c["body"]));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let name = s(&c["author"]["name"]);
            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
            app.avatars.paint_or_initials(ui.painter(), r.center(), 14.0, &s(&c["author"]["email"]), &name, Color32::WHITE);
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{name} <{}>", s(&c["author"]["email"]))).strong());
                let time = c["author"]["time"].as_i64().unwrap_or(0);
                ui.label(RichText::new(format!("authored {}", util::date_time(time))).small().weak());
                let committer = s(&c["committer"]["name"]);
                if committer != name {
                    ui.label(RichText::new(format!("committed by {committer}")).small().weak());
                }
            });
        });
        if let Some(refs) = v["refs"].as_array().filter(|r| !r.is_empty()) {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for r in refs {
                    ui.label(RichText::new(r.as_str().unwrap_or("")).small().color(t.accent));
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (id, label) in [("commit.checkout", "Checkout"), ("cherryPick.commit", "Cherry Pick"), ("revert.commit", "Revert")] {
                if ui.add_enabled(app.disabled(id).is_none(), egui::Button::new(RichText::new(label).small())).clicked() {
                    run = Some((id, json!({"oid": s(&c["oid"])})));
                }
            }
            if ui.button(RichText::new("Branch here…").small()).clicked() {
                app.dialog = Some(Dialog::CreateBranch { name: String::new(), from: Some(s(&c["oid"])), checkout: true });
            }
            if ui.button(RichText::new("Tag here…").small()).clicked() {
                app.dialog = Some(Dialog::CreateTag { name: String::new(), oid: Some(s(&c["oid"])), message: String::new() });
            }
        });
        ui.add_space(8.0);
        ui.separator();
        let files = v["files"].as_array().cloned().unwrap_or_default();
        ui.label(RichText::new(format!("{} changed file{}", files.len(), if files.len() == 1 { "" } else { "s" })).strong());
        let sel = app.session.sel.file.clone();
        for f in files {
            let path = s(&f["path"]);
            let change = match f["change"].as_str().unwrap_or("") {
                "added" => Change::Added,
                "deleted" => Change::Deleted,
                "renamed" => Change::Renamed,
                "copied" => Change::Copied,
                "typeChanged" => Change::TypeChanged,
                _ => Change::Modified,
            };
            let selected = sel.as_ref().is_some_and(|x| x.path == path);
            let (clicked, _, resp) = file_row(ui, &path, change, selected, false, &[]);
            if clicked {
                run = Some(("file.select", json!({"path": path, "area": "commit", "oid": s(&c["oid"])})));
            }
            resp.context_menu(|ui| {
                if ui.button("File History").clicked() {
                    run = Some(("view.history", json!({"path": path})));
                    ui.close();
                }
                if ui.button("Copy Path").clicked() {
                    app.copy(path.clone());
                    ui.close();
                }
            });
        }
    });
    if let Some((id, p)) = run
        && app.run(id, p).is_ok()
        && id == "commit.reword"
    {
        app.reword = None;
    }
}

/// The commit's message; a click edits it.
fn message_box(app: &mut GitApp, ui: &mut egui::Ui, oid: &str, summary: &str, body: &str) {
    let t = Tokens::get(ui.ctx());
    let why = app.disabled("commit.reword");
    let out = Frame::NONE.fill(t.bg).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(egui::Label::new(RichText::new(summary).size(15.0).strong()).wrap());
        if !body.trim().is_empty() {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(body).size(12.5)).wrap());
        }
    });
    let rect = out.response.rect;
    let resp = ui.interact(rect, ui.id().with(("message", oid)), Sense::click());
    if resp.hovered() && why.is_none() {
        ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        let pen = Rect::from_min_size(pos2(rect.right() - 22.0, rect.top() + 6.0), vec2(16.0, 16.0));
        icons::paint(ui.painter(), pen, Icon::Pencil, t.dim, t.accent);
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let resp = resp.on_hover_text(why.unwrap_or("Click to edit the commit message"));
    if resp.clicked() && why.is_none() {
        app.reword = Some(Reword { oid: oid.to_string(), summary: summary.to_string(), description: body.trim().to_string(), focus: true });
    }
    resp.context_menu(|ui| {
        if ui.add_enabled(why.is_none(), egui::Button::new("Edit Commit Message")).clicked() {
            app.reword = Some(Reword { oid: oid.to_string(), summary: summary.to_string(), description: body.trim().to_string(), focus: true });
            ui.close();
        }
        if ui.button("Copy Commit Message").clicked() {
            let msg = if body.trim().is_empty() { summary.to_string() } else { format!("{summary}\n\n{}", body.trim_end()) };
            app.copy(msg);
            ui.close();
        }
    });
}

/// The message editor for an existing commit: summary, description, update and cancel.
fn reword_form(app: &mut GitApp, ui: &mut egui::Ui, run: &mut Option<(&'static str, Value)>) {
    let t = Tokens::get(ui.ctx());
    let Some(mut r) = app.reword.clone() else { return };
    let mut submit = false;
    let mut cancel = false;
    Frame::NONE.fill(t.bg).corner_radius(6).inner_margin(Margin::same(8)).stroke(Stroke::new(1.0, t.accent)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let left = SUMMARY_LIMIT - r.summary.chars().count() as i64;
        let s = ui.add(egui::TextEdit::singleline(&mut r.summary).hint_text("Summary").desired_width(f32::INFINITY).font(FontId::proportional(14.0)));
        ui.painter().text(
            pos2(s.rect.right() - 6.0, s.rect.center().y),
            egui::Align2::RIGHT_CENTER,
            left.to_string(),
            FontId::proportional(10.5),
            if left < 0 { t.danger } else { t.dim },
        );
        if r.focus {
            s.request_focus();
            r.focus = false;
        }
        let d = ui.add(egui::TextEdit::multiline(&mut r.description).hint_text("Description").desired_rows(4).desired_width(f32::INFINITY));
        if s.has_focus() || d.has_focus() {
            let (m, k) = util::parse_shortcut("Mod+Enter").unwrap_or((egui::Modifiers::COMMAND, egui::Key::Enter));
            submit |= ui.input_mut(|i| i.consume_key(m, k));
            cancel |= ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
    });
    ui.add_space(6.0);
    let can = !r.summary.trim().is_empty() && app.disabled("commit.reword").is_none();
    ui.horizontal(|ui| {
        let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(60.0);
        let update = egui::Button::new(RichText::new("Update Message").color(Color32::WHITE).strong())
            .fill(if can { crate::theme::APP_COLOR } else { t.dim })
            .min_size(vec2(w, 30.0));
        if ui.add_enabled(can, update).on_hover_text(util::shortcut_label("Mod+Enter")).clicked() {
            submit = true;
        }
        let stop = egui::Button::new(RichText::new("Cancel").color(t.danger)).fill(t.danger.gamma_multiply(0.15)).min_size(vec2(w, 30.0));
        if ui.add(stop).on_hover_text("Esc").clicked() {
            cancel = true;
        }
    });
    if cancel {
        app.reword = None;
        return;
    }
    if submit && can {
        *run = Some(("commit.reword", json!({"oid": r.oid, "summary": r.summary, "description": r.description})));
    }
    app.reword = Some(r);
}
