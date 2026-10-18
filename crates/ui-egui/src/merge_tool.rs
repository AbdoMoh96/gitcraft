//! The merge tool for a conflicted file: the current (ours) and incoming (theirs) sides side by
//! side, each conflict with a box to take that side's whole hunk and a box per line; picked lines
//! go to the output below in the order they were picked. The output can be edited by hand, and
//! Save writes it and marks the file resolved (`conflict.edit`).

use egui::{Color32, FontId, Frame, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};
use gitcraft_engine::git::Chunk;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{Dialog, GitApp};

const ROW_H: f32 = 19.0;
const GUTTER: f32 = 40.0;
const BOX: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

/// The merge tool's state for one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeTool {
    pub path: String,
    pub chunks: Vec<Chunk>,
    /// Per conflict (in file order): the lines picked, in the order they were picked.
    pub picks: Vec<Vec<(Side, usize)>>,
    /// The output edited by hand; `None` while it follows the picks.
    pub edited: Option<String>,
    /// Why the file can't be merged line by line (binary, deleted on a side…).
    pub error: Option<String>,
}

impl MergeTool {
    pub fn new(path: &str, chunks: Vec<Chunk>) -> MergeTool {
        let n = chunks.iter().filter(|c| matches!(c, Chunk::Conflict { .. })).count();
        MergeTool { path: path.to_string(), chunks, picks: vec![Vec::new(); n], edited: None, error: None }
    }

    fn conflicts(&self) -> impl Iterator<Item = (&Vec<String>, &Vec<String>)> {
        self.chunks.iter().filter_map(|c| match c {
            Chunk::Conflict { ours, theirs, .. } => Some((ours, theirs)),
            Chunk::Common { .. } => None,
        })
    }

    fn side_len(&self, ci: usize, side: Side) -> usize {
        self.conflicts().nth(ci).map(|(o, t)| if side == Side::Ours { o.len() } else { t.len() }).unwrap_or(0)
    }

    pub fn picked(&self, ci: usize, side: Side, line: usize) -> bool {
        self.picks.get(ci).is_some_and(|p| p.contains(&(side, line)))
    }

    /// Whether every line of a side is picked (an empty side: never).
    pub fn hunk_picked(&self, ci: usize, side: Side) -> bool {
        let n = self.side_len(ci, side);
        n > 0 && (0..n).all(|l| self.picked(ci, side, l))
    }

    pub fn toggle_line(&mut self, ci: usize, side: Side, line: usize) {
        if line >= self.side_len(ci, side) {
            return;
        }
        if let Some(p) = self.picks.get_mut(ci) {
            if let Some(i) = p.iter().position(|x| *x == (side, line)) {
                p.remove(i);
            } else {
                p.push((side, line));
            }
        }
        self.edited = None;
    }

    /// Take a side's whole hunk, or drop it when it's all taken.
    pub fn toggle_hunk(&mut self, ci: usize, side: Side) {
        let all = self.hunk_picked(ci, side);
        let n = self.side_len(ci, side);
        if let Some(p) = self.picks.get_mut(ci) {
            p.retain(|(s, _)| *s != side);
            if !all {
                p.extend((0..n).map(|l| (side, l)));
            }
        }
        self.edited = None;
    }

    /// Take one side for every conflict.
    pub fn take_all(&mut self, side: Side) {
        for ci in 0..self.picks.len() {
            let n = self.side_len(ci, side);
            if let Some(p) = self.picks.get_mut(ci) {
                *p = (0..n).map(|l| (side, l)).collect();
            }
        }
        self.edited = None;
    }

    pub fn reset(&mut self) {
        self.picks.iter_mut().for_each(Vec::clear);
        self.edited = None;
    }

    /// Conflicts with nothing picked.
    pub fn unresolved(&self) -> usize {
        self.picks.iter().filter(|p| p.is_empty()).count()
    }

