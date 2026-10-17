//! The start screen, when no repository is open: open, clone or create one, or pick a recent one.

use egui::{Color32, FontId, RichText, vec2};
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
        if !app.prefs.recent.is_empty() {
            ui.label(RichText::new("RECENT REPOSITORIES").small().strong().color(t.dim));
            ui.add_space(4.0);
            for (i, p) in app.prefs.recent.clone().iter().enumerate() {
                let name = std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
                let r = ui.add(egui::Button::new(RichText::new(format!("{name}   {p}")).size(13.0)).frame(false));
                if r.clicked() {
                    let _ = app.ui_command("ui.recent", &json!({"index": i}));
                }
            }
        }
        ui.add_space(16.0);
        ui.label(
            RichText::new(format!("Or drop a folder on this window. {} finds any command.", crate::util::shortcut_label("Mod+P")))
                .small()
                .color(t.dim),
        );
    });
}
