//! The commit graph: one row per commit (and the work-in-progress row on top) with the branch and
//! tag labels, the lanes, the message, author, date and id. Rows are virtualised: only the visible
//! ones are laid out, so long histories scroll smoothly.

use egui::epaint::CubicBezierShape;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use gitcraft_engine::git::{Commit, RefKind};
use gitcraft_engine::graph::Seg;
use gitcraft_engine::{Columns, RepoData, Selected};
use serde_json::json;

use crate::avatars::Avatars;
use crate::icons::{self, Icon};
use crate::theme::{Tokens, lane_color};
use crate::{Dialog, GitApp, Reword, util};

pub const ROW_H: f32 = 28.0;
const LANE_W: f32 = 16.0;
const REFS_W: f32 = 180.0;
const AUTHOR_W: f32 = 150.0;
const DATE_W: f32 = 110.0;
const SHA_W: f32 = 70.0;
/// Lanes drawn at most; wider graphs are cut at the column's edge.
const MAX_GRAPH_W: f32 = 360.0;

/// A label on a row: a branch (local, its remote twin, or both), a remote branch, or a tag.
struct Label {
    text: String,
    local: bool,
    remote: bool,
    tag: bool,
    head: bool,
    /// The ref to act on (a local branch's name, else the full remote name or tag).
    target: usize,
}

fn labels(d: &RepoData, oid: &str) -> Vec<Label> {
    let Some(ix) = d.labels.get(oid) else { return Vec::new() };
    let refs: Vec<(usize, &gitcraft_engine::git::Ref)> = ix.iter().filter_map(|&i| d.refs.get(i).map(|r| (i, r))).collect();
    let mut out: Vec<Label> = Vec::new();
    for (i, r) in refs.iter().filter(|(_, r)| r.kind == RefKind::Local) {
        let remote = refs.iter().any(|(_, x)| x.kind == RefKind::Remote && x.remote_branch() == Some(r.name.as_str()));
        out.push(Label { text: r.name.clone(), local: true, remote, tag: false, head: r.head, target: *i });
    }
    for (i, r) in refs.iter().filter(|(_, r)| r.kind == RefKind::Remote) {
        let twin = out.iter().any(|l| l.local && Some(l.text.as_str()) == r.remote_branch());
        if !twin {
            out.push(Label { text: r.name.clone(), local: false, remote: true, tag: false, head: false, target: *i });
        }
    }
    for (i, r) in refs.iter().filter(|(_, r)| r.kind == RefKind::Tag) {
        out.push(Label { text: r.name.clone(), local: false, remote: false, tag: true, head: false, target: *i });
    }
    // The checked-out branch first.
    out.sort_by_key(|l| (!l.head, !l.local, l.tag));
    out
}

struct Cols {
    refs: Rect,
    graph: Rect,
    msg: Rect,
    author: Rect,
    date: Rect,
    sha: Rect,
}

fn columns(row: Rect, graph_w: f32, shown: Columns) -> Cols {
    let mut x = row.left();
    let mut take = |w: f32| {
        let r = Rect::from_min_size(pos2(x, row.top()), vec2(w, row.height()));
        x += w;
        r
    };
    let w = |on: bool, w: f32| if on { w } else { 0.0 };
    let (refs_w, author_w, date_w, sha_w) = (w(shown.refs, REFS_W), w(shown.author, AUTHOR_W), w(shown.date, DATE_W), w(shown.sha, SHA_W));
    let refs = take(refs_w);
    let graph = take(graph_w);
    let msg_w = (row.width() - refs_w - graph_w - author_w - date_w - sha_w).max(120.0);
    let msg = take(msg_w);
    let author = take(author_w);
    let date = take(date_w);
    let sha = take(sha_w);
    Cols { refs, graph, msg, author, date, sha }
}

fn lane_x(graph: Rect, lane: usize) -> f32 {
    graph.left() + 10.0 + lane as f32 * LANE_W + LANE_W / 2.0
}