    /// The output from the picks: common lines, and each conflict's picked lines in pick order.
    pub fn output(&self) -> String {
        let mut out = String::new();
        let mut ci = 0;
        for c in &self.chunks {
            match c {
                Chunk::Common { lines } => lines.iter().for_each(|l| out.push_str(l)),
                Chunk::Conflict { ours, theirs, .. } => {
                    for (side, l) in self.picks.get(ci).map(Vec::as_slice).unwrap_or(&[]) {
                        let line = if *side == Side::Ours { ours.get(*l) } else { theirs.get(*l) };
                        if let Some(line) = line {
                            out.push_str(line);
                            // A side's last line without a newline, followed by more lines.
                            if !line.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                    }
                    ci += 1;
                }
            }
        }
        out
    }

    /// The text Save writes.
    pub fn result(&self) -> String {
        self.edited.clone().unwrap_or_else(|| self.output())
    }

    /// Save is possible when every conflict has lines picked, or the output was edited by hand.
    pub fn can_save(&self) -> bool {
        self.error.is_none() && (self.edited.is_some() || self.unresolved() == 0)
    }
}

/// A row of the side-by-side panes.
enum Row {
    /// A line both sides have: (chunk, line).
    Common(usize, usize),
    /// A conflict's header: (conflict index).
    Header(usize),
    /// A conflict line: (chunk, conflict index, line) — either side may be past its end.
    Line(usize, usize, usize),
}

fn rows(chunks: &[Chunk]) -> Vec<Row> {
    let mut out = Vec::new();
    let mut ci = 0;
    for (k, c) in chunks.iter().enumerate() {
        match c {
            Chunk::Common { lines } => out.extend((0..lines.len()).map(|l| Row::Common(k, l))),
            Chunk::Conflict { ours, theirs, .. } => {
                out.push(Row::Header(ci));
                out.extend((0..ours.len().max(theirs.len()).max(1)).map(|l| Row::Line(k, ci, l)));
                ci += 1;
            }
        }
    }
    out
}

/// Line numbers of each row on both sides (`None` where a side has no line).
fn numbers(chunks: &[Chunk], rows: &[Row]) -> Vec<(Option<usize>, Option<usize>)> {
    let (mut a, mut b) = (0usize, 0usize);
    rows.iter()
        .map(|r| match *r {
            Row::Common(..) => {
                a += 1;
                b += 1;
                (Some(a), Some(b))
            }
            Row::Header(_) => (None, None),
            Row::Line(k, _, l) => {
                let (o, t) = match chunks.get(k) {
                    Some(Chunk::Conflict { ours, theirs, .. }) => (l < ours.len(), l < theirs.len()),
                    _ => (false, false),
                };
                let na = o.then(|| {
                    a += 1;
                    a
                });
                let nb = t.then(|| {
                    b += 1;
                    b
                });
                (na, nb)
            }
        })
        .collect()
}

fn display(text: &str) -> String {
    text.trim_end_matches(['\n', '\r']).replace('\t', "    ")
}

/// Load the merge tool for a conflicted file (once per file).
fn load(app: &mut GitApp, path: &str) {
    if app.merge.as_ref().is_some_and(|m| m.path == path) {
        return;
    }
    let r = app.session.run("conflict.versions", &json!({"path": path}));
    let mut m = match r {
        Ok(v) => {
            let chunks: Vec<Chunk> = serde_json::from_value(v.get("chunks").cloned().unwrap_or(Value::Null)).unwrap_or_default();
            let mut m = MergeTool::new(path, chunks);
            if !v.get("mergeable").and_then(Value::as_bool).unwrap_or(false) {
                let gone = |k: &str| v.get(k).is_none_or(Value::is_null);
                m.error = Some(if gone("ours") || gone("theirs") {
                    "The file was deleted on one side: take the current or the incoming version.".into()
                } else {
                    "No conflict markers to merge line by line (a binary file, or edited by hand): take a side, or edit the file and mark it resolved.".into()
                });
            }
            m
        }
        Err(e) => MergeTool { path: path.to_string(), error: Some(e.to_string()), ..MergeTool::default() },
    };
    if m.chunks.is_empty() && m.error.is_none() {
        m.error = Some("The file is empty.".into());
    }
    app.merge = Some(m);
}

pub fn show(app: &mut GitApp, ui: &mut egui::Ui, path: &str) {
    load(app, path);
    let t = Tokens::get(ui.ctx());
    let mut run: Option<(&'static str, Value)> = None;
    let mut dialog: Option<Dialog> = None;
    let Some(mut m) = app.merge.take() else { return };
    header(ui, &mut m, &mut run, &mut dialog);
    if let Some(e) = &m.error {
        Frame::NONE.fill(t.warning_bg).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(e).color(t.warning));
        });
    } else {
        egui::Panel::bottom(egui::Id::new(("merge_output", &m.path)))
            .resizable(true)
            .default_size((ui.available_height() * 0.4).max(140.0))
            .size_range(90.0..=(ui.available_height() - 120.0).max(100.0))
            .frame(Frame::NONE.fill(t.bg).stroke(Stroke::new(1.0, t.border)))
            .show(ui, |ui| output(ui, &mut m));
        sides(ui, &mut m);
    }
    app.merge = Some(m);
    if let Some(d) = dialog {
        app.dialog = Some(d);
    }
    if let Some((id, p)) = run {
        if app.run(id, p).is_ok() && id != "file.close" {
            // Resolved: back to the file list.
            app.merge = None;
            let _ = app.run("file.close", json!({}));
        } else if id == "file.close" {
            app.merge = None;
        }
    }
}

