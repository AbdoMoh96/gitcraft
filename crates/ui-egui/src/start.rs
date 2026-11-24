//! The start screen, when no repository is open: open, clone or create one, or pick a recent one.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{APP_COLOR, Tokens};
use crate::{Dialog, GitApp};

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space((ui.available_height() * 0.12).max(20.0));
    ui.vertical_centered(|ui| {
        // The mark: three commits on two lanes, drawn in code.
        let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), egui::Sense::hover());
        let p = ui.painter();
        p.circle_filled(r.center(), 36.0, APP_COLOR);
        let (a, b, c) = (r.center() + vec2(-12.0, -16.0), r.center() + vec2(-12.0, 16.0), r.center() + vec2(13.0, -2.0));
        p.line_segment([a, b], egui::Stroke::new(4.0, Color32::WHITE));
        p.add(egui::epaint::CubicBezierShape::from_points_stroke(
            [c, c + vec2(0.0, 12.0), b + vec2(10.0, -6.0), b + vec2(0.0, -6.0)],
            false,
            Color32::TRANSPARENT,
            egui::Stroke::new(4.0, Color32::WHITE),
        ));
        for x in [a, b, c] {
            p.circle_filled(x, 6.0, Color32::WHITE);
        }
        ui.add_space(10.0);
        ui.label(RichText::new("GitCraft").size(28.0).strong());
        ui.label(RichText::new("A fast, open-source Git client").color(t.dim));
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            let w = 3.0 * 150.0 + 2.0 * 12.0;
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for (icon, label, name) in [
                (Icon::Folder, "Open a repository", "repo.open"),
                (Icon::Cloud, "Clone a repository", "repo.clone"),
                (Icon::Plus, "Start a new one", "repo.init"),
            ] {
                let (rect, resp) = ui.allocate_exact_size(vec2(150.0, 92.0), egui::Sense::click());
                ui.painter().rect_filled(rect, 8.0, if resp.hovered() { t.hover } else { t.panel });
                ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                icons::paint(ui.painter(), egui::Rect::from_center_size(rect.center() - vec2(0.0, 14.0), vec2(28.0, 28.0)), icon, t.text, t.accent);
                ui.painter().text(rect.center() + vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, label, FontId::proportional(13.0), t.text);
                if resp.clicked() {
                    app.dialog = Dialog::named(name, &app.session, &json!({}));
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(26.0);
        most_used(app, ui);
        if !app.prefs.favorites.is_empty() {
            ui.label(RichText::new("FAVOURITES").small().strong().color(t.dim));
            ui.add_space(4.0);
            for p in app.prefs.favorites.clone() {
                let name = std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
                let r = ui.add(egui::Button::new(RichText::new(format!("{name}   {p}")).size(13.0)).frame(false));
                if r.clicked() {
                    let _ = app.open_repo(std::path::Path::new(&p));
                }
                r.context_menu(|ui| {
                    if ui.button("Remove from Favourites").clicked() {
                        app.toggle_favorite(&p);
                        ui.close();
                    }
                });
            }
            ui.add_space(12.0);
        }
        if !app.prefs.recent.is_empty() {
            ui.label(RichText::new("RECENT REPOSITORIES").small().strong().color(t.dim));
            ui.add_space(4.0);
            for (i, p) in app.prefs.recent.clone().iter().enumerate() {
                let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
                let r = ui.add(egui::Button::new(RichText::new(format!("{name}   {p}")).size(13.0)).frame(false));
                if r.clicked() {
                    let _ = app.ui_command("ui.recent", &json!({"index": i}));
                }
                r.context_menu(|ui| {
                    if ui.button("Add to Favourites").clicked() {
                        app.toggle_favorite(p);
                        ui.close();
                    }
                });
            }
        }
        ui.add_space(12.0);
        if ui.button("New to Git? Learn with a practice repository…").clicked() {
            app.dialog = Dialog::named("ui.learn", &app.session, &json!({}));
        }
        ui.add_space(16.0);
        ui.label(
            RichText::new(format!("Or drop a folder on this window. {} finds any command.", crate::util::shortcut_label("Mod+P")))
                .small()
                .color(t.dim),
        );
    });
}

/// Side of a most used tile, the gap between tiles, and tiles per row.
const TILE: f32 = 112.0;
const TILE_GAP: f32 = 12.0;
const TILES_PER_ROW: usize = 5;

/// The most opened repositories as clickable squares, five to a row.
fn most_used(app: &mut GitApp, ui: &mut egui::Ui) {
    let repos = app.prefs.most_used(crate::MOST_USED);
    if repos.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("MOST USED").small().strong().color(t.dim));
    ui.add_space(6.0);
    for row in repos.chunks(TILES_PER_ROW) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let w = row.len() as f32 * TILE + row.len().saturating_sub(1) as f32 * TILE_GAP;
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for (i, (path, opens)) in row.iter().enumerate() {
                if i > 0 {
                    ui.add_space(TILE_GAP);
                }
                tile(app, ui, path, *opens);
            }
        });
        ui.add_space(TILE_GAP);
    }
    ui.add_space(14.0);
}

fn tile(app: &mut GitApp, ui: &mut egui::Ui, path: &str, opens: u32) {
    let t = Tokens::get(ui.ctx());
    let p = std::path::Path::new(path);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
    let parent = p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, if resp.hovered() { t.hover } else { t.panel });
    painter.rect_stroke(rect, 10.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
    // A badge with the name's first letter, coloured from the name so each repository keeps its colour.
    let hue = name.bytes().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(b)));
    let badge = Rect::from_center_size(pos2(rect.center().x, rect.top() + 34.0), vec2(44.0, 44.0));
    painter.rect_filled(badge, 10.0, crate::theme::lane_color(hue));
    let initial: String = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    painter.text(badge.center(), Align2::CENTER_CENTER, initial, FontId::proportional(22.0), Color32::WHITE);
    let text = rect.shrink2(vec2(8.0, 0.0));
    let clip = painter.with_clip_rect(text);
    clip.text(pos2(text.center().x, rect.top() + 72.0), Align2::CENTER_CENTER, &name, FontId::proportional(13.0), t.text);
    clip.text(pos2(text.center().x, rect.top() + 90.0), Align2::CENTER_CENTER, &parent, FontId::proportional(11.0), t.dim);
    let times = if opens == 1 { "once".to_string() } else { format!("{opens} times") };
    let resp = resp.on_hover_text(if opens == 0 { path.to_string() } else { format!("{path}\nOpened {times}") });
    if resp.clicked() {
        let _ = app.open_repo(p);
    }
    resp.context_menu(|ui| {
        let fav = app.prefs.favorites.iter().any(|f| f == path);
        if ui.button(if fav { "Remove from Favourites" } else { "Add to Favourites" }).clicked() {
            app.toggle_favorite(path);
            ui.close();
        }
        if ui.button("Forget This Repository").on_hover_text("Removes it from the recent and most used lists; nothing on disk changes").clicked() {
            app.prefs.forget_repo(path);
            ui.close();
        }
    });
}