fn text_in(p: &egui::Painter, r: Rect, s: &str, font: FontId, c: Color32) {
    let g = p.layout_no_wrap(s.to_string(), font, c);
    p.with_clip_rect(r.shrink2(vec2(4.0, 0.0))).galley(pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, c);
}

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let d = &app.session.data;
    let wip = !d.status.is_clean() || d.commits.is_empty();
    let lane_count = d.graph.width.max(1);
    let graph_w = (20.0 + lane_count as f32 * LANE_W).clamp(48.0, MAX_GRAPH_W);
    let n = d.commits.len() + usize::from(wip) + usize::from(d.more);

    // Header.
    let shown = app.session.view.columns;
    let (hdr, hdr_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    ui.painter().rect_filled(hdr, 0.0, t.chrome);
    let c = columns(hdr, graph_w, shown);
    let hf = FontId::proportional(11.0);
    let title = if let Some(p) = &app.session.view.history { format!("HISTORY OF {p}") } else { "COMMIT MESSAGE".into() };
    for (r, s) in [(c.refs, "BRANCH / TAG"), (c.graph, "GRAPH"), (c.msg, title.as_str()), (c.author, "AUTHOR"), (c.date, "DATE"), (c.sha, "COMMIT")] {
        text_in(ui.painter(), r, s, hf.clone(), t.dim);
    }
    if app.session.view.history.is_some() {
        let b = Rect::from_min_size(pos2(c.msg.right() - 120.0, hdr.top() + 2.0), vec2(116.0, 20.0));
        if ui.put(b, egui::Button::new(egui::RichText::new("Show all commits").small())).clicked() {
            let _ = app.run("view.historyClear", json!({}));
        }
    } else if app.disabled("graph.showAll").is_none() {
        let v = &app.session.view;
        let what = if !v.solo.is_empty() {
            format!(
                "Solo: {}",
                v.solo
                    .iter()
                    .map(|f| ["refs/heads/", "refs/remotes/", "refs/tags/"].iter().find_map(|p| f.strip_prefix(p)).unwrap_or(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if !v.hidden.is_empty() {
            format!("{} hidden", v.hidden.len())
        } else {
            "Filtered".to_string()
        };
        let b = Rect::from_min_size(pos2(c.msg.right() - 250.0, hdr.top() + 2.0), vec2(246.0, 20.0));
        if ui
            .put(b, egui::Button::new(egui::RichText::new(format!("{what} · Show all")).small().color(t.accent)))
            .on_hover_text("Show every branch again")
            .clicked()
        {
            let _ = app.run("graph.showAll", json!({}));
        }
    }
    // Right-click the header: which columns and refs show.
    hdr_resp.context_menu(|ui| {
        let v = app.session.view.clone();
        ui.label(egui::RichText::new("Columns").small().weak());
        for (k, on, label) in [
            ("refs", v.columns.refs, "Branch / Tag"),
            ("author", v.columns.author, "Author"),
            ("date", v.columns.date, "Date"),
            ("sha", v.columns.sha, "Commit ID"),
        ] {
            let mut x = on;
            if ui.checkbox(&mut x, label).changed() {
                let _ = app.run("graph.columns", json!({ k: x }));
            }
        }
        ui.separator();
        let mut r = v.show_remotes;
        if ui.checkbox(&mut r, "Remote branches").changed() {
            let _ = app.run("graph.refs", json!({"remotes": r}));
        }
        let mut tg = v.show_tags;
        if ui.checkbox(&mut tg, "Tags").changed() {
            let _ = app.run("graph.refs", json!({"tags": tg}));
        }
    });

    let selected_row = match &app.session.sel.row {
        Selected::Wip => Some(0),
        Selected::Commit(o) => app.session.data.row_of.get(o).map(|&i| i + usize::from(wip)),
        Selected::None => None,
    };
    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("graph_scroll");
    let view_h = ui.available_height();
    let offset_key = egui::Id::new("graph_offset");
    let jump_key = egui::Id::new("graph_jump");
    if let Some(y) = ui.ctx().data_mut(|m| m.remove_temp::<f32>(jump_key)) {
        area = area.vertical_scroll_offset(y.max(0.0));
    }
    let list_rect = ui.available_rect_before_wrap();
    if app.scroll_to_selection {
        app.scroll_to_selection = false;
        if let Some(r) = selected_row {
            let cur: f32 = ui.ctx().data(|m| m.get_temp(offset_key)).unwrap_or(0.0);
            let y = r as f32 * ROW_H;
            if y < cur || y + ROW_H > cur + view_h {
                area = area.vertical_scroll_offset((y - view_h / 3.0).max(0.0));
            }
        }
    }
    let search = app.session.view.search.to_lowercase();
    let mut actions: Vec<Action> = Vec::new();
    let dragging = egui::DragAndDrop::payload::<DragRef>(ui.ctx());
    // Rows touch: any vertical spacing would break every lane line into dashes.
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let out = area.show_rows(ui, ROW_H, n, |ui, range| {
        let now = util::now_s();
        let d = &app.session.data;
        let head_row = d.status.head.as_ref().and_then(|h| d.row_of.get(h)).copied();
        let head_lane = head_row.and_then(|r| d.graph.rows.get(r)).map(|r| r.lane).unwrap_or(0);
        for i in range {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            let cols = columns(rect, graph_w, shown);
            let p = ui.painter().clone();
            let selected = selected_row == Some(i);
            if selected {
                p.rect_filled(rect, 0.0, t.selection);
            } else if resp.hovered() {
                p.rect_filled(rect, 0.0, t.hover);
            }
            if wip && i == 0 {
                wip_row(app, ui, rect, &cols, head_lane, head_row.is_some(), &resp, &mut actions);
                continue;
            }
            let ci = i - usize::from(wip);
            let Some(commit) = d.commits.get(ci) else {
                // The last row: load more.
                if ui.put(cols.msg.shrink(3.0), egui::Button::new("Load more commits")).clicked() {
                    actions.push(Action::Run("graph.loadMore", json!({})));
                }
                continue;
            };
            let mut over_node = false;
            let dim = !search.is_empty() && !gitcraft_engine::cmd::view::matches(commit, &search);
            let fade = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
            // Lanes.
            if let Some(row) = d.graph.rows.get(ci) {
                let gp = p.with_clip_rect(cols.graph);
                // The line from the WIP row down to HEAD.
                if wip && let Some(h) = head_row {
                    let x = lane_x(cols.graph, head_lane);
                    let (y0, y1) = if ci < h {
                        (rect.top(), rect.bottom())
                    } else if ci == h {
                        (rect.top(), rect.center().y)
                    } else {
                        (0.0, 0.0)
                    };
                    if y1 > y0 {
                        gp.line_segment([pos2(x, y0), pos2(x, y1)], Stroke::new(1.5, t.dim));
                    }
                }
                lanes(&gp, cols.graph, rect, &row.up, &row.down, &fade);
                let at = pos2(lane_x(cols.graph, row.lane), rect.center().y);
                node(
                    &gp,
                    &app.avatars,
                    at,
                    commit,
                    fade(lane_color(row.color)),
                    d.status.head.as_deref() == Some(commit.oid.as_str()),
                    fade(Color32::WHITE),
                    t,
                );
                if !commit.is_merge() && resp.hover_pos().is_some_and(|h| h.distance(at) <= NODE_R + 2.0) {
                    over_node = true;
                }
                // Labels.
                let ls = labels(d, &commit.oid);
                if !ls.is_empty() {
                    if let Some(pr) = pills(ui, cols.refs, &ls, fade(lane_color(row.color))) {
                        pill_events(app, &pr, &ls, commit, &mut actions);
                    }
                    // A line from the label to the commit.
                    let y = rect.center().y;
                    p.line_segment(
                        [pos2(cols.refs.right() - 2.0, y), pos2(lane_x(cols.graph, row.lane) - 8.0, y)],
                        Stroke::new(1.0, fade(lane_color(row.color))),
                    );
                }
            }
            if let Some(dr) = &dragging
                && ui.rect_contains_pointer(rect)
            {
                let target = labels(d, &commit.oid).into_iter().find(|l| l.local && l.text != dr.name);
                if let Some(target) = target {
                    p.rect_stroke(rect.shrink(1.0), 4.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                    if ui.input(|i| i.pointer.any_released()) {
                        let at = ui.input(|i| i.pointer.interact_pos()).unwrap_or(rect.center());
                        actions.push(Action::DropMenu(dr.name.clone(), target.text, at));
                    }
                } else {
                    p.rect_filled(rect, 0.0, t.hover);
                }
            }
            let msg_color = fade(t.text);
            text_in(&p, cols.msg, &commit.summary, FontId::proportional(13.0), msg_color);
            author_cell(&p, &app.avatars, cols.author, commit, &fade, t);
            text_in(&p, cols.date, &util::relative_time(commit.author.time, now), FontId::proportional(12.0), fade(t.dim));
            text_in(&p, cols.sha, commit.short(), FontId::monospace(11.5), fade(t.dim));
            if app.picked_commits.contains(&commit.oid) {
                p.rect_stroke(rect.shrink(1.0), 3.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            }
            if resp.clicked() && ui.input(|i| i.modifiers.shift) {
                // Shift-click picks several commits (to cherry-pick them together).
                actions.push(Action::Pick(commit.oid.clone()));
            } else if resp.clicked() {
                let compare_with = ui.input(|i| i.modifiers.command).then(|| match &app.session.sel.row {
                    Selected::Commit(o) if *o != commit.oid => Some(o.clone()),
                    _ => None,
                });
                match compare_with.flatten() {
                    // Mod-click a second commit: compare it with the selected one.
                    Some(from) => actions.push(Action::Run("diff.compare", json!({"from": from, "to": commit.oid}))),
                    None => actions.push(Action::Run("graph.select", json!({"oid": commit.oid}))),
                }
            }
            if resp.double_clicked()
                && let Some(t) = checkout_target(&labels(d, &commit.oid))
            {
                actions.push(t.action());
            }
            let resp = if over_node {
                resp.on_hover_ui_at_pointer(|ui| person_card(ui, &app.avatars, commit))
            } else {
                resp.on_hover_text_at_pointer(format!(
                    "{}\n{} <{}>\n{}",
                    commit.message(),
                    commit.author.name,
                    commit.author.email,
                    util::date_time(commit.author.time)
                ))
            };
            resp.context_menu(|ui| commit_menu(app, ui, commit, &mut actions));
        }
    });
    ui.spacing_mut().item_spacing.y = spacing;
    ui.ctx().data_mut(|m| m.insert_temp(offset_key, out.state.offset.y));
    minimap(app, ui, list_rect, n, wip, out.state.offset.y, view_h, jump_key);
    if let Some(dr) = &dragging {
        drag_ghost(ui.ctx(), &dr.name);
    }
    for a in actions {
        a.apply(app);
    }
    drop_menu(app, ui.ctx());
    label_popup(app, ui.ctx());
}

/// Marks on the overview strip: (row, colour kind) — 0 branch tip, 1 search hit, 2 HEAD.
pub type MinimapMarks = Vec<(usize, u8)>;

/// A thin overview of the whole loaded history at the graph's right edge: branch tips, search
/// hits, HEAD and the selection, with the visible window; a click jumps there.
fn minimap(app: &mut GitApp, ui: &mut egui::Ui, area: Rect, rows: usize, wip: bool, offset: f32, view_h: f32, jump_key: egui::Id) {
    if rows < 2 || (rows as f32) * ROW_H <= view_h {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let key = (app.session.generation(), app.session.view.search.clone());
    if app.minimap.as_ref().map(|(k, _)| k) != Some(&key) {
        let d = &app.session.data;
        let q = app.session.view.search.to_lowercase();
        let mut marks: MinimapMarks = Vec::new();
        for (i, c) in d.commits.iter().enumerate() {
            let row = i + usize::from(wip);
            if d.status.head.as_deref() == Some(c.oid.as_str()) {
                marks.push((row, 2));
            } else if !q.is_empty() && gitcraft_engine::cmd::view::matches(c, &q) {
                marks.push((row, 1));
            } else if d.labels.get(&c.oid).is_some_and(|ix| ix.iter().any(|&j| d.refs.get(j).is_some_and(|r| r.kind == RefKind::Local))) {
                marks.push((row, 0));
            }
        }
        app.minimap = Some((key, marks));
    }
    let strip = Rect::from_min_max(pos2(area.right() - 22.0, area.top() + 2.0), pos2(area.right() - 14.0, area.bottom() - 2.0));
    let p = ui.painter();
    p.rect_filled(strip, 3.0, t.chrome.gamma_multiply(0.8));
    let y_of = |row: usize| strip.top() + strip.height() * (row as f32 + 0.5) / rows as f32;
    if let Some((_, marks)) = &app.minimap {
        for &(row, kind) in marks {
            let c = match kind {
                2 => t.accent,
                1 => t.warning,
                _ => t.dim,
            };
            let y = y_of(row);
            p.line_segment([pos2(strip.left() + 1.0, y), pos2(strip.right() - 1.0, y)], Stroke::new(if kind == 0 { 1.0 } else { 2.0 }, c));
        }
    }
    let selected = match &app.session.sel.row {
        Selected::Wip => Some(0),
        Selected::Commit(o) => app.session.data.row_of.get(o).map(|&i| i + usize::from(wip)),
        Selected::None => None,
    };
    if let Some(r) = selected {
        p.circle_filled(pos2(strip.center().x, y_of(r)), 2.5, t.text);
    }
    let total = rows as f32 * ROW_H;
    let win = Rect::from_min_max(
        pos2(strip.left() - 1.0, strip.top() + strip.height() * (offset / total).clamp(0.0, 1.0)),
        pos2(strip.right() + 1.0, strip.top() + strip.height() * ((offset + view_h) / total).clamp(0.0, 1.0)),
    );
    p.rect_stroke(win, 2.0, Stroke::new(1.0, t.accent.gamma_multiply(0.7)), egui::StrokeKind::Outside);
    let resp =
        ui.interact(strip.expand2(vec2(3.0, 0.0)), egui::Id::new("graph_minimap"), Sense::click_and_drag()).on_hover_text("Overview: click to jump");
    if (resp.clicked() || resp.dragged())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let frac = ((pos.y - strip.top()) / strip.height()).clamp(0.0, 1.0);
        ui.ctx().data_mut(|m| m.insert_temp(jump_key, frac * total - view_h / 2.0));
        ui.ctx().request_repaint();
    }
}

/// The list of every label on a commit, open while the pointer is on its pill or the list.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelPopup {
    pub oid: String,
    pub pill: Rect,
}

/// Every branch and tag on the hovered pill's commit; a click checks one out.
fn label_popup(app: &mut GitApp, ctx: &egui::Context) {
    let Some(pop) = app.label_popup.clone() else { return };
    let ls = labels(&app.session.data, &pop.oid);
    if ls.len() < 2 || app.drop_menu.is_some() {
        app.label_popup = None;
        return;
    }
    let t = Tokens::get(ctx);
    let mut run: Option<(&'static str, serde_json::Value)> = None;
    let area = egui::Area::new(egui::Id::new("label_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(pop.pill.left_bottom() + vec2(0.0, 2.0))
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                let w = 230.0;
                ui.set_width(w);
                ui.label(egui::RichText::new(format!("{} refs on this commit", ls.len())).small().weak());
                for l in &ls {
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
                    let p = ui.painter();
                    if resp.hovered() && !l.head {
                        p.rect_filled(r, 4.0, t.hover);
                    }
                    let mut x = r.left() + 4.0;
                    for (on, icon) in [(l.local, Icon::Laptop), (l.remote, Icon::Cloud), (l.tag, Icon::Tag)] {
                        if on {
                            icons::paint(p, Rect::from_center_size(pos2(x + 7.0, r.center().y), vec2(13.0, 13.0)), icon, t.dim, t.accent);
                            x += 16.0;
                        }
                    }
                    let font = FontId::proportional(12.5);
                    let color = if l.head { t.accent } else { t.text };
                    let right = if l.head { r.right() - 82.0 } else { r.right() - 4.0 };
                    let clip = Rect::from_min_max(pos2(x + 4.0, r.top()), pos2(right, r.bottom()));
                    let g = p.layout_no_wrap(l.text.clone(), font, color);
                    p.with_clip_rect(clip).galley(pos2(clip.left(), r.center().y - g.size().y / 2.0), g, color);
                    if l.head {
                        p.text(r.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, "checked out", FontId::proportional(11.0), t.dim);
                    } else if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let resp = resp.on_hover_text(if l.tag && !l.local && !l.remote {
                        format!("Check out {} (detached)", l.text)
                    } else {
                        format!("Check out {}", l.text)
                    });
                    if resp.clicked() && !l.head {
                        run = Some(if l.remote && !l.local {
                            ("branch.pullRemote", json!({"name": l.text}))
                        } else if l.local {
                            ("branch.checkout", json!({"name": l.text}))
                        } else {
                            ("commit.checkout", json!({"oid": pop.oid}))
                        });
                    }
                }
            });
        });
    // Open while the pointer is on the pill, the list, or the gap between them.
    let list = area.response.rect;
    let bridge = Rect::from_min_max(pos2(pop.pill.left().min(list.left()), pop.pill.top()), pos2(pop.pill.right().max(list.right()), list.top()));
    let inside = ctx.pointer_hover_pos().is_some_and(|p| pop.pill.expand(3.0).contains(p) || list.expand(6.0).contains(p) || bridge.contains(p));
    if let Some((id, params)) = run {
        app.label_popup = None;
        let _ = app.run(id, params);
    } else if !inside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.label_popup = None;
    }
}

/// A branch, remote branch or tag being dragged from a label.
#[derive(Clone, Debug, PartialEq)]
pub struct DragRef {
    pub name: String,
}

/// What can be done by dropping `from` onto local branch `onto`.
#[derive(Clone, Debug, PartialEq)]
pub struct DropMenu {
    pub from: String,
    pub onto: String,
    pub at: Pos2,
    /// (label, `branch.dropOnto` action, hover note).
    pub options: Vec<(String, &'static str, Option<String>)>,
}

impl DropMenu {
    pub fn new(app: &GitApp, from: String, onto: String, at: Pos2) -> DropMenu {
        let d = &app.session.data;
        let current = d.status.branch.as_deref();
        let from_local = d.local_branches().any(|r| r.name == from);
        let mut options = Vec::new();
        if let Some(repo) = &app.session.repo {
            let (f, o) = (repo.resolve(&from), repo.resolve(&format!("refs/heads/{onto}")));
            if let (Some(f), Some(o)) = (f, o)
                && f != o
                && repo.is_ancestor(&o, &f)
            {
                options.push((format!("Fast-forward {onto} to {from}"), "fastForward", None));
            }
        }
        let first = |b: &str| (current != Some(b)).then(|| format!("Checks out {b} first"));
        options.push((format!("Merge {from} into {onto}"), "merge", first(&onto)));
        if from_local {
            options.push((format!("Rebase {from} onto {onto}"), "rebase", first(&from)));
        }
        DropMenu { from, onto, at, options }
    }
}

/// The dragged label following the pointer.
fn drag_ghost(ctx: &egui::Context, name: &str) {
    let Some(pos) = ctx.pointer_hover_pos() else { return };
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    let t = Tokens::get(ctx);
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_ref_ghost")));
    let g = p.layout_no_wrap(name.to_string(), FontId::proportional(11.5), Color32::WHITE);
    let r = Rect::from_min_size(pos + vec2(14.0, 6.0), g.size() + vec2(16.0, 8.0));
    p.rect_filled(r, 4.0, t.accent.gamma_multiply(0.9));
    p.galley(r.min + vec2(8.0, 4.0), g, Color32::WHITE);
}

/// The menu shown where a label was dropped on another branch.
fn drop_menu(app: &mut GitApp, ctx: &egui::Context) {
    let Some(m) = app.drop_menu.clone() else { return };
    let mut pick: Option<&'static str> = None;
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let area = egui::Area::new(egui::Id::new("drop_menu")).order(egui::Order::Foreground).fixed_pos(m.at).constrain(true).show(ctx, |ui| {
        egui::Frame::menu(ui.style()).show(ui, |ui| {
            let w = 240.0;
            ui.set_width(w);
            ui.label(egui::RichText::new(format!("{} dropped on {}", m.from, m.onto)).small().weak());
            ui.separator();
            for (label, action, note) in &m.options {
                let b = ui.add(egui::Button::new(label.as_str()).frame_when_inactive(false).min_size(vec2(w, 0.0)));
                let b = match note {
                    Some(n) => b.on_hover_text(n.as_str()),
                    None => b,
                };
                if b.clicked() {
                    pick = Some(action);
                }
            }
            ui.separator();
            if ui.add(egui::Button::new("Cancel").frame_when_inactive(false).min_size(vec2(w, 0.0))).clicked() {
                close = true;
            }
        });
    });
    if area.response.clicked_elsewhere() {
        close = true;
    }
    if let Some(action) = pick {
        app.drop_menu = None;
        let _ = app.run("branch.dropOnto", json!({"from": m.from, "onto": m.onto, "action": action}));
    } else if close {
        app.drop_menu = None;
    }
}

/// What double-clicking a row checks out.
#[derive(Debug, PartialEq)]
enum Target {
    /// A local branch that isn't checked out.
    Local(String),
    /// A remote branch: fetched, then its local branch tracks it or is pulled up to it.
    Remote(String),
}

impl Target {
    fn action(self) -> Action {
        match self {
            Target::Local(n) => Action::Run("branch.checkout", json!({"name": n})),
            Target::Remote(n) => Action::Run("branch.pullRemote", json!({"name": n})),
        }
    }
}

fn checkout_target(ls: &[Label]) -> Option<Target> {
    if let Some(l) = ls.iter().find(|l| l.local && !l.head) {
        return Some(Target::Local(l.text.clone()));
    }
    ls.iter().find(|l| l.remote && !l.local).map(|l| Target::Remote(l.text.clone()))
}

/// Clicks, double-clicks, right-clicks and drags on a row's label.
fn pill_events(app: &GitApp, r: &egui::Response, ls: &[Label], commit: &Commit, actions: &mut Vec<Action>) {
    let Some(first) = ls.first() else { return };
    if ls.len() > 1 && r.hovered() && !r.dragged() && !egui::DragAndDrop::has_any_payload(&r.ctx) {
        actions.push(Action::LabelPopup(LabelPopup { oid: commit.oid.clone(), pill: r.rect }));
    }
    r.dnd_set_drag_payload(DragRef { name: first.text.clone() });
    if r.clicked() {
        actions.push(Action::Run("graph.select", json!({"oid": commit.oid})));
    }
    if r.double_clicked()
        && let Some(t) = checkout_target(ls)
    {
        actions.push(t.action());
    }
    r.context_menu(|ui| commit_menu(app, ui, commit, actions));
}

enum Action {
    Run(&'static str, serde_json::Value),
    Reword(Reword),
    DropMenu(String, String, Pos2),
    LabelPopup(LabelPopup),
    Dialog(Dialog),
    Copy(String),
    /// Add a commit to (or take it out of) the picked commits.
    Pick(String),
    ClearPicks,
}

impl Action {
    fn apply(self, app: &mut GitApp) {
        match self {
            Action::Run(id, p) => {
                let _ = app.run(id, p);
            }
            Action::Dialog(d) => app.dialog = Some(d),
            Action::LabelPopup(p) => app.label_popup = Some(p),
            Action::DropMenu(from, onto, at) => {
                app.drop_menu = Some(DropMenu::new(app, from, onto, at));
            }
            Action::Reword(r) => {
                if app.run("graph.select", json!({"oid": r.oid})).is_ok() {
                    app.reword = Some(r);
                }
            }
            Action::Copy(s) => app.copy(s),
            Action::Pick(oid) => {
                if let Some(i) = app.picked_commits.iter().position(|o| *o == oid) {
                    app.picked_commits.remove(i);
                } else if app.picked_commits.len() < 1000 {
                    app.picked_commits.push(oid);
                }
            }
            Action::ClearPicks => app.picked_commits.clear(),
        }
    }
}

fn wip_row(
    app: &GitApp,
    ui: &mut egui::Ui,
    rect: Rect,
    cols: &Cols,
    head_lane: usize,
    has_head: bool,
    resp: &egui::Response,
    actions: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    let st = &app.session.data.status;
    let x = lane_x(cols.graph, head_lane);
    let c = pos2(x, rect.center().y);
    if has_head {
        p.with_clip_rect(cols.graph).line_segment([c + vec2(0.0, 7.0), pos2(x, rect.bottom())], Stroke::new(1.5, t.dim));
    }
    // A dashed ring.
    let n = 16;
    for k in (0..n).step_by(2) {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        p.line_segment([c + 7.0 * vec2(a0.cos(), a0.sin()), c + 7.0 * vec2(a1.cos(), a1.sin())], Stroke::new(1.6, t.dim));
    }
    let staged = st.staged().count();
    let unstaged = st.unstaged().count();
    let conflicts = st.conflicted().count();
    let summary = app.session.draft.summary.trim();
    let mut text = if summary.is_empty() { "// WIP".to_string() } else { summary.to_string() };
    if conflicts > 0 {
        text.push_str(&format!("   ({conflicts} conflicted)"));
    }
    text_in(p, cols.msg, &text, FontId::proportional(13.0), if conflicts > 0 { t.warning } else { t.dim });
    // Counts of unstaged and staged files, right-aligned in the label column.
    let y = rect.center().y;
    let mut x = cols.refs.right() - 8.0;
    for (icon, n) in [(Icon::Check, staged), (Icon::File, unstaged)] {
        let g = p.layout_no_wrap(n.to_string(), FontId::proportional(12.0), t.dim);
        x -= g.size().x;
        p.galley(pos2(x, y - g.size().y / 2.0), g, t.dim);
        x -= 16.0;
        icons::paint(p, Rect::from_center_size(pos2(x + 7.0, y), vec2(13.0, 13.0)), icon, t.dim, t.accent);
        x -= 8.0;
    }
    if resp.clicked() {
        actions.push(Action::Run("graph.select", json!({"oid": "wip"})));
    }
    resp.context_menu(|ui| {
        for (id, label) in [("stage.all", "Stage All Changes"), ("unstage.all", "Unstage All Changes"), ("stash.save", "Stash All Changes")] {
            if ui.add_enabled(app.disabled(id).is_none(), egui::Button::new(label)).clicked() {
                actions.push(Action::Run(id, json!({})));
                ui.close();
            }
        }
        if ui.add_enabled(app.disabled("discard.all").is_none(), egui::Button::new("Discard All Changes…")).clicked() {
            actions.push(Action::Dialog(Dialog::confirm(
                "Discard all changes?",
                "Every staged and unstaged change is thrown away and untracked files are deleted. This can't be undone.",
                "discard.all",
                json!({}),
                "Discard All",
            )));
            ui.close();
        }
    });
}

/// The lane lines of one row: straight where a lane passes, curved where it bends into or out of
/// the commit.
fn lanes(p: &egui::Painter, graph: Rect, row: Rect, up: &[Seg], down: &[Seg], fade: &dyn Fn(Color32) -> Color32) {
    let (top, mid, bottom) = (row.top(), row.center().y, row.bottom());
    let w = 2.0;
    for s in up {
        let (xf, xt) = (lane_x(graph, s.from), lane_x(graph, s.to));
        let st = Stroke::new(w, fade(lane_color(s.color)));
        if s.from == s.to {
            p.line_segment([pos2(xf, top), pos2(xf, mid)], st);
        } else {
            p.add(CubicBezierShape::from_points_stroke(
                [pos2(xf, top), pos2(xf, mid), pos2(xf, mid), pos2(xt, mid)],
                false,
                Color32::TRANSPARENT,
                st,
            ));
        }
    }
    for s in down {
        let (xf, xt) = (lane_x(graph, s.from), lane_x(graph, s.to));
        let st = Stroke::new(w, fade(lane_color(s.color)));
        if s.from == s.to {
            p.line_segment([pos2(xf, mid), pos2(xf, bottom)], st);
        } else {
            p.add(CubicBezierShape::from_points_stroke(
                [pos2(xf, mid), pos2(xt, mid), pos2(xt, mid), pos2(xt, bottom)],
                false,
                Color32::TRANSPARENT,
                st,
            ));
        }
    }
}

/// A commit dot's radius (merges are smaller).
const NODE_R: f32 = 10.0;

/// A commit's dot: the author's picture (or initials) ringed in the lane colour; merges are small;
/// HEAD gets an outer ring.
fn node(p: &egui::Painter, avatars: &Avatars, c: Pos2, commit: &Commit, color: Color32, head: bool, tint: Color32, t: Tokens) {
    if commit.is_merge() {
        p.circle_filled(c, 5.0, color);
        p.circle_stroke(c, 5.0, Stroke::new(1.5, t.bg));
    } else {
        p.circle_filled(c, NODE_R, color);
        if !avatars.paint(p, c, NODE_R - 2.0, &commit.author.email, tint) {
            p.text(c, Align2::CENTER_CENTER, util::initials(&commit.author.name), FontId::proportional(8.5), crate::theme::on_lane());
        }
    }
    if head {
        p.circle_stroke(c, if commit.is_merge() { 8.0 } else { NODE_R + 2.5 }, Stroke::new(1.5, t.accent));
    }
}

/// The hover card over a commit's dot: the author's picture, name and address.
fn person_card(ui: &mut egui::Ui, avatars: &Avatars, c: &Commit) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        avatars.paint_or_initials(ui.painter(), r.center(), 32.0, &c.author.email, &c.author.name, Color32::WHITE);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&c.author.name).strong().size(14.0));
            ui.label(egui::RichText::new(&c.author.email).small().color(t.dim));
            ui.label(egui::RichText::new(util::date_time(c.author.time)).small().color(t.dim));
        });
    });
}

