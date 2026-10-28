//! The left panel: local branches, remotes and their branches, stashes and tags, with a filter.
//! Double-click checks a branch out; right-click opens its menu.

use egui::{Frame, Margin, RichText, Sense, Stroke, vec2};

use gitcraft_engine::git::{Ref, RefKind};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{Dialog, GitApp};

const ROW_H: f32 = 22.0;

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("refs_panel")
        .default_size(250.0)
        .size_range(160.0..=520.0)
        .resizable(true)
        .frame(Frame::NONE.fill(t.panel).inner_margin(Margin::symmetric(6, 6)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            let mut filter = app.ref_filter.clone();
            ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Filter branches, tags, stashes").desired_width(f32::INFINITY));
            if filter != app.ref_filter {
                let _ = app.ui_command("ui.filterRefs", &json!({"text": filter}));
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| sections(app, ui));
        });
}

fn matches(app: &GitApp, name: &str) -> bool {
    app.ref_filter.is_empty() || name.to_lowercase().contains(&app.ref_filter.to_lowercase())
}

/// A collapsible section header with a count; remembers whether it is open.
fn header(ui: &mut egui::Ui, id: &str, icon: Icon, title: &str, count: usize, extra: impl FnOnce(&mut egui::Ui)) -> bool {
    let t = Tokens::get(ui.ctx());
    let key = egui::Id::new(("refs_section", id));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    let chev = egui::Rect::from_center_size(rect.left_center() + vec2(9.0, 0.0), vec2(12.0, 12.0));
    icons::paint(ui.painter(), chev, if open { Icon::ChevronDown } else { Icon::Chevron }, t.dim, t.dim);
    let ic = egui::Rect::from_center_size(rect.left_center() + vec2(26.0, 0.0), vec2(15.0, 15.0));
    icons::paint(ui.painter(), ic, icon, t.text, t.accent);
    ui.painter().text(rect.left_center() + vec2(40.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(12.0), t.text);
    ui.painter().text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, count.to_string(), egui::FontId::proportional(11.0), t.dim);
    if resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(key, open));
    }
    resp.context_menu(extra);
    open
}

