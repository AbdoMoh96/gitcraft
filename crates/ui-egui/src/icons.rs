//! GitCraft's icon set, drawn in code (original artwork; no external icon assets).
//!
//! Icons are designed on a 20×20 grid in one line colour (`c`) and an accent (`a`).

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Undo,
    Redo,
    Pull,
    Push,
    Fetch,
    Branch,
    Stash,
    Pop,
    Folder,
    Search,
    Close,
    Plus,
    Minus,
    Trash,
    Tag,
    Cloud,
    Laptop,
    Commit,
    File,
    Warning,
    Check,
    Palette,
    Refresh,
    Moon,
    Sun,
    History,
    Blame,
    Chevron,
    ChevronDown,
    ChevronUp,
    Pencil,
    Terminal,
}

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    c: Color32,
    a: Color32,
}

impl Pen<'_> {
    fn s(&self) -> f32 {
        self.r.width().min(self.r.height()) / 20.0
    }
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        let s = self.s();
        let o = self.r.center() - vec2(10.0 * s, 10.0 * s);
        pos2(o.x + x * s, o.y + y * s)
    }
    fn st(&self, c: Color32) -> Stroke {
        Stroke::new(1.6 * self.s(), c)
    }
    fn line(&self, pts: &[(f32, f32)]) {
        self.line_c(pts, self.c);
    }
    fn line_c(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(v, self.st(c)));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::closed_line(v, self.st(self.c)));
    }
    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::convex_polygon(v, c, Stroke::NONE));
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s(), self.st(c));
    }
    fn dot(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.s(), c);
    }
    /// An arc around (x, y) from angle `a0` to `a1` (radians, 0 = right, clockwise on screen).
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32) {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let t = a0 + (a1 - a0) * i as f32 / n as f32;
                (x + r * t.cos(), y + r * t.sin())
            })
            .collect();
        self.line(&pts);
    }
    fn arrow_down(&self, x: f32, top: f32, bottom: f32, c: Color32) {
        self.line_c(&[(x, top), (x, bottom)], c);
        self.line_c(&[(x - 3.5, bottom - 3.5), (x, bottom), (x + 3.5, bottom - 3.5)], c);
    }
    fn arrow_up(&self, x: f32, top: f32, bottom: f32, c: Color32) {
        self.line_c(&[(x, bottom), (x, top)], c);
        self.line_c(&[(x - 3.5, top + 3.5), (x, top), (x + 3.5, top + 3.5)], c);
    }
}