fn author_cell(p: &egui::Painter, avatars: &Avatars, r: Rect, c: &Commit, fade: &dyn Fn(Color32) -> Color32, t: Tokens) {
    let center = pos2(r.left() + 14.0, r.center().y);
    avatars.paint_or_initials(p, center, 8.0, &c.author.email, &c.author.name, fade(Color32::WHITE));
    text_in(p, Rect::from_min_max(pos2(r.left() + 20.0, r.top()), r.max), &c.author.name, FontId::proportional(12.0), fade(t.text));
}

/// The row's labels: the first as a pill (with local / remote / tag marks), the rest counted.
/// The pill can be clicked, right-clicked and dragged.
fn pills(ui: &mut egui::Ui, r: Rect, ls: &[Label], color: Color32) -> Option<egui::Response> {
    let p = ui.painter();
    let first = ls.first()?;
    let h = 20.0;
    let mut text = first.text.clone();
    if ls.len() > 1 {
        text.push_str(&format!("  +{}", ls.len() - 1));
    }
    let font = FontId::proportional(11.5);
    let g = p.layout_no_wrap(text, font, Color32::WHITE);
    let icons_w = 14.0 * f32::from(u8::from(first.local) + u8::from(first.remote) + u8::from(first.tag));
    let w = (g.size().x + icons_w + 14.0).min(r.width() - 8.0);
    let pill = Rect::from_min_size(pos2(r.right() - w - 4.0, r.center().y - h / 2.0), vec2(w, h));
    p.rect_filled(pill, 4.0, color);
    if first.head {
        p.rect_stroke(pill.expand(1.5), 5.0, Stroke::new(1.5, Tokens::get(ui.ctx()).accent), egui::StrokeKind::Outside);
    }
    let mut x = pill.left() + 5.0;
    for (on, icon) in [(first.local, Icon::Laptop), (first.remote, Icon::Cloud), (first.tag, Icon::Tag)] {
        if on {
            icons::paint(p, Rect::from_center_size(pos2(x + 6.0, pill.center().y), vec2(12.0, 12.0)), icon, Color32::WHITE, Color32::WHITE);
            x += 14.0;
        }
    }
    p.with_clip_rect(pill.shrink(2.0)).galley(pos2(x + 2.0, pill.center().y - g.size().y / 2.0), g, Color32::WHITE);
    let resp = ui.interact(pill, egui::Id::new(("pill", &first.text, first.target)), Sense::click_and_drag());
    if resp.hovered() && !resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    // Several labels: hovering opens the list of all of them instead (see `label_popup`).
    if ls.len() > 1 {
        return Some(resp);
    }
    Some(resp.on_hover_text(format!("{}\n\nDouble-click to check out; drag onto another branch to merge or rebase", first.text)))
}

