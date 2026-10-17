//! The command palette (Mod+P): every command, branch to check out and recent repository, found by
//! typing a few letters of it.

use egui::{Align2, FontId, Frame, Id, Key, RichText, Sense, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::{Dialog, GitApp, util};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

/// What an entry does.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Command(&'static str),
    Checkout(String),
    Open(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub detail: String,
    pub target: Target,
    pub disabled: Option<&'static str>,
}

/// Fuzzy match: every query character in order; higher is better (`None` when not all found).
pub fn score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut s = 0i32;
    let mut at = 0usize;
    let mut prev: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = t.iter().enumerate().skip(at).find(|(_, c)| **c == q).map(|(i, _)| i)?;
        // Consecutive letters and word starts score more.
        if prev.is_some_and(|p| p + 1 == found) {
            s += 6;
        }
        if found == 0 || t.get(found - 1).is_some_and(|c| !c.is_alphanumeric()) {
            s += 4;
        }
        s -= (found - at) as i32;
        prev = Some(found);
        at = found + 1;
    }
    Some(s)
}

pub fn entries(app: &GitApp, query: &str) -> Vec<Entry> {
    let mut v: Vec<(i32, Entry)> = Vec::new();
    for c in app.session.registry.all() {
        // Queries and selection plumbing are for agents, not the palette.
        if matches!(c.location, "Agents")
            || matches!(c.id, "graph.select" | "file.select" | "commit.message" | "diff.file" | "commit.show" | "file.blame" | "conflict.versions")
        {
            continue;
        }
        let text = format!("{} {}", c.label, c.location);
        if let Some(sc) = score(query, &text) {
            let key = if c.shortcut.is_empty() { String::new() } else { format!("   {}", util::shortcut_label(c.shortcut)) };
            v.push((
                sc,
                Entry {
                    label: c.label.to_string(),
                    detail: format!("{}{key}", c.location),
                    target: Target::Command(c.id),
                    disabled: (c.enabled)(&app.session),
                },
            ));
        }
    }
    for (id, label, loc) in [
        ("ui.preferences", "Preferences", "Settings"),
        ("ui.shortcuts", "Keyboard Shortcuts", "Help"),
        ("ui.theme", "Switch Light/Dark Theme", "View"),
        ("ui.revealRepo", "Show in File Manager", "Repository"),
    ] {
        if let Some(sc) = score(query, label) {
            v.push((sc, Entry { label: label.into(), detail: loc.into(), target: Target::Command(id), disabled: None }));
        }
    }
    for r in app.session.data.local_branches().filter(|r| !r.head) {
        let text = format!("Checkout {}", r.name);
        if let Some(sc) = score(query, &text) {
            v.push((sc - 1, Entry { label: text, detail: "Branch".into(), target: Target::Checkout(r.name.clone()), disabled: None }));
        }
    }
    for p in &app.prefs.recent {
        let text = format!("Open {p}");
        if let Some(sc) = score(query, &text) {
            v.push((sc - 2, Entry { label: text, detail: "Recent repository".into(), target: Target::Open(p.clone()), disabled: None }));
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.disabled.is_some().cmp(&b.1.disabled.is_some())));
    v.into_iter().map(|(_, e)| e).take(200).collect()
}

fn activate(app: &mut GitApp, e: &Entry) {
    app.palette = None;
    match &e.target {
        Target::Command(id) => {
            // Commands that need a name or a choice open their dialog.
            if let Some(d) = Dialog::named(id, &app.session, &json!({})).filter(|_| !matches!(*id, "stash.save")) {
                app.dialog = Some(d);
            } else if id.starts_with("ui.") {
                let _ = app.ui_command(id, &json!({}));
            } else if *id == "commit.reword" {
                // Edits the selected commit's message in the commit panel.
                let _ = app.ui_command("ui.editMessage", &json!({}));
            } else {
                let _ = app.run(id, json!({}));
            }
        }
        Target::Checkout(b) => {
            let _ = app.run("branch.checkout", json!({"name": b}));
        }
        Target::Open(p) => {
            let _ = app.open_repo(std::path::Path::new(p));
        }
    }
}

pub fn show(app: &mut GitApp, ctx: &egui::Context) {
    let Some(mut pal) = app.palette.take() else { return };
    let t = Tokens::get(ctx);
    let list = entries(app, &pal.query);
    pal.selected = pal.selected.min(list.len().saturating_sub(1));
    let (up, down, enter, esc) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
        )
    });
    if esc {
        return;
    }
    if up {
        pal.selected = pal.selected.saturating_sub(1);
    }
    if down && pal.selected + 1 < list.len() {
        pal.selected += 1;
    }
    if enter && let Some(e) = list.get(pal.selected).filter(|e| e.disabled.is_none()) {
        activate(app, e);
        return;
    }
    let mut chosen: Option<Entry> = None;
    let mut close = false;
    let screen = ctx.content_rect();
    let w = 560.0f32.min(screen.width() - 40.0);
    let area = egui::Area::new(Id::new("palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.center().x - w / 2.0, screen.top() + 70.0))
        .show(ctx, |ui| {
            Frame::popup(ui.style()).inner_margin(8).show(ui, |ui| {
                ui.set_width(w);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut pal.query)
                        .hint_text("Type a command, branch or repository")
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(15.0)),
                );
                r.request_focus();
                if r.changed() {
                    pal.selected = 0;
                }
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    for (i, e) in list.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
                        if i == pal.selected {
                            ui.painter().rect_filled(rect, 4.0, t.selection);
                            if down || up {
                                ui.scroll_to_rect(rect, None);
                            }
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, 4.0, t.hover);
                        }
                        let c = if e.disabled.is_some() { t.dim } else { t.text };
                        ui.painter().text(rect.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, &e.label, FontId::proportional(13.0), c);
                        ui.painter().text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, &e.detail, FontId::proportional(11.0), t.dim);
                        let resp = match e.disabled {
                            Some(why) => resp.on_hover_text(why),
                            None => resp,
                        };
                        if resp.clicked() && e.disabled.is_none() {
                            chosen = Some(e.clone());
                        }
                    }
                    if list.is_empty() {
                        ui.label(RichText::new("Nothing matches").weak());
                    }
                });
            });
        });
    // A click outside closes it.
    let inside = area.response.rect;
    if ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !inside.contains(p))) {
        close = true;
    }
    match chosen {
        Some(e) => activate(app, &e),
        None if !close => app.palette = Some(pal),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn fuzzy() {
        assert!(score("cmt", "Commit").is_some());
        assert!(score("xyz", "Commit").is_none());
        assert!(score("stage all", "Stage All Changes").unwrap() > score("stage all", "Unstage All Changes").unwrap());
        assert_eq!(score("", "anything"), Some(0));
    }
}