/// Paint `icon` in `r`.
pub fn paint(p: &Painter, r: Rect, icon: Icon, c: Color32, a: Color32) {
    let pen = Pen { p, r, c, a };
    use std::f32::consts::PI;
    match icon {
        Icon::Undo => {
            pen.arc(10.0, 11.0, 5.5, -PI * 0.9, PI * 0.6);
            pen.line(&[(2.5, 5.0), (4.6, 9.4), (8.6, 7.4)]);
        }
        Icon::Redo => {
            pen.arc(10.0, 11.0, 5.5, PI * 0.4, PI * 1.9);
            pen.line(&[(17.5, 5.0), (15.4, 9.4), (11.4, 7.4)]);
        }
        Icon::Pull => {
            pen.arrow_down(10.0, 2.5, 13.5, pen.a);
            pen.line(&[(3.0, 13.0), (3.0, 17.0), (17.0, 17.0), (17.0, 13.0)]);
        }
        Icon::Push => {
            pen.arrow_up(10.0, 2.5, 13.5, pen.a);
            pen.line(&[(3.0, 13.0), (3.0, 17.0), (17.0, 17.0), (17.0, 13.0)]);
        }
        Icon::Fetch => {
            pen.arc(10.0, 10.0, 6.5, -PI * 0.25, PI * 1.35);
            pen.arrow_down(10.0, 6.0, 13.0, pen.a);
        }
        Icon::Branch => {
            pen.circle(6.0, 4.5, 2.0, pen.c);
            pen.circle(6.0, 15.5, 2.0, pen.c);
            pen.circle(14.5, 6.5, 2.0, pen.a);
            pen.line(&[(6.0, 6.5), (6.0, 13.5)]);
            pen.line_c(&[(14.5, 8.5), (14.5, 9.5), (13.5, 11.0), (7.5, 12.5)], pen.a);
        }
        Icon::Stash => {
            pen.closed(&[(3.0, 9.0), (17.0, 9.0), (17.0, 17.0), (3.0, 17.0)]);
            pen.arrow_down(10.0, 2.0, 13.0, pen.a);
        }
        Icon::Pop => {
            pen.closed(&[(3.0, 9.0), (17.0, 9.0), (17.0, 17.0), (3.0, 17.0)]);
            pen.arrow_up(10.0, 2.0, 13.0, pen.a);
        }
        Icon::Folder => {
            pen.closed(&[(2.5, 5.0), (7.5, 5.0), (9.0, 7.0), (17.5, 7.0), (17.5, 16.0), (2.5, 16.0)]);
            pen.line_c(&[(2.5, 9.0), (17.5, 9.0)], pen.a);
        }
        Icon::Search => {
            pen.circle(8.5, 8.5, 5.0, pen.c);
            pen.line_c(&[(12.3, 12.3), (17.0, 17.0)], pen.a);
        }
        Icon::Close => {
            pen.line(&[(5.0, 5.0), (15.0, 15.0)]);
            pen.line(&[(15.0, 5.0), (5.0, 15.0)]);
        }
        Icon::Plus => {
            pen.line(&[(10.0, 4.0), (10.0, 16.0)]);
            pen.line(&[(4.0, 10.0), (16.0, 10.0)]);
        }
        Icon::Minus => pen.line(&[(4.0, 10.0), (16.0, 10.0)]),
        Icon::Terminal => {
            // A window with a prompt and a cursor.
            pen.closed(&[(2.5, 4.0), (17.5, 4.0), (17.5, 16.0), (2.5, 16.0)]);
            pen.line(&[(5.5, 8.0), (8.5, 10.5), (5.5, 13.0)]);
            pen.line_c(&[(10.0, 13.0), (14.5, 13.0)], pen.a);
        }
        Icon::Pencil => {
            // A pencil lying from bottom left to top right, its tip in the accent colour.
            pen.closed(&[(5.5, 12.0), (13.5, 4.0), (16.0, 6.5), (8.0, 14.5)]);
            pen.line(&[(12.0, 5.5), (14.5, 8.0)]);
            pen.fill(&[(5.5, 12.0), (8.0, 14.5), (4.0, 16.0)], pen.a);
        }
        Icon::Trash => {
            pen.line(&[(3.5, 5.5), (16.5, 5.5)]);
            pen.line(&[(8.0, 5.5), (8.0, 3.5), (12.0, 3.5), (12.0, 5.5)]);
            pen.closed(&[(5.0, 5.5), (15.0, 5.5), (14.0, 17.0), (6.0, 17.0)]);
            pen.line_c(&[(8.5, 8.5), (8.5, 14.0)], pen.a);
            pen.line_c(&[(11.5, 8.5), (11.5, 14.0)], pen.a);
        }
        Icon::Tag => {
            pen.closed(&[(3.0, 3.0), (10.0, 3.0), (17.0, 10.0), (10.0, 17.0), (3.0, 10.0)]);
            pen.dot(6.8, 6.8, 1.4, pen.a);
        }
        Icon::Cloud => {
            pen.arc(7.0, 11.5, 3.5, PI * 0.5, PI * 1.5);
            pen.arc(11.0, 9.0, 4.5, PI * 1.05, PI * 2.0);
            pen.arc(14.5, 12.0, 3.0, -PI * 0.5, PI * 0.5);
            pen.line(&[(7.0, 15.0), (14.5, 15.0)]);
        }
        Icon::Laptop => {
            pen.closed(&[(4.5, 4.5), (15.5, 4.5), (15.5, 12.5), (4.5, 12.5)]);
            pen.line_c(&[(2.0, 15.5), (18.0, 15.5)], pen.a);
        }
        Icon::Commit => {
            pen.circle(10.0, 10.0, 3.5, pen.a);
            pen.line(&[(1.5, 10.0), (6.5, 10.0)]);
            pen.line(&[(13.5, 10.0), (18.5, 10.0)]);
        }
        Icon::File => {
            pen.closed(&[(5.0, 2.5), (12.0, 2.5), (15.5, 6.0), (15.5, 17.5), (5.0, 17.5)]);
            pen.line_c(&[(12.0, 2.5), (12.0, 6.0), (15.5, 6.0)], pen.a);
        }
        Icon::Warning => {
            pen.closed(&[(10.0, 2.5), (18.0, 17.0), (2.0, 17.0)]);
            pen.line_c(&[(10.0, 7.5), (10.0, 12.0)], pen.a);
            pen.dot(10.0, 14.6, 1.0, pen.a);
        }
        Icon::Check => pen.line_c(&[(4.0, 10.5), (8.0, 14.5), (16.0, 5.5)], pen.a),
        Icon::Palette => {
            pen.closed(&[(3.0, 4.0), (17.0, 4.0), (17.0, 16.0), (3.0, 16.0)]);
            pen.line_c(&[(6.0, 8.0), (14.0, 8.0)], pen.a);
            pen.line(&[(6.0, 12.0), (11.0, 12.0)]);
        }
        Icon::Refresh => {
            pen.arc(10.0, 10.0, 6.0, -PI * 0.4, PI * 1.4);
            pen.fill(&[(13.0, 1.8), (16.8, 5.4), (12.0, 6.6)], pen.a);
        }
        Icon::Moon => {
            pen.arc(10.0, 10.0, 7.0, PI * 0.35, PI * 1.65);
            pen.arc(13.5, 8.0, 5.5, PI * 0.62, PI * 1.38);
        }
        Icon::Sun => {
            pen.circle(10.0, 10.0, 3.5, pen.a);
            for i in 0..8 {
                let t = i as f32 * PI / 4.0;
                pen.line(&[(10.0 + 6.0 * t.cos(), 10.0 + 6.0 * t.sin()), (10.0 + 8.0 * t.cos(), 10.0 + 8.0 * t.sin())]);
            }
        }
        Icon::History => {
            pen.arc(10.0, 10.0, 7.0, -PI * 0.9, PI * 1.0);
            pen.line_c(&[(10.0, 6.0), (10.0, 10.0), (13.0, 12.0)], pen.a);
            pen.line(&[(1.8, 5.5), (3.2, 8.4), (6.2, 7.0)]);
        }
        Icon::Blame => {
            pen.dot(5.0, 5.0, 1.6, pen.a);
            pen.dot(5.0, 10.0, 1.6, pen.a);
            pen.dot(5.0, 15.0, 1.6, pen.a);
            pen.line(&[(8.5, 5.0), (17.0, 5.0)]);
            pen.line(&[(8.5, 10.0), (15.0, 10.0)]);
            pen.line(&[(8.5, 15.0), (16.0, 15.0)]);
        }
        Icon::Chevron => pen.line(&[(7.5, 4.5), (13.0, 10.0), (7.5, 15.5)]),
        Icon::ChevronDown => pen.line(&[(4.5, 7.5), (10.0, 13.0), (15.5, 7.5)]),
        Icon::ChevronUp => pen.line(&[(4.5, 12.5), (10.0, 7.0), (15.5, 12.5)]),
    }
}

