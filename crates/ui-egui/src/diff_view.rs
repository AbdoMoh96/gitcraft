//! The diff view: a file's changes as hunks (each with stage / unstage / discard), the whole file
//! inline, or old and new side by side; click changed lines to stage or discard only those. Blame
//! shows who last changed each line. Rows are virtualised for long files.

use egui::{Color32, FontId, Frame, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};
use gitcraft_engine::git::{FileDiff, LineKind};
use gitcraft_engine::{Area, DiffMode, FileSel};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{Dialog, GitApp, util};

const ROW_H: f32 = 19.0;
const GUTTER: f32 = 46.0;
/// Width of one monospace character at the diff's font size (for the horizontal extent).
const CHAR_W: f32 = 7.6;

/// Context the engine used for the diff on screen: hunk and line indices refer to it.
fn context_used(app: &GitApp) -> u32 {
    if app.session.view.diff_mode == DiffMode::Inline { 100_000 } else { app.session.view.context }
}

enum Row {
    Hunk(usize),
    Line(usize, usize),
    /// Split view: the old side's line and the new side's (indices into the hunk).
    Pair(usize, Option<usize>, Option<usize>),
}

fn rows_for(d: &FileDiff, split: bool) -> Vec<Row> {
    let mut out = Vec::new();
    for (hi, h) in d.hunks.iter().enumerate() {
        out.push(Row::Hunk(hi));
        if !split {
            out.extend((0..h.lines.len()).map(|li| Row::Line(hi, li)));
            continue;
        }
        let mut i = 0;
        while let Some(l) = h.lines.get(i) {
            match l.kind {
                LineKind::Context => {
                    out.push(Row::Pair(hi, Some(i), Some(i)));
                    i += 1;
                }
                LineKind::NoNewline => i += 1,
                LineKind::Del | LineKind::Add => {
                    let mut dels = Vec::new();
                    let mut adds = Vec::new();
                    while let Some(x) = h.lines.get(i) {
                        match x.kind {
                            LineKind::Del => dels.push(i),
                            LineKind::Add => adds.push(i),
                            LineKind::NoNewline => {}
                            LineKind::Context => break,
                        }
                        i += 1;
                    }
                    for k in 0..dels.len().max(adds.len()) {
                        out.push(Row::Pair(hi, dels.get(k).copied(), adds.get(k).copied()));
                    }
                }
            }
        }
    }
    out
}

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(sel) = app.session.sel.file.clone() else { return };
    let mut run: Option<(&'static str, Value)> = None;
    let mut dialog: Option<Dialog> = None;
    header(app, ui, &sel, &mut run, &mut dialog);
    if app.session.view.blame {
        blame(app, ui, &sel, &mut run);
    } else {
        match app.session.selected_diff() {
            None => {}
            Some(Err(e)) => {
                ui.label(RichText::new(e.to_string()).color(t.danger));
            }
            Some(Ok(d)) => body(app, ui, &sel, &d, &mut run, &mut dialog),
        }
    }
    if let Some(d) = dialog {
        app.dialog = Some(d);
    }
    if let Some((id, p)) = run {
        let _ = app.run(id, p);
    }
}

fn header(app: &mut GitApp, ui: &mut egui::Ui, sel: &FileSel, run: &mut Option<(&'static str, Value)>, dialog: &mut Option<Dialog>) {
    let t = Tokens::get(ui.ctx());
    Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 5)).stroke(Stroke::new(1.0, t.border)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), r, Icon::File, t.text, t.accent);
            ui.label(RichText::new(&sel.path).strong());
            let chip = match &sel.area {
                Area::Unstaged => "Unstaged".to_string(),
                Area::Staged => "Staged".to_string(),
                Area::Commit(o) => format!("commit {}", o.get(..7).unwrap_or(o)),
            };
            ui.label(RichText::new(chip).small().color(t.dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::Close, "Close (Esc)", None, 14.0).clicked() {
                    *run = Some(("file.close", json!({})));
                }
                match sel.area {
                    Area::Unstaged => {
                        if ui.button("Stage File").clicked() {
                            *run = Some(("stage.file", json!({"path": sel.path})));
                        }
                        if ui.button("Discard…").clicked() {
                            *dialog = Some(Dialog::confirm(
                                &format!("Discard changes to {}?", sel.path),
                                "This can't be undone.",
                                "discard.file",
                                json!({"path": sel.path}),
                                "Discard",
                            ));
                        }
                    }
                    Area::Staged => {
                        if ui.button("Unstage File").clicked() {
                            *run = Some(("unstage.file", json!({"path": sel.path})));
                        }
                    }
                    Area::Commit(_) => {}
                }
                ui.separator();
                if icons::button(ui, Icon::History, "File history", app.disabled("view.history"), 15.0).clicked() {
                    *run = Some(("view.history", json!({"path": sel.path})));
                }
                let blame_on = app.session.view.blame;
                if ui.selectable_label(blame_on, "Blame").clicked() {
                    *run = Some(("view.blame", json!({"value": !blame_on})));
                }
                ui.separator();
                for m in DiffMode::ALL.iter().rev() {
                    if ui.selectable_label(app.session.view.diff_mode == *m && !blame_on, m.label()).clicked() {
                        *run = Some(("view.diffMode", json!({"value": m.code()})));
                        app.pick.lines.clear();
                    }
                }
            });
        });
    });
}