fn header(ui: &mut egui::Ui, m: &mut MergeTool, run: &mut Option<(&'static str, Value)>, dialog: &mut Option<Dialog>) {
    let t = Tokens::get(ui.ctx());
    Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 5)).stroke(Stroke::new(1.0, t.border)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::Warning, t.warning, t.warning);
            ui.label(RichText::new(&m.path).strong());
            let total = m.picks.len();
            if m.error.is_none() {
                let done = total.saturating_sub(m.unresolved());
                let c = if done == total { t.added } else { t.warning };
                ui.label(RichText::new(format!("{done} of {total} conflict{} resolved", if total == 1 { "" } else { "s" })).small().color(c));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::Close, "Close the merge tool (Esc)", None, 14.0).clicked() {
                    *run = Some(("file.close", json!({})));
                }
                let save = ui
                    .add_enabled(m.can_save(), egui::Button::new(RichText::new("Save and Mark Resolved").strong()))
                    .on_disabled_hover_text("Pick lines for every conflict, or edit the output");
                if save.clicked() {
                    *run = Some(("conflict.edit", json!({"path": m.path, "content": m.result()})));
                }
            });
        });
        ui.horizontal(|ui| {
            if m.error.is_none() {
                if ui.button("All Current").on_hover_text("Take the current side of every conflict").clicked() {
                    m.take_all(Side::Ours);
                }
                if ui.button("All Incoming").on_hover_text("Take the incoming side of every conflict").clicked() {
                    m.take_all(Side::Theirs);
                }
                if ui.button("Reset").on_hover_text("Unpick every line").clicked() {
                    m.reset();
                }
                ui.separator();
            }
            if ui.button("Take Current File").on_hover_text("Resolve with the current (ours) version").clicked() {
                *dialog = Some(Dialog::confirm(
                    &format!("Take the current version of {}?", m.path),
                    "The incoming side's changes to this file are dropped.",
                    "conflict.takeOurs",
                    json!({"path": m.path}),
                    "Take Current",
                ));
            }
            if ui.button("Take Incoming File").on_hover_text("Resolve with the incoming (theirs) version").clicked() {
                *dialog = Some(Dialog::confirm(
                    &format!("Take the incoming version of {}?", m.path),
                    "The current side's changes to this file are dropped.",
                    "conflict.takeTheirs",
                    json!({"path": m.path}),
                    "Take Incoming",
                ));
            }
        });
    });
}