fn commit_menu(app: &GitApp, ui: &mut egui::Ui, c: &Commit, actions: &mut Vec<Action>) {
    let short = c.short().to_string();
    let d = &app.session.data;
    if app.picked_commits.len() > 1 && app.picked_commits.contains(&c.oid) {
        // Oldest first: the graph lists newest first.
        let mut picks = app.picked_commits.clone();
        picks.sort_by_key(|o| std::cmp::Reverse(d.row_of.get(o).copied().unwrap_or(0)));
        ui.label(egui::RichText::new(format!("{} picked commits", picks.len())).small().weak());
        if ui.add_enabled(app.disabled("cherryPick.commit").is_none(), egui::Button::new(format!("Cherry Pick {} Commits", picks.len()))).clicked() {
            actions.push(Action::Run("cherryPick.commit", json!({"oids": picks})));
            actions.push(Action::ClearPicks);
            ui.close();
        }
        if ui.button("Clear Picks").clicked() {
            actions.push(Action::ClearPicks);
            ui.close();
        }
        ui.separator();
    }
    for l in labels(d, &c.oid) {
        if l.tag {
            continue;
        }
        if let Some(r) = d.refs.get(l.target).cloned() {
            ui.menu_button(format!("Branch {}", l.text), |ui| {
                // The branch menu needs the app mutably: queue the common actions instead.
                if !r.head && ui.button(format!("Checkout {}", r.name)).clicked() {
                    actions.push(Action::Run("branch.checkout", json!({"name": r.name})));
                    ui.close();
                }
                if !r.head && ui.button(format!("Merge {} into current branch", r.name)).clicked() {
                    actions.push(Action::Run("merge.into", json!({"ref": r.name})));
                    ui.close();
                }
                if !r.head && ui.button(format!("Rebase current branch onto {}", r.name)).clicked() {
                    actions.push(Action::Run("rebase.onto", json!({"ref": r.name})));
                    ui.close();
                }
                if ui.button(format!("Solo {}", r.name)).on_hover_text("Show only this branch's history").clicked() {
                    actions.push(Action::Run("graph.solo", json!({"name": r.name})));
                    ui.close();
                }
                if !r.head && ui.button(format!("Hide {}", r.name)).on_hover_text("Leave this branch out of the graph").clicked() {
                    actions.push(Action::Run("graph.hide", json!({"name": r.name})));
                    ui.close();
                }
                if ui.button("Copy Branch Name").clicked() {
                    actions.push(Action::Copy(r.name.clone()));
                    ui.close();
                }
            });
        }
    }
    let item =
        |ui: &mut egui::Ui, id: &'static str, label: &str| -> bool { ui.add_enabled(app.disabled(id).is_none(), egui::Button::new(label)).clicked() };
    if item(ui, "commit.checkout", &format!("Checkout {short} (detached)")) {
        actions.push(Action::Run("commit.checkout", json!({"oid": c.oid})));
        ui.close();
    }
    if item(ui, "branch.create", "Create Branch Here…") {
        actions.push(Action::Dialog(Dialog::CreateBranch { name: String::new(), from: Some(c.oid.clone()), checkout: true }));
        ui.close();
    }
    if item(ui, "tag.create", "Create Tag Here…") {
        actions.push(Action::Dialog(Dialog::CreateTag { name: String::new(), oid: Some(c.oid.clone()), message: String::new() }));
        ui.close();
    }
    if item(ui, "commit.reword", "Edit Commit Message") {
        actions.push(Action::Reword(Reword { oid: c.oid.clone(), summary: c.summary.clone(), description: c.body.trim().to_string(), focus: true }));
        ui.close();
    }
    ui.menu_button("Edit History", |ui| {
        if item(ui, "rebase.interactive", "Interactive Rebase from Here…") {
            match Dialog::named("rebase.interactive", &app.session, &json!({"oid": c.oid})) {
                Some(d) => actions.push(Action::Dialog(d)),
                // Not rebasable (a merge in the way): the plan's error says why.
                None => actions.push(Action::Run("rebase.plan", json!({"oid": c.oid}))),
            }
            ui.close();
        }
        if !c.parents.is_empty() && item(ui, "commit.squash", "Squash into Parent") {
            actions.push(Action::Run("commit.squash", json!({"oid": c.oid})));
            ui.close();
        }
        if item(ui, "commit.moveUp", "Move Up") {
            actions.push(Action::Run("commit.moveUp", json!({"oid": c.oid})));
            ui.close();
        }
        if !c.parents.is_empty() && item(ui, "commit.moveDown", "Move Down") {
            actions.push(Action::Run("commit.moveDown", json!({"oid": c.oid})));
            ui.close();
        }
        if item(ui, "commit.drop", "Drop Commit…") {
            actions.push(Action::Dialog(Dialog::confirm(
                &format!("Drop {short}?"),
                "The commit is removed from the branch and the commits after it are re-created. Undo brings it back.",
                "commit.drop",
                json!({"oid": c.oid}),
                "Drop",
            )));
            ui.close();
        }
    });
    ui.separator();
    if item(ui, "cherryPick.commit", "Cherry Pick Commit") {
        actions.push(Action::Run("cherryPick.commit", json!({"oid": c.oid})));
        ui.close();
    }
    if item(ui, "revert.commit", "Revert Commit") {
        actions.push(Action::Run("revert.commit", json!({"oid": c.oid})));
        ui.close();
    }
    let branch = d.status.branch.clone().unwrap_or_else(|| "HEAD".into());
    ui.menu_button(format!("Reset {branch} to this commit"), |ui| {
        for (mode, label) in [("soft", "Soft: keep all changes staged"), ("mixed", "Mixed: keep the work tree, reset the index")] {
            if item(ui, "reset.to", label) {
                actions.push(Action::Run("reset.to", json!({"oid": c.oid, "mode": mode})));
                ui.close();
            }
        }
        if item(ui, "reset.to", "Hard: discard all changes…") {
            actions.push(Action::Dialog(Dialog::confirm(
                &format!("Hard reset {branch} to {short}?"),
                "Uncommitted changes are thrown away. Undo brings the branch back, but not the uncommitted changes.",
                "reset.to",
                json!({"oid": c.oid, "mode": "hard"}),
                "Reset Hard",
            )));
            ui.close();
        }
    });
    ui.menu_button("Compare", |ui| {
        if ui.button("With the Working Tree").clicked() {
            actions.push(Action::Run("diff.compare", json!({"from": c.oid})));
            ui.close();
        }
        if d.status.head.as_deref() != Some(c.oid.as_str()) && ui.button("With HEAD").clicked() {
            actions.push(Action::Run("diff.compare", json!({"from": c.oid, "to": "HEAD"})));
            ui.close();
        }
        if let Selected::Commit(o) = &app.session.sel.row
            && *o != c.oid
            && ui.button(format!("With the Selected Commit ({})", o.get(..7).unwrap_or(o))).clicked()
        {
            actions.push(Action::Run("diff.compare", json!({"from": o, "to": c.oid})));
            ui.close();
        }
        ui.label(egui::RichText::new(format!("Tip: {}-click a second commit", util::shortcut_label("Mod"))).small().weak());
    });
    if item(ui, "patch.create", "Create Patch…") {
        if let Some(d) = Dialog::named("patch.create", &app.session, &json!({"oid": c.oid})) {
            actions.push(Action::Dialog(d));
        }
        ui.close();
    }
    if app.session.data.setting("gitcraft.aicommand").is_some() && item(ui, "commit.explain", "Explain Commit (AI)") {
        actions.push(Action::Run("commit.explain", json!({"oid": c.oid})));
        actions.push(Action::Dialog(Dialog::AiText { title: format!("Explain {short}"), id: "commit.explain".into(), field: "explanation".into() }));
        ui.close();
    }
    if crate::hosting_ui::forge_kind(app).is_some() && item(ui, "cloudpatch.create", "Share as Cloud Patch…") {
        actions.push(Action::Dialog(Dialog::confirm(
            &format!("Share {short} as a patch?"),
            "It's uploaded as a secret gist (GitHub) or private snippet (GitLab) on your account; anyone with the link can read it.",
            "cloudpatch.create",
            json!({"oid": c.oid}),
            "Share",
        )));
        ui.close();
    }
    if ui.button(format!("Highlight Commits by {}", c.author.name)).clicked() {
        actions.push(Action::Run("view.search", json!({"text": c.author.email})));
        ui.close();
    }
    ui.separator();
    if ui.button("Copy Commit SHA").clicked() {
        actions.push(Action::Copy(c.oid.clone()));
        ui.close();
    }
    if ui.button("Copy Commit Message").clicked() {
        actions.push(Action::Copy(c.message()));
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str, local: bool, remote: bool, head: bool) -> Label {
        Label { text: text.into(), local, remote, tag: false, head, target: 0 }
    }

    #[test]
    fn double_click_prefers_a_local_branch_then_a_remote_one() {
        let remote_only = [label("main", true, false, true), label("origin/main", false, true, false)];
        assert_eq!(checkout_target(&remote_only), Some(Target::Remote("origin/main".into())));
        let both = [label("origin/x", false, true, false), label("x2", true, false, false)];
        assert_eq!(checkout_target(&both), Some(Target::Local("x2".into())));
        assert_eq!(checkout_target(&[label("main", true, true, true)]), None);
        assert_eq!(checkout_target(&[]), None);
    }
}
