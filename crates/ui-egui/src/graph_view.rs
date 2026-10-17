//! The commit graph: one row per commit (and the work-in-progress row on top) with the branch and
//! tag labels, the lanes, the message, author, date and id. Rows are virtualised: only the visible
//! ones are laid out, so long histories scroll smoothly.

use egui::epaint::CubicBezierShape;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use gitcraft_engine::git::{Commit, RefKind};
use gitcraft_engine::graph::Seg;
use gitcraft_engine::{RepoData, Selected};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{Tokens, lane_color};
use crate::{Dialog, GitApp, util};

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

fn columns(row: Rect, graph_w: f32) -> Cols {
    let mut x = row.left();
    let mut take = |w: f32| {
        let r = Rect::from_min_size(pos2(x, row.top()), vec2(w, row.height()));
        x += w;
        r
    };
    let refs = take(REFS_W);
    let graph = take(graph_w);
    let msg_w = (row.width() - REFS_W - graph_w - AUTHOR_W - DATE_W - SHA_W).max(120.0);
    let msg = take(msg_w);
    let author = take(AUTHOR_W);
    let date = take(DATE_W);
    let sha = take(SHA_W);
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
    let (hdr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().rect_filled(hdr, 0.0, t.chrome);
    let c = columns(hdr, graph_w);
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
    }

    let selected_row = match &app.session.sel.row {
        Selected::Wip => Some(0),
        Selected::Commit(o) => app.session.data.row_of.get(o).map(|&i| i + usize::from(wip)),
        Selected::None => None,
    };
    let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("graph_scroll");
    let view_h = ui.available_height();
    let offset_key = egui::Id::new("graph_offset");
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
    let out = area.show_rows(ui, ROW_H, n, |ui, range| {
        let now = util::now_s();
        let d = &app.session.data;
        let head_row = d.status.head.as_ref().and_then(|h| d.row_of.get(h)).copied();
        let head_lane = head_row.and_then(|r| d.graph.rows.get(r)).map(|r| r.lane).unwrap_or(0);
        for i in range {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            let cols = columns(rect, graph_w);
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
            let dim = !search.is_empty() && !gitcraft_engine::cmd::view::matches(commit, &search);
            let fade = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
            // Lanes.
            if let Some(row) = d.graph.rows.get(ci) {
                let gp = p.with_clip_rect(cols.graph);
                // The dashed line from the WIP row down to HEAD.
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
                        gp.extend(Shape::dashed_line(&[pos2(x, y0), pos2(x, y1)], Stroke::new(1.5, t.dim), 3.0, 3.0));
                    }
                }
                lanes(&gp, cols.graph, rect, &row.up, &row.down, &fade);
                node(
                    &gp,
                    pos2(lane_x(cols.graph, row.lane), rect.center().y),
                    commit,
                    fade(lane_color(row.color)),
                    d.status.head.as_deref() == Some(commit.oid.as_str()),
                    t,
                );
                // Labels.
                let ls = labels(d, &commit.oid);
                if !ls.is_empty() {
                    pills(ui, cols.refs, &ls, fade(lane_color(row.color)));
                    // A line from the label to the commit.
                    let y = rect.center().y;
                    p.line_segment(
                        [pos2(cols.refs.right() - 2.0, y), pos2(lane_x(cols.graph, row.lane) - 8.0, y)],
                        Stroke::new(1.0, fade(lane_color(row.color))),
                    );
                }
            }
            let msg_color = fade(t.text);
            text_in(&p, cols.msg, &commit.summary, FontId::proportional(13.0), msg_color);
            author_cell(&p, cols.author, commit, &fade, t);
            text_in(&p, cols.date, &util::relative_time(commit.author.time, now), FontId::proportional(12.0), fade(t.dim));
            text_in(&p, cols.sha, commit.short(), FontId::monospace(11.5), fade(t.dim));
            if resp.clicked() {
                actions.push(Action::Run("graph.select", json!({"oid": commit.oid})));
            }
            if resp.double_clicked() {
                // Double-click checks out the row's branch.
                if let Some(l) = labels(d, &commit.oid).into_iter().find(|l| l.local && !l.head) {
                    actions.push(Action::Run("branch.checkout", json!({"name": l.text})));
                }
            }
            let resp = resp.on_hover_text_at_pointer(format!(
                "{}\n{} <{}>\n{}",
                commit.message(),
                commit.author.name,
                commit.author.email,
                util::date_time(commit.author.time)
            ));
            resp.context_menu(|ui| commit_menu(app, ui, commit, &mut actions));
        }
    });
    ui.ctx().data_mut(|m| m.insert_temp(offset_key, out.state.offset.y));
    for a in actions {
        a.apply(app);
    }
}

enum Action {
    Run(&'static str, serde_json::Value),
    Dialog(Dialog),
    Copy(String),
}

impl Action {
    fn apply(self, app: &mut GitApp) {
        match self {
            Action::Run(id, p) => {
                let _ = app.run(id, p);
            }
            Action::Dialog(d) => app.dialog = Some(d),
            Action::Copy(s) => app.copy(s),
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
        p.with_clip_rect(cols.graph).extend(Shape::dashed_line(&[c, pos2(x, rect.bottom())], Stroke::new(1.5, t.dim), 3.0, 3.0));
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

/// A commit's dot: the author's initials on the lane colour; merges are small; HEAD gets a ring.
fn node(p: &egui::Painter, c: Pos2, commit: &Commit, color: Color32, head: bool, t: Tokens) {
    if commit.is_merge() {
        p.circle_filled(c, 5.0, color);
        p.circle_stroke(c, 5.0, Stroke::new(1.5, t.bg));
    } else {
        p.circle_filled(c, 9.0, color);
        p.text(c, Align2::CENTER_CENTER, util::initials(&commit.author.name), FontId::proportional(8.5), crate::theme::on_lane());
    }
    if head {
        p.circle_stroke(c, if commit.is_merge() { 8.0 } else { 11.5 }, Stroke::new(1.5, t.accent));
    }
}

fn author_cell(p: &egui::Painter, r: Rect, c: &Commit, fade: &dyn Fn(Color32) -> Color32, t: Tokens) {
    let center = pos2(r.left() + 14.0, r.center().y);
    p.circle_filled(center, 8.0, fade(util::person_color(&c.author.name)));
    p.text(center, Align2::CENTER_CENTER, util::initials(&c.author.name), FontId::proportional(8.0), Color32::WHITE);
    text_in(p, Rect::from_min_max(pos2(r.left() + 20.0, r.top()), r.max), &c.author.name, FontId::proportional(12.0), fade(t.text));
}

/// The row's labels: the first as a pill (with local / remote / tag marks), the rest counted.
fn pills(ui: &mut egui::Ui, r: Rect, ls: &[Label], color: Color32) {
    let p = ui.painter();
    let Some(first) = ls.first() else { return };
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
    if ls.len() > 1 {
        let all: Vec<&str> = ls.iter().map(|l| l.text.as_str()).collect();
        ui.interact(pill, egui::Id::new(("pill", &first.text, first.target)), Sense::hover()).on_hover_text(all.join("\n"));
    }
}

fn commit_menu(app: &GitApp, ui: &mut egui::Ui, c: &Commit, actions: &mut Vec<Action>) {
    let short = c.short().to_string();
    let d = &app.session.data;
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