/// A pick box; returns whether it was clicked.
fn pick_box(ui: &mut egui::Ui, at: Rect, on: bool, color: Color32, tip: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let r = ui.interact(at, ui.id().with(("pick", at.min.x as i32, at.min.y as i32)), Sense::click()).on_hover_text(tip);
    let b = Rect::from_center_size(at.center(), vec2(12.0, 12.0));
    let p = ui.painter();
    if on {
        p.rect_filled(b, 3.0, color);
        icons::paint(p, b.shrink(1.0), Icon::Check, Color32::WHITE, Color32::WHITE);
    } else {
        p.rect_stroke(b, 3.0, Stroke::new(1.3, if r.hovered() { color } else { t.dim }), egui::StrokeKind::Inside);
    }
    r.clicked()
}

/// Both sides, row for row; a conflict's shorter side is padded so the two stay aligned.
fn sides(ui: &mut egui::Ui, m: &mut MergeTool) {
    let t = Tokens::get(ui.ctx());
    let rows = rows(&m.chunks);
    let nums = numbers(&m.chunks, &rows);
    let width = ui.available_width();
    let half = (width / 2.0).floor();
    // Titles.
    let (title, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(title, 0.0, t.panel);
    let (ours_c, theirs_c) = (t.accent, t.modified);
    p.text(pos2(title.left() + 10.0, title.center().y), egui::Align2::LEFT_CENTER, "CURRENT (ours)", FontId::proportional(11.0), ours_c);
    p.text(pos2(title.left() + half + 10.0, title.center().y), egui::Align2::LEFT_CENTER, "INCOMING (theirs)", FontId::proportional(11.0), theirs_c);
    p.line_segment([pos2(title.left() + half, title.top()), pos2(title.left() + half, title.bottom())], Stroke::new(1.0, t.border));

    let mono = FontId::monospace(12.5);
    let mut toggle: Option<(usize, Side, Option<usize>)> = None;
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical().id_salt(("merge_sides", &m.path)).auto_shrink([false, false]).show_rows(ui, ROW_H, rows.len(), |ui, range| {
        let start = range.start;
        for (i, row) in rows.get(range).unwrap_or(&[]).iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
            let left = Rect::from_min_max(rect.min, pos2(rect.left() + half, rect.bottom()));
            let right = Rect::from_min_max(pos2(rect.left() + half, rect.top()), rect.max);
            let (na, nb) = nums.get(start + i).copied().unwrap_or((None, None));
            let p = ui.painter().clone();
            let num = |r: Rect, n: Option<usize>| {
                if let Some(n) = n {
                    p.text(pos2(r.left() + GUTTER - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, n.to_string(), FontId::monospace(10.5), t.dim);
                }
            };
            match *row {
                Row::Common(k, l) => {
                    let text = match m.chunks.get(k) {
                        Some(Chunk::Common { lines }) => lines.get(l).map(|s| display(s)).unwrap_or_default(),
                        _ => String::new(),
                    };
                    for (r, n) in [(left, na), (right, nb)] {
                        num(r, n);
                        let clip = p.with_clip_rect(r.intersect(ui.clip_rect()));
                        clip.text(pos2(r.left() + GUTTER + BOX + 4.0, r.center().y), egui::Align2::LEFT_CENTER, &text, mono.clone(), t.dim);
                    }
                }
                Row::Header(ci) => {
                    p.rect_filled(rect, 0.0, t.hunk_bg);
                    for (r, side, c) in [(left, Side::Ours, ours_c), (right, Side::Theirs, theirs_c)] {
                        let n = m.side_len(ci, side);
                        let b = Rect::from_min_size(pos2(r.left() + GUTTER, r.top()), vec2(BOX, ROW_H));
                        if n > 0 && pick_box(ui, b, m.hunk_picked(ci, side), c, "Take this side's whole hunk") {
                            toggle = Some((ci, side, None));
                        }
                        let label = format!(
                            "Conflict {} — {}",
                            ci + 1,
                            if n == 0 { "nothing on this side".to_string() } else { format!("{n} line{}", if n == 1 { "" } else { "s" }) }
                        );
                        p.text(pos2(b.right() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(11.0), t.dim);
                    }
                }
                Row::Line(k, ci, l) => {
                    let (ours, theirs) = match m.chunks.get(k) {
                        Some(Chunk::Conflict { ours, theirs, .. }) => (ours.get(l), theirs.get(l)),
                        _ => (None, None),
                    };
                    for (r, side, line, n, c, bg) in
                        [(left, Side::Ours, ours, na, ours_c, t.removed_bg), (right, Side::Theirs, theirs, nb, theirs_c, t.added_bg)]
                    {
                        let Some(line) = line else {
                            p.rect_filled(r, 0.0, t.panel);
                            continue;
                        };
                        let on = m.picked(ci, side, l);
                        p.rect_filled(r, 0.0, if on { t.picked_bg } else { bg });
                        num(r, n);
                        let b = Rect::from_min_size(pos2(r.left() + GUTTER, r.top()), vec2(BOX, ROW_H));
                        if pick_box(ui, b, on, c, "Take this line") {
                            toggle = Some((ci, side, Some(l)));
                        }
                        // A click anywhere on the line picks it too.
                        let hit = Rect::from_min_max(pos2(b.right(), r.top()), r.max);
                        if ui.interact(hit, ui.id().with(("line", side == Side::Ours, ci, l)), Sense::click()).clicked() {
                            toggle = Some((ci, side, Some(l)));
                        }
                        let clip = p.with_clip_rect(r.intersect(ui.clip_rect()));
                        clip.text(pos2(b.right() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, display(line), mono.clone(), t.text);
                    }
                }
            }
            p.line_segment([pos2(left.right(), rect.top()), pos2(left.right(), rect.bottom())], Stroke::new(1.0, t.border));
        }
    });
    ui.spacing_mut().item_spacing.y = spacing;
    match toggle {
        Some((ci, side, Some(l))) => m.toggle_line(ci, side, l),
        Some((ci, side, None)) => m.toggle_hunk(ci, side),
        None => {}
    }
}

/// The output: what Save writes. Read-only (following the picks) until Edit.
fn output(ui: &mut egui::Ui, m: &mut MergeTool) {
    let t = Tokens::get(ui.ctx());
    Frame::NONE.fill(t.panel).inner_margin(Margin::symmetric(8, 3)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("OUTPUT").small().strong().color(t.dim));
            let n = m.unresolved();
            if m.edited.is_some() {
                ui.label(RichText::new("edited by hand").small().color(t.modified));
            } else if n > 0 {
                ui.label(RichText::new(format!("{n} conflict{} left: pick lines above", if n == 1 { "" } else { "s" })).small().color(t.warning));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if m.edited.is_some() {
                    if ui.small_button("Back to Picks").on_hover_text("Drop the hand edits").clicked() {
                        m.edited = None;
                    }
                } else if ui.small_button("Edit").on_hover_text("Edit the output by hand").clicked() {
                    m.edited = Some(m.output());
                }
            });
        });
    });
    egui::ScrollArea::both().id_salt(("merge_out", &m.path)).auto_shrink([false, false]).show(ui, |ui| {
        if let Some(text) = m.edited.as_mut() {
            ui.add(egui::TextEdit::multiline(text).code_editor().desired_width(f32::INFINITY).desired_rows(12));
            return;
        }
        // Following the picks: unresolved conflicts show as a marked row.
        let mono = FontId::monospace(12.5);
        let mut n = 0usize;
        let mut ci = 0usize;
        let mut line = |ui: &mut egui::Ui, text: &str, color: Color32, bg: Option<Color32>, num: bool| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(400.0), ROW_H), Sense::hover());
            if let Some(bg) = bg {
                ui.painter().rect_filled(r, 0.0, bg);
            }
            if num {
                n += 1;
                ui.painter().text(
                    pos2(r.left() + GUTTER - 6.0, r.center().y),
                    egui::Align2::RIGHT_CENTER,
                    n.to_string(),
                    FontId::monospace(10.5),
                    t.dim,
                );
            }
            ui.painter().text(pos2(r.left() + GUTTER + 4.0, r.center().y), egui::Align2::LEFT_CENTER, display(text), mono.clone(), color);
        };
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        for c in &m.chunks {
            match c {
                Chunk::Common { lines } => lines.iter().for_each(|l| line(ui, l, t.text, None, true)),
                Chunk::Conflict { ours, theirs, .. } => {
                    let picks = m.picks.get(ci).map(Vec::as_slice).unwrap_or(&[]);
                    if picks.is_empty() {
                        line(ui, &format!("  conflict {} not resolved", ci + 1), t.warning, Some(t.warning_bg), false);
                    }
                    for (side, l) in picks {
                        let text = if *side == Side::Ours { ours.get(*l) } else { theirs.get(*l) };
                        if let Some(text) = text {
                            line(ui, text, t.text, Some(t.picked_bg), true);
                        }
                    }
                    ci += 1;
                }
            }
        }
        ui.spacing_mut().item_spacing.y = spacing;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> MergeTool {
        let chunks =
            gitcraft_engine::git::conflict::parse("a\n<<<<<<< x\no1\no2\n=======\nt1\n>>>>>>> y\nb\n<<<<<<< x\n=======\nonly theirs\n>>>>>>> y\n");
        MergeTool::new("f.txt", chunks)
    }

    #[test]
    fn picks_build_the_output_in_pick_order() {
        let mut m = tool();
        assert_eq!(m.picks.len(), 2);
        assert_eq!(m.unresolved(), 2);
        assert!(!m.can_save());
        m.toggle_line(0, Side::Theirs, 0);
        m.toggle_line(0, Side::Ours, 1);
        m.toggle_hunk(1, Side::Theirs);
        assert_eq!(m.output(), "a\nt1\no2\nb\nonly theirs\n");
        assert!(m.can_save());
        // Unpicking, a whole hunk on and off, an empty side, out-of-range lines.
        m.toggle_line(0, Side::Theirs, 0);
        m.toggle_hunk(0, Side::Ours);
        assert!(m.hunk_picked(0, Side::Ours));
        assert_eq!(m.output(), "a\no1\no2\nb\nonly theirs\n");
        m.toggle_hunk(0, Side::Ours);
        assert_eq!(m.unresolved(), 1);
        m.toggle_hunk(1, Side::Ours);
        m.toggle_line(9, Side::Ours, 9);
        m.toggle_line(0, Side::Ours, 99);
        assert!(!m.hunk_picked(1, Side::Ours));
        m.take_all(Side::Ours);
        assert_eq!(m.unresolved(), 1, "the second conflict has nothing on our side");
        m.take_all(Side::Theirs);
        assert_eq!(m.result(), "a\nt1\nb\nonly theirs\n");
        // Hand edits win until a pick changes.
        m.edited = Some("mine\n".into());
        assert_eq!(m.result(), "mine\n");
        m.toggle_line(0, Side::Ours, 0);
        assert_eq!(m.edited, None);
        m.reset();
        assert_eq!(m.unresolved(), 2);
    }

    #[test]
    fn rows_align_both_sides() {
        let m = tool();
        let r = rows(&m.chunks);
        // a, header, 2 lines, b, header, 1 line.
        assert_eq!(r.len(), 7);
        let n = numbers(&m.chunks, &r);
        assert_eq!(n[4], (Some(4), Some(3)));
        assert_eq!(n[6], (None, Some(4)));
    }
}