/// An icon-only button with a tooltip; `None` when disabled (with the reason in the tooltip).
pub fn button(ui: &mut Ui, icon: Icon, tip: &str, disabled: Option<&str>, size: f32) -> Response {
    let t = crate::theme::Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 8.0), if disabled.is_some() { Sense::hover() } else { Sense::click() });
    if resp.hovered() && disabled.is_none() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    let (c, a) = if disabled.is_some() { (t.dim.gamma_multiply(0.5), t.dim.gamma_multiply(0.5)) } else { (t.text, t.accent) };
    paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size)), icon, c, a);
    match disabled {
        Some(why) => resp.on_hover_text(format!("{tip}: {why}")),
        None => resp.on_hover_text(tip),
    }
}

/// A toolbar tool: icon over a caption, like a big button.
pub fn tool(ui: &mut Ui, icon: Icon, caption: &str, tip: &str, disabled: Option<&str>) -> Response {
    let t = crate::theme::Tokens::get(ui.ctx());
    let size = vec2(56.0, 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, if disabled.is_some() { Sense::hover() } else { Sense::click() });
    if (resp.hovered() || resp.has_focus()) && disabled.is_none() {
        ui.painter().rect_filled(rect, 6.0, t.hover);
    }
    let (c, a) = if disabled.is_some() { (t.dim.gamma_multiply(0.45), t.dim.gamma_multiply(0.45)) } else { (t.text, t.accent) };
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 15.0), Vec2::splat(20.0));
    paint(ui.painter(), icon_rect, icon, c, a);
    ui.painter().text(pos2(rect.center().x, rect.bottom() - 9.0), egui::Align2::CENTER_CENTER, caption, egui::FontId::proportional(11.0), c);
    match disabled {
        Some(why) => resp.on_hover_text(format!("{tip}: {why}")),
        None => resp.on_hover_text(tip),
    }
}
