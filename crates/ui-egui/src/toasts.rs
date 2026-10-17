//! Notifications: a card per result (pushed, merged, failed…) sliding in at the bottom left,
//! stacked newest at the bottom, gone after a few seconds (errors stay longer; hovering keeps one).

use egui::{Color32, Frame, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::util;

/// Seconds a notification stays: success, error.
const STAY_OK: f64 = 5.0;
const STAY_ERR: f64 = 10.0;
/// Seconds to slide in and to fade out.
const ENTER: f64 = 0.28;
const LEAVE: f64 = 0.35;
/// Cards shown at once (older ones leave first).
const MAX_SHOWN: usize = 5;
/// Cards kept (the rest are dropped).
const MAX_KEPT: usize = 20;
const WIDTH: f32 = 340.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    id: u64,
    /// What it is about (the command's name), if known.
    pub title: Option<String>,
    pub text: String,
    pub error: bool,
    /// When it appeared (seconds since the epoch); reset while hovered.
    at: f64,
    /// When it started leaving (closed by hand).
    leaving: Option<f64>,
    /// Height last frame, for stacking.
    height: f32,
}

#[derive(Default)]
pub struct Toasts {
    pub notes: Vec<Note>,
    next: u64,
}

fn ease_out(x: f64) -> f32 {
    let x = x.clamp(0.0, 1.0);
    (1.0 - (1.0 - x).powi(3)) as f32
}

impl Toasts {
    pub fn push(&mut self, title: Option<&str>, text: impl Into<String>, error: bool) {
        let text: String = text.into();
        let text = text.trim().chars().take(2000).collect::<String>();
        if text.is_empty() {
            return;
        }
        // The same message again just restarts its clock.
        let now = util::now_s();
        if let Some(n) = self.notes.iter_mut().find(|n| n.text == text && n.error == error && n.leaving.is_none()) {
            n.at = now - ENTER;
            return;
        }
        self.next = self.next.wrapping_add(1);
        let title = title.map(str::to_string);
        self.notes.push(Note { id: self.next, title, text, error, at: now, leaving: None, height: 56.0 });
        if self.notes.len() > MAX_KEPT {
            self.notes.remove(0);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, bottom_margin: f32) {
        if self.notes.is_empty() {
            return;
        }
        let now = util::now_s();
        // Expire: past their time, or pushed out by newer ones.
        let shown = self.notes.len().saturating_sub(MAX_SHOWN);
        for (i, n) in self.notes.iter_mut().enumerate() {
            let stay = if n.error { STAY_ERR } else { STAY_OK };
            if n.leaving.is_none() && (now - n.at > stay || i < shown) {
                n.leaving = Some(now);
            }
        }
        self.notes.retain(|n| n.leaving.is_none_or(|l| now - l < LEAVE));

        let t = Tokens::get(ctx);
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - bottom_margin - 12.0;
        let mut close: Option<u64> = None;
        let mut hovered: Option<u64> = None;
        for n in self.notes.iter_mut().rev() {
            let enter = ease_out((now - n.at) / ENTER);
            let leave = n.leaving.map(|l| ease_out((now - l) / LEAVE)).unwrap_or(0.0);
            let alpha = (enter * (1.0 - leave)).clamp(0.0, 1.0);
            let x = screen.left() + 12.0 - (1.0 - enter) * (WIDTH + 24.0);
            let h = n.height;
            let top = y - h + leave * 16.0;
            let area = egui::Area::new(egui::Id::new(("toast", n.id)))
                .order(egui::Order::Foreground)
                .fixed_pos(pos2(x, top))
                .interactable(true)
                .show(ctx, |ui| {
                    ui.multiply_opacity(alpha);
                    let accent = if n.error { t.danger } else { t.added };
                    let out = Frame::NONE
                        .fill(t.panel)
                        .stroke(Stroke::new(1.0, t.border))
                        .corner_radius(8)
                        .inner_margin(Margin { left: 14, right: 30, top: 10, bottom: 10 })
                        .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(70) })
                        .show(ui, |ui| {
                            ui.set_width(WIDTH - 44.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                                icons::paint(ui.painter(), r, if n.error { Icon::Warning } else { Icon::Check }, accent, accent);
                                ui.vertical(|ui| {
                                    let title = n.title.clone().unwrap_or_else(|| if n.error { "Error".into() } else { "Done".into() });
                                    ui.label(RichText::new(title).strong().size(12.5).color(accent));
                                    ui.add(egui::Label::new(RichText::new(&n.text).size(12.5).color(t.text)).wrap());
                                });
                            });
                        });
                    let card = out.response.rect;
                    // The colour strip on the left.
                    ui.painter().rect_filled(
                        Rect::from_min_max(card.left_top(), pos2(card.left() + 4.0, card.bottom())),
                        egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 },
                        accent,
                    );
                    let x_rect = Rect::from_center_size(pos2(card.right() - 16.0, card.top() + 16.0), vec2(18.0, 18.0));
                    let xr = ui.interact(x_rect, ui.id().with("close"), Sense::click());
                    let c = if xr.hovered() { t.text } else { t.dim };
                    icons::paint(ui.painter(), x_rect.shrink(3.0), Icon::Close, c, c);
                    let whole = ui.interact(card, ui.id().with("card"), Sense::click());
                    (card, xr.clicked() || whole.clicked(), whole.hovered() || xr.hovered())
                });
            let (card, clicked, hover) = area.inner;
            n.height = card.height();
            y = top - 8.0 + leave * 16.0;
            if clicked {
                close = Some(n.id);
            }
            if hover && n.leaving.is_none() {
                hovered = Some(n.id);
            }
            if n.error {
                // Copy the error text from the card's context menu.
                area.response.context_menu(|ui| {
                    if ui.button("Copy Message").clicked() {
                        ui.ctx().copy_text(n.text.clone());
                        ui.close();
                    }
                });
            }
        }
        for n in &mut self.notes {
            if Some(n.id) == close && n.leaving.is_none() {
                n.leaving = Some(now);
            }
            if Some(n.id) == hovered {
                n.at = now - ENTER;
            }
        }
        if !self.notes.is_empty() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_stack_dedupe_and_cap() {
        let mut t = Toasts::default();
        t.push(Some("Push"), "Pushed main", false);
        t.push(Some("Push"), "Pushed main", false);
        t.push(None, "  ", true);
        assert_eq!(t.notes.len(), 1);
        for i in 0..40 {
            t.push(None, format!("n{i}"), i % 2 == 0);
        }
        assert_eq!(t.notes.len(), MAX_KEPT);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| t.show(ui.ctx(), 24.0));
        out.textures_delta.clear();
        assert!(t.notes.iter().filter(|n| n.leaving.is_none()).count() <= MAX_SHOWN);
    }
}