fn line_colors(kind: LineKind, t: Tokens) -> (Color32, Color32) {
    match kind {
        LineKind::Add => (t.added_bg, t.added),
        LineKind::Del => (t.removed_bg, t.removed),
        _ => (Color32::TRANSPARENT, t.dim),
    }
}

fn display(text: &str) -> String {
    text.trim_end_matches('\r').replace('\t', "    ")
}

fn body(app: &mut GitApp, ui: &mut egui::Ui, sel: &FileSel, d: &FileDiff, run: &mut Option<(&'static str, Value)>, dialog: &mut Option<Dialog>) {
    let t = Tokens::get(ui.ctx());
    if d.binary {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Binary file: no text changes to show").weak()));
        return;
    }
    if d.hunks.is_empty() {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No text changes to this file here (a rename, a mode change, or not part of this change)").weak())
        });
        return;
    }
    if d.truncated {
        Frame::NONE.fill(t.warning_bg).inner_margin(6).show(ui, |ui| {
            ui.label(
                RichText::new("This diff is very long: only the first lines are shown, and its hunks are staged with the whole file.")
                    .color(t.warning)
                    .small(),
            );
        });
    }
    let split = app.session.view.diff_mode == DiffMode::Split;
    let rows = rows_for(d, split);
    let longest = d.hunks.iter().flat_map(|h| &h.lines).map(|l| l.text.chars().count()).max().unwrap_or(0).min(2000);
    let editable = !matches!(sel.area, Area::Commit(_)) && !d.truncated;
    let ctx_used = context_used(app);
    let mono = FontId::monospace(12.5);
    let width = ui.available_width();
    let content_w = if split { width } else { (GUTTER * 2.0 + 20.0 + longest as f32 * CHAR_W + 20.0).max(width) };
    let mut toggle: Option<(usize, usize)> = None;
    egui::ScrollArea::both().id_salt(("diff", &sel.path)).auto_shrink([false, false]).show_rows(ui, ROW_H, rows.len(), |ui, range| {
        for row in rows.get(range).unwrap_or(&[]) {
            match *row {
                Row::Hunk(hi) => {
                    let Some(h) = d.hunks.get(hi) else { continue };
                    let (rect, _) = ui.allocate_exact_size(vec2(content_w, ROW_H + 6.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, t.hunk_bg);
                    let vis = ui.clip_rect();
                    p.text(
                        pos2(vis.left().max(rect.left()) + 8.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        h.header(),
                        FontId::monospace(11.5),
                        t.dim,
                    );
                    if !editable {
                        continue;
                    }
                    let picked = if app.pick.path == sel.path && app.pick.hunk == hi { app.pick.lines.len() } else { 0 };
                    // Buttons pinned to the visible right edge.
                    let mut x = vis.right().min(rect.right()) - 6.0;
                    let mut btn = |ui: &mut egui::Ui, label: &str| -> bool {
                        let w = 9.0 + label.len() as f32 * 6.6;
                        x -= w;
                        let b = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(w, ROW_H));
                        x -= 6.0;
                        ui.put(b, egui::Button::new(RichText::new(label).small())).clicked()
                    };
                    let lines: Vec<usize> = if picked > 0 { app.pick.lines.iter().copied().collect() } else { Vec::new() };
                    let what = |base: &str| {
                        if picked > 0 { format!("{base} {picked} Line{}", if picked == 1 { "" } else { "s" }) } else { format!("{base} Hunk") }
                    };
                    match sel.area {
                        Area::Unstaged => {
                            if btn(ui, &what("Stage")) {
                                *run = Some(if picked > 0 {
                                    ("stage.lines", json!({"path": sel.path, "hunk": hi, "lines": lines, "context": ctx_used}))
                                } else {
                                    ("stage.hunk", json!({"path": sel.path, "hunk": hi, "context": ctx_used}))
                                });
                            }
                            if btn(ui, &what("Discard")) {
                                let (id, params) = if picked > 0 {
                                    ("discard.lines", json!({"path": sel.path, "hunk": hi, "lines": lines, "context": ctx_used}))
                                } else {
                                    ("discard.hunk", json!({"path": sel.path, "hunk": hi, "context": ctx_used}))
                                };
                                *dialog = Some(Dialog::confirm(
                                    "Discard these changes?",
                                    "They are removed from the file. This can't be undone.",
                                    id,
                                    params,
                                    "Discard",
                                ));
                            }
                        }
                        Area::Staged => {
                            if btn(ui, &what("Unstage")) {
                                *run = Some(if picked > 0 {
                                    ("unstage.lines", json!({"path": sel.path, "hunk": hi, "lines": lines, "context": ctx_used}))
                                } else {
                                    ("unstage.hunk", json!({"path": sel.path, "hunk": hi, "context": ctx_used}))
                                });
                            }
                        }
                        Area::Commit(_) => {}
                    }
                }
                Row::Line(hi, li) => {
                    let Some(l) = d.hunks.get(hi).and_then(|h| h.lines.get(li)) else { continue };
                    let (rect, resp) = ui.allocate_exact_size(vec2(content_w, ROW_H), Sense::click());
                    let p = ui.painter();
                    let (bg, fg) = line_colors(l.kind, t);
                    let picked = app.pick.path == sel.path && app.pick.hunk == hi && app.pick.lines.contains(&li);
                    p.rect_filled(rect, 0.0, if picked { t.picked_bg } else { bg });
                    if resp.hovered() && editable && matches!(l.kind, LineKind::Add | LineKind::Del) {
                        p.rect_stroke(rect, 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                    }
                    let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
                    p.text(
                        pos2(rect.left() + GUTTER - 6.0, rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        num(l.old_no),
                        FontId::monospace(11.0),
                        t.dim,
                    );
                    p.text(
                        pos2(rect.left() + GUTTER * 2.0 - 6.0, rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        num(l.new_no),
                        FontId::monospace(11.0),
                        t.dim,
                    );
                    let mark = match l.kind {
                        LineKind::Add => "+",
                        LineKind::Del => "-",
                        LineKind::Context => " ",
                        LineKind::NoNewline => "\\",
                    };
                    p.text(pos2(rect.left() + GUTTER * 2.0 + 4.0, rect.center().y), egui::Align2::LEFT_CENTER, mark, mono.clone(), fg);
                    let text_c = if l.kind == LineKind::NoNewline { t.dim } else { t.text };
                    p.text(
                        pos2(rect.left() + GUTTER * 2.0 + 18.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        display(&l.text),
                        mono.clone(),
                        text_c,
                    );
                    if resp.clicked() && editable && matches!(l.kind, LineKind::Add | LineKind::Del) {
                        toggle = Some((hi, li));
                    }
                }
                Row::Pair(hi, left, right) => {
                    let Some(h) = d.hunks.get(hi) else { continue };
                    let (rect, _) = ui.allocate_exact_size(vec2(content_w, ROW_H), Sense::hover());
                    let half = rect.width() / 2.0;
                    for (side, idx) in [(0, left), (1, right)] {
                        let r = Rect::from_min_size(pos2(rect.left() + half * side as f32, rect.top()), vec2(half, ROW_H));
                        let p = ui.painter().with_clip_rect(r.intersect(ui.clip_rect()));
                        let Some(l) = idx.and_then(|i| h.lines.get(i)) else {
                            p.rect_filled(r, 0.0, t.panel);
                            continue;
                        };
                        let kind = if l.kind == LineKind::Context { LineKind::Context } else { l.kind };
                        let (bg, _) = line_colors(kind, t);
                        p.rect_filled(r, 0.0, bg);
                        let n = if side == 0 { l.old_no } else { l.new_no };
                        p.text(
                            pos2(r.left() + GUTTER - 6.0, r.center().y),
                            egui::Align2::RIGHT_CENTER,
                            n.map(|n| n.to_string()).unwrap_or_default(),
                            FontId::monospace(11.0),
                            t.dim,
                        );
                        p.text(pos2(r.left() + GUTTER + 6.0, r.center().y), egui::Align2::LEFT_CENTER, display(&l.text), mono.clone(), t.text);
                    }
                    ui.painter()
                        .line_segment([pos2(rect.left() + half, rect.top()), pos2(rect.left() + half, rect.bottom())], Stroke::new(1.0, t.border));
                }
            }
        }
    });
    if let Some((hi, li)) = toggle {
        if app.pick.path != sel.path || app.pick.hunk != hi {
            app.pick = crate::LinePick { path: sel.path.clone(), hunk: hi, lines: Default::default() };
        }
        if !app.pick.lines.remove(&li) {
            app.pick.lines.insert(li);
        }
    }
}

fn blame(app: &mut GitApp, ui: &mut egui::Ui, sel: &FileSel, run: &mut Option<(&'static str, Value)>) {
    let t = Tokens::get(ui.ctx());
    let key = (format!("{:?}{}", sel.area, sel.path), app.session.generation());
    if app.blame.as_ref().map(|(k, _)| k) != Some(&key) {
        let rev = match &sel.area {
            Area::Commit(o) => Some(o.as_str()),
            _ => None,
        };
        let r = match app.session.repo() {
            Ok(repo) => repo.blame(&sel.path, rev).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        app.blame = Some((key, r));
    }
    let Some((_, result)) = &app.blame else { return };
    let b = match result {
        Ok(b) => b,
        Err(e) => {
            ui.label(RichText::new(e).color(t.danger));
            return;
        }
    };
    let now = util::now_s();
    let mono = FontId::monospace(12.5);
    let meta_w = 300.0;
    let width = ui.available_width();
    // Alternate bands per run of lines from one commit.
    let mut bands = Vec::with_capacity(b.lines.len());
    let mut on = false;
    for (i, l) in b.lines.iter().enumerate() {
        if i > 0 && b.lines.get(i - 1).is_some_and(|x| x.oid != l.oid) {
            on = !on;
        }
        bands.push(on);
    }
    egui::ScrollArea::both().id_salt(("blame", &sel.path)).auto_shrink([false, false]).show_rows(ui, ROW_H, b.lines.len(), |ui, range| {
        for i in range {
            let Some(l) = b.lines.get(i) else { continue };
            let (rect, resp) = ui.allocate_exact_size(vec2(width.max(meta_w + 600.0), ROW_H), Sense::click());
            let p = ui.painter();
            let first = i == 0 || b.lines.get(i - 1).is_some_and(|x| x.oid != l.oid);
            if bands.get(i).copied().unwrap_or(false) {
                p.rect_filled(Rect::from_min_size(rect.min, vec2(meta_w, ROW_H)), 0.0, t.panel);
            }
            if first && let Some(c) = b.commits.get(&l.oid) {
                let short = l.oid.get(..7).unwrap_or(&l.oid);
                p.text(pos2(rect.left() + 6.0, rect.center().y), egui::Align2::LEFT_CENTER, short, FontId::monospace(11.0), t.accent);
                let who = format!("{} · {}", c.author, util::relative_time(c.time, now));
                p.with_clip_rect(Rect::from_min_size(pos2(rect.left() + 62.0, rect.top()), vec2(meta_w - 66.0, ROW_H))).text(
                    pos2(rect.left() + 62.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    who,
                    FontId::proportional(11.5),
                    t.dim,
                );
                if resp.clicked() {
                    *run = Some(("graph.select", json!({"oid": l.oid})));
                }
                resp.on_hover_text(format!("{}\n{} <{}>", c.summary, c.author, c.email));
            }
            p.text(
                pos2(rect.left() + meta_w + 40.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                l.line.to_string(),
                FontId::monospace(11.0),
                t.dim,
            );
            p.text(pos2(rect.left() + meta_w + 50.0, rect.center().y), egui::Align2::LEFT_CENTER, display(&l.text), mono.clone(), t.text);
        }
    });
}