/// One row: indent, icon, name, badges; returns the response.
fn row(ui: &mut egui::Ui, indent: f32, icon: Icon, name: &str, bold: bool, badge: Option<String>, color: Option<egui::Color32>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    if bold {
        ui.painter().rect_filled(rect, 4.0, t.selection);
    }
    let x = rect.left() + indent;
    if let Some(c) = color {
        ui.painter().rect_filled(egui::Rect::from_min_size(egui::pos2(x - 6.0, rect.top() + 5.0), vec2(3.0, ROW_H - 10.0)), 1.0, c);
    }
    icons::paint(ui.painter(), egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), vec2(14.0, 14.0)), icon, t.dim, t.accent);
    let font = if bold { egui::FontId::new(12.5, egui::FontFamily::Proportional) } else { egui::FontId::proportional(12.5) };
    let galley = ui.painter().layout_no_wrap(name.to_string(), font, if bold { t.accent } else { t.text });
    let mut right = rect.right() - 6.0;
    if let Some(b) = badge {
        let g = ui.painter().layout_no_wrap(b, egui::FontId::proportional(10.5), t.dim);
        right -= g.size().x;
        ui.painter().galley(egui::pos2(right, rect.center().y - g.size().y / 2.0), g, t.dim);
        right -= 6.0;
    }
    let clip = egui::Rect::from_min_max(egui::pos2(x + 20.0, rect.top()), egui::pos2(right, rect.bottom()));
    ui.painter().with_clip_rect(clip).galley(egui::pos2(x + 20.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp.on_hover_text(name)
}

fn track_badge(r: &Ref) -> Option<String> {
    if r.gone {
        return Some("gone".into());
    }
    match (r.ahead, r.behind) {
        (0, 0) => None,
        (a, 0) => Some(format!("↑{a}")),
        (0, b) => Some(format!("↓{b}")),
        (a, b) => Some(format!("↑{a} ↓{b}")),
    }
}

fn lane_of(app: &GitApp, commit: &str) -> Option<egui::Color32> {
    let i = *app.session.data.row_of.get(commit)?;
    let row = app.session.data.graph.rows.get(i)?;
    Some(crate::theme::lane_color(row.color))
}

fn sections(app: &mut GitApp, ui: &mut egui::Ui) {
    let d = &app.session.data;
    let locals: Vec<Ref> = d.local_branches().filter(|r| matches(app, &r.name)).cloned().collect();
    let remotes: Vec<String> = d.remotes.iter().map(|r| r.name.clone()).collect();
    let remote_refs: Vec<Ref> = d.remote_branches().filter(|r| matches(app, &r.name)).cloned().collect();
    let tags: Vec<Ref> = d.tags().filter(|r| matches(app, &r.name)).cloned().collect();
    let stashes: Vec<(usize, String)> = d.stashes.iter().filter(|s| matches(app, &s.message)).map(|s| (s.index, s.message.clone())).collect();

    if header(ui, "local", Icon::Laptop, "LOCAL", locals.len(), |ui| {
        if ui.button("Create Branch…").clicked() {
            app.dialog = Dialog::named("branch.create", &app.session, &json!({}));
            ui.close();
        }
    }) {
        let pinned = app.pinned_branches();
        let mut ordered: Vec<&Ref> = locals.iter().filter(|r| pinned.contains(&r.name)).collect();
        ordered.extend(locals.iter().filter(|r| !pinned.contains(&r.name)));
        for r in ordered {
            let color = lane_of(app, &r.commit);
            let is_pinned = pinned.contains(&r.name);
            let badge = match (track_badge(r), is_pinned) {
                (Some(b), true) => Some(format!("pinned  {b}")),
                (None, true) => Some("pinned".to_string()),
                (b, false) => b,
            };
            let resp = row(ui, 22.0, Icon::Branch, &r.name, r.head, badge, color);
            if app.picked_branches.contains(&r.name) {
                let t = Tokens::get(ui.ctx());
                ui.painter().rect_stroke(resp.rect.shrink(1.0), 4.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            }
            resp.dnd_set_drag_payload(crate::graph_view::DragRef { name: r.name.clone() });
            if resp.clicked() {
                if ui.input(|i| i.modifiers.command) {
                    // Mod-click picks several branches (to delete them together).
                    if !app.picked_branches.remove(&r.name) {
                        app.picked_branches.insert(r.name.clone());
                    }
                } else {
                    app.picked_branches.clear();
                    let _ = app.run("graph.select", json!({"oid": r.commit}));
                }
            }
            if resp.double_clicked() && !r.head {
                let _ = app.run("branch.checkout", json!({"name": r.name}));
            }
            if app.picked_branches.len() > 1 && app.picked_branches.contains(&r.name) {
                resp.context_menu(|ui| picked_menu(app, ui));
            } else {
                resp.context_menu(|ui| branch_menu(app, ui, r));
            }
        }
    }
    if header(ui, "remote", Icon::Cloud, "REMOTE", remotes.len(), |ui| {
        if ui.button("Add Remote…").clicked() {
            app.dialog = Dialog::named("remote.add", &app.session, &json!({}));
            ui.close();
        }
    }) {
        for remote in &remotes {
            let mine: Vec<&Ref> = remote_refs.iter().filter(|r| r.remote() == Some(remote.as_str())).collect();
            let resp = row(ui, 22.0, Icon::Cloud, remote, false, Some(mine.len().to_string()), None);
            // Drop a local branch on a remote to push it there.
            if let Some(dr) = egui::DragAndDrop::payload::<crate::graph_view::DragRef>(ui.ctx())
                && app.session.data.local_branches().any(|b| b.name == dr.name)
                && resp.contains_pointer()
            {
                let t = Tokens::get(ui.ctx());
                ui.painter().rect_stroke(resp.rect.shrink(1.0), 4.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                resp.clone().on_hover_text(format!("Push {} to {remote}", dr.name));
            }
            if let Some(dr) = resp.dnd_release_payload::<crate::graph_view::DragRef>()
                && app.session.data.local_branches().any(|b| b.name == dr.name)
            {
                let _ = app.run("remote.push", json!({"branch": dr.name, "remote": remote}));
            }
            resp.context_menu(|ui| {
                if ui.button(format!("Fetch {remote}")).clicked() {
                    let _ = app.run("remote.fetch", json!({"remote": remote}));
                    ui.close();
                }
                if ui.button("Prune Deleted Branches").clicked() {
                    let _ = app.run("remote.prune", json!({"name": remote}));
                    ui.close();
                }
                if ui.button("Edit URL…").clicked() {
                    let url = app.session.data.remotes.iter().find(|x| x.name == *remote).map(|x| x.fetch_url.clone()).unwrap_or_default();
                    app.dialog = Some(Dialog::EditRemote { name: remote.clone(), new_name: remote.clone(), url });
                    ui.close();
                }
                if ui.button("Copy URL").clicked() {
                    let url = app.session.data.remotes.iter().find(|x| x.name == *remote).map(|x| x.fetch_url.clone()).unwrap_or_default();
                    app.copy(url);
                    ui.close();
                }
                if ui.button(format!("Remove {remote}")).clicked() {
                    app.dialog = Some(Dialog::confirm(
                        &format!("Remove remote {remote}?"),
                        "Its remote-tracking branches are removed too. The repository on the server is not touched.",
                        "remote.remove",
                        json!({"name": remote}),
                        "Remove",
                    ));
                    ui.close();
                }
            });
            for r in mine {
                let short = r.remote_branch().unwrap_or(&r.name);
                let resp = row(ui, 40.0, Icon::Branch, short, false, None, lane_of(app, &r.commit));
                if resp.clicked() {
                    let _ = app.run("graph.select", json!({"oid": r.commit}));
                }
                if resp.double_clicked() {
                    let _ = app.run("branch.pullRemote", json!({"name": r.name}));
                }
                resp.context_menu(|ui| branch_menu(app, ui, r));
            }
        }
    }
    if header(ui, "stashes", Icon::Stash, "STASHES", stashes.len(), |_| {}) {
        for (i, msg) in &stashes {
            let resp = row(ui, 22.0, Icon::Stash, msg, false, Some(format!("@{{{i}}}")), None);
            if resp.double_clicked() {
                let _ = app.run("stash.apply", json!({"index": i}));
            }
            resp.context_menu(|ui| {
                if ui.button("Apply Stash").clicked() {
                    let _ = app.run("stash.apply", json!({"index": i}));
                    ui.close();
                }
                if ui.button("Pop Stash").clicked() {
                    let _ = app.run("stash.pop", json!({"index": i}));
                    ui.close();
                }
                if ui.button("View Changes").clicked() {
                    let r = format!("stash@{{{i}}}");
                    let _ = app.run("diff.compare", json!({"from": format!("{r}^1"), "to": r}));
                    ui.close();
                }
                if ui.button("Rename…").clicked() {
                    let current = msg.split_once(": ").map(|(_, m)| m.to_string()).unwrap_or_else(|| msg.clone());
                    app.dialog = Some(Dialog::RenameStash { index: *i, message: current });
                    ui.close();
                }
                if ui.button("Delete Stash").clicked() {
                    app.dialog = Some(Dialog::confirm("Delete stash?", msg, "stash.drop", json!({"index": i}), "Delete"));
                    ui.close();
                }
            });
        }
    }
    if header(ui, "tags", Icon::Tag, "TAGS", tags.len(), |_| {}) {
        for r in &tags {
            let resp = row(ui, 22.0, Icon::Tag, &r.name, false, None, lane_of(app, &r.commit));
            if resp.clicked() {
                let _ = app.run("graph.select", json!({"oid": r.commit}));
            }
            resp.context_menu(|ui| {
                if ui.add_enabled(app.disabled("tag.push").is_none(), egui::Button::new("Push Tag")).clicked() {
                    let _ = app.run("tag.push", json!({"name": r.name}));
                    ui.close();
                }
                if ui.button("Copy Tag Name").clicked() {
                    app.copy(r.name.clone());
                    ui.close();
                }
                if ui.button("Delete Tag").clicked() {
                    let _ = app.run("tag.delete", json!({"name": r.name}));
                    ui.close();
                }
                if ui.add_enabled(app.disabled("tag.deleteRemote").is_none(), egui::Button::new("Delete Tag from Remote…")).clicked() {
                    app.dialog = Some(Dialog::confirm(
                        &format!("Delete tag {} on the remote?", r.name),
                        "This removes the tag from the server for everyone. The local tag stays.",
                        "tag.deleteRemote",
                        json!({"name": r.name}),
                        "Delete",
                    ));
                    ui.close();
                }
            });
        }
    }
    submodules_and_worktrees(app, ui);
    crate::hosting_ui::sections(app, ui);
    let _ = RichText::new("");
}

fn submodules_and_worktrees(app: &mut GitApp, ui: &mut egui::Ui) {
    let subs: Vec<gitcraft_engine::git::Submodule> = app.session.data.submodules.iter().filter(|s| matches(app, &s.path)).cloned().collect();
    let root = app.session.repo.as_ref().map(|r| r.root().to_path_buf()).unwrap_or_default();
    if header(ui, "submodules", Icon::Folder, "SUBMODULES", subs.len(), |ui| {
        if ui.button("Add Submodule…").clicked() {
            app.dialog = Some(Dialog::AddSubmodule { url: String::new(), path: String::new() });
            ui.close();
        }
        if ui.add_enabled(app.disabled("submodule.update").is_none(), egui::Button::new("Update All")).clicked() {
            let _ = app.run("submodule.update", json!({}));
            ui.close();
        }
    }) {
        use gitcraft_engine::git::SubmoduleState as St;
        for sm in &subs {
            let badge = match sm.state {
                St::Clean => None,
                St::Uninitialized => Some("not cloned".to_string()),
                St::Modified => Some("changed".to_string()),
                St::Conflict => Some("conflict".to_string()),
            };
            let resp = row(ui, 22.0, Icon::Folder, &sm.path, false, badge, None);
            let resp = resp.on_hover_text(sm.url.clone().unwrap_or_default());
            if resp.double_clicked() && sm.state != St::Uninitialized {
                let _ = app.open_repo(&root.join(&sm.path));
            }
            resp.context_menu(|ui| {
                if ui.add_enabled(sm.state != St::Uninitialized, egui::Button::new("Open in a Tab")).clicked() {
                    let _ = app.open_repo(&root.join(&sm.path));
                    ui.close();
                }
                let label = if sm.state == St::Uninitialized { "Clone (Initialize)" } else { "Update" };
                if ui.button(label).clicked() {
                    let _ = app.run("submodule.update", json!({"path": sm.path}));
                    ui.close();
                }
                if ui.button("Copy URL").clicked() {
                    app.copy(sm.url.clone().unwrap_or_default());
                    ui.close();
                }
                if ui.button("Sync URLs").clicked() {
                    let _ = app.run("submodule.sync", json!({}));
                    ui.close();
                }
                if ui.button("Remove Submodule…").clicked() {
                    app.dialog = Some(Dialog::confirm(
                        &format!("Remove the submodule {}?", sm.path),
                        "Its folder is deleted and its entry removed from .gitmodules; commit to finish.",
                        "submodule.remove",
                        json!({"path": sm.path}),
                        "Remove",
                    ));
                    ui.close();
                }
            });
        }
    }
    let wts: Vec<gitcraft_engine::git::Worktree> = app.session.data.worktrees.iter().filter(|w| matches(app, &w.path)).cloned().collect();
    if header(ui, "worktrees", Icon::Laptop, "WORKTREES", wts.len(), |ui| {
        if ui.add_enabled(app.disabled("worktree.add").is_none(), egui::Button::new("Add Worktree…")).clicked() {
            app.dialog = Some(Dialog::AddWorktree { path: String::new(), branch: String::new(), new_branch: true });
            ui.close();
        }
        if ui.button("Prune Missing").clicked() {
            let _ = app.run("worktree.prune", json!({}));
            ui.close();
        }
    }) {
        for w in &wts {
            let here = root.as_path() == std::path::Path::new(&w.path);
            let name = w.branch.clone().unwrap_or_else(|| format!("detached {}", w.head.as_deref().and_then(|h| h.get(..7)).unwrap_or("")));
            let badge = if w.main {
                Some("main".to_string())
            } else if w.prunable {
                Some("missing".to_string())
            } else if w.locked {
                Some("locked".to_string())
            } else {
                None
            };
            let resp = row(ui, 22.0, Icon::Laptop, &name, here, badge, None).on_hover_text(&w.path);
            if resp.double_clicked() && !here {
                let _ = app.open_repo(std::path::Path::new(&w.path));
            }
            resp.context_menu(|ui| {
                if ui.add_enabled(!here && !w.prunable, egui::Button::new("Open in a Tab")).clicked() {
                    let _ = app.open_repo(std::path::Path::new(&w.path));
                    ui.close();
                }
                if ui.button("Copy Path").clicked() {
                    app.copy(w.path.clone());
                    ui.close();
                }
                if !w.main && !here && ui.button("Remove Worktree…").clicked() {
                    app.dialog = Some(Dialog::confirm(
                        &format!("Remove the work tree at {}?", w.path),
                        "Its folder is deleted (refused when it has uncommitted changes). The branch stays.",
                        "worktree.remove",
                        json!({"path": w.path}),
                        "Remove",
                    ));
                    ui.close();
                }
            });
        }
    }
}

/// The menu of a branch (local or remote), also used by the graph's labels.
pub fn branch_menu(app: &mut GitApp, ui: &mut egui::Ui, r: &Ref) {
    let current = app.session.data.status.branch.clone().unwrap_or_else(|| "HEAD".into());
    let local = r.kind == RefKind::Local;
    if !r.head && ui.button(format!("Checkout {}", r.name)).clicked() {
        let _ = app.run("branch.checkout", json!({"name": r.name}));
        ui.close();
    }
    if !r.head {
        if ui.button(format!("Merge {} into {current}", r.name)).clicked() {
            let _ = app.run("merge.into", json!({"ref": r.name}));
            ui.close();
        }
        if ui.button(format!("Rebase {current} onto {}", r.name)).clicked() {
            app.dialog = Some(Dialog::confirm(
                &format!("Rebase {current} onto {}?", r.name),
                "Your commits are replayed on top of it with new ids. Avoid rebasing commits others already have.",
                "rebase.onto",
                json!({"ref": r.name}),
                "Rebase",
            ));
            ui.close();
        }
        if ui.button(format!("Fast-forward {current} to {}", r.name)).clicked() {
            let _ = app.run("merge.fastForward", json!({"ref": r.name}));
            ui.close();
        }
    }
    ui.separator();
    if ui.button("Create Branch Here…").clicked() {
        app.dialog = Dialog::named("branch.create", &app.session, &json!({"from": r.name}));
        ui.close();
    }
    if local {
        if r.head && ui.add_enabled(app.disabled("remote.push").is_none(), egui::Button::new("Push")).clicked() {
            let _ = app.run("remote.push", json!({}));
            ui.close();
        }
        if !r.head && ui.add_enabled(app.disabled("remote.push").is_none(), egui::Button::new(format!("Push {}", r.name))).clicked() {
            let _ = app.run("remote.push", json!({"branch": r.name}));
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            app.dialog = Dialog::named("branch.rename", &app.session, &json!({"from": r.name}));
            ui.close();
        }
        if crate::hosting_ui::forge_kind(app).is_some() && ui.button("Create Pull Request…").clicked() {
            app.dialog = Dialog::named("pr.create", &app.session, &json!({"head": r.name}));
            ui.close();
        }
        if !r.head && ui.button(format!("Delete {}", r.name)).clicked() {
            app.dialog = Some(Dialog::confirm(
                &format!("Delete branch {}?", r.name),
                "Commits only on this branch stay reachable through Undo until you close GitCraft.",
                "branch.delete",
                json!({"name": r.name, "force": true}),
                "Delete",
            ));
            ui.close();
        }
    } else if ui.button(format!("Delete {} from the remote", r.name)).clicked() {
        app.dialog = Some(Dialog::confirm(
            &format!("Delete {} on the remote?", r.name),
            "This removes the branch from the server for everyone.",
            "branch.deleteRemote",
            json!({"name": r.name}),
            "Delete",
        ));
        ui.close();
    }
    ui.separator();
    let soloed = app.session.view.solo.contains(&r.full);
    if ui
        .button(if soloed { format!("Stop Soloing {}", r.name) } else { format!("Solo {}", r.name) })
        .on_hover_text("Show only this branch's history")
        .clicked()
    {
        let _ = app.run("graph.solo", json!({"name": r.full}));
        ui.close();
    }
    let hidden = app.session.view.hidden.contains(&r.full);
    if !r.head && ui.button(if hidden { format!("Show {}", r.name) } else { format!("Hide {}", r.name) }).clicked() {
        let _ = app.run("graph.hide", json!({"name": r.full}));
        ui.close();
    }
    if ui.button(format!("Compare {} with {current}", r.name)).clicked() {
        let _ = app.run("diff.compare", json!({"from": "HEAD", "to": r.name}));
        ui.close();
    }
    if local {
        let pinned = app.pinned_branches().contains(&r.name);
        if ui.button(if pinned { "Unpin" } else { "Pin to Top" }).clicked() {
            app.toggle_pin(&r.name);
            ui.close();
        }
    }
    if ui.button("Copy Branch Name").clicked() {
        app.copy(r.name.clone());
        ui.close();
    }
}

/// The menu for several branches picked with Mod-click.
fn picked_menu(app: &mut GitApp, ui: &mut egui::Ui) {
    let names: Vec<String> = app.picked_branches.iter().cloned().collect();
    ui.label(RichText::new(format!("{} branches", names.len())).small().weak());
    let current = app.session.data.status.branch.clone();
    let deletable: Vec<String> = names.iter().filter(|n| Some(*n) != current.as_ref()).cloned().collect();
    if ui.add_enabled(!deletable.is_empty(), egui::Button::new(format!("Delete {} Branches…", deletable.len()))).clicked() {
        app.dialog = Some(Dialog::confirm(
            &format!("Delete {} branches?", deletable.len()),
            &format!("{}\n\nUndo brings them back until you close GitCraft.", deletable.join(", ")),
            "branch.delete",
            json!({"names": deletable, "force": true}),
            "Delete",
        ));
        app.picked_branches.clear();
        ui.close();
    }
    if ui.button("Clear Selection").clicked() {
        app.picked_branches.clear();
        ui.close();
    }
}
