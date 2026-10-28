//! The built-in terminal: the user's shell in a pseudo-terminal, opened in the repository, shown in
//! a panel at the bottom of the window. Output is parsed by a VT100 emulator on a reader thread;
//! keys typed while the panel has the focus go to the shell.

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Frame, Margin, RichText, Sense, Stroke, pos2, vec2};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::GitApp;
use crate::theme::Tokens;

/// Lines kept above the screen.
const SCROLLBACK: usize = 5000;
const FONT: f32 = 12.5;

pub struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    screen: Arc<Mutex<vt100::Parser>>,
    /// The shell exited (the reader saw the end of its output).
    ended: Arc<AtomicBool>,
    pub dir: String,
    size: (u16, u16),
    /// Lines scrolled back from the bottom.
    scroll: usize,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal").field("dir", &self.dir).field("size", &self.size).finish()
    }
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).unwrap_or(NonZeroU16::MIN)
}

impl Terminal {
    /// Start the user's shell in `dir`, `rows` × `cols` characters.
    pub fn start(dir: &Path, rows: u16, cols: u16, ctx: egui::Context) -> Result<Terminal, String> {
        let (rows, cols) = (rows.clamp(2, 500), cols.clamp(10, 1000));
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| format!("terminal: {e}"))?;
        let mut cmd = CommandBuilder::new_default_prog();
        cmd.cwd(dir);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).map_err(|e| format!("terminal: {e}"))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| format!("terminal: {e}"))?;
        let writer = pair.master.take_writer().map_err(|e| format!("terminal: {e}"))?;
        let screen = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), SCROLLBACK)));
        let ended = Arc::new(AtomicBool::new(false));
        let (s2, e2) = (screen.clone(), ended.clone());
        std::thread::Builder::new()
            .name("gitcraft terminal".into())
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let (Ok(mut p), Some(bytes)) = (s2.lock(), buf.get(..n)) {
                                p.process(bytes);
                            }
                            ctx.request_repaint();
                        }
                    }
                }
                e2.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            })
            .map_err(|e| format!("terminal: {e}"))?;
        Ok(Terminal { master: pair.master, writer, child, screen, ended, dir: dir.display().to_string(), size: (rows, cols), scroll: 0 })
    }

    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    pub fn send(&mut self, bytes: &[u8]) {
        self.scroll = 0;
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.clamp(2, 500), cols.clamp(10, 1000));
        if (rows, cols) == self.size {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        if let Ok(mut p) = self.screen.lock() {
            p.set_size(nz(rows), nz(cols));
        }
    }

    /// The visible text (for agents and tests).
    pub fn contents(&self) -> String {
        self.screen.lock().map(|p| p.screen().contents()).unwrap_or_default()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// The 16 basic terminal colours (original values, readable on both themes' backgrounds).
const BASIC: [(u8, u8, u8); 16] = [
    (0x2E, 0x30, 0x3A),
    (0xD6, 0x45, 0x45),
    (0x3F, 0xA8, 0x5A),
    (0xC9, 0x9A, 0x2E),
    (0x4C, 0x7F, 0xE0),
    (0xA8, 0x5C, 0xD6),
    (0x2A, 0xA8, 0xB5),
    (0xC8, 0xCA, 0xD2),
    (0x6A, 0x6F, 0x80),
    (0xF0, 0x6A, 0x6A),
    (0x66, 0xCF, 0x80),
    (0xF0, 0xC6, 0x5A),
    (0x78, 0xA2, 0xF5),
    (0xC9, 0x8C, 0xF0),
    (0x5C, 0xD2, 0xDE),
    (0xF4, 0xF5, 0xF8),
];

fn color(c: vt100::Color, default: Color32) -> Color32 {
    match c {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(i) if i < 16 => {
            let (r, g, b) = BASIC.get(usize::from(i)).copied().unwrap_or((0x80, 0x80, 0x80));
            Color32::from_rgb(r, g, b)
        }
        vt100::Color::Idx(i) if i < 232 => {
            // The 6×6×6 cube.
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        vt100::Color::Idx(i) => {
            let v = 8 + (i - 232) * 10;
            Color32::from_rgb(v, v, v)
        }
    }
}

/// The bytes a key sends to the shell.
fn key_bytes(key: egui::Key, m: egui::Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    use egui::Key as K;
    if m.ctrl && !m.alt {
        let name = key.name();
        if name.len() == 1
            && let Some(c) = name.chars().next().filter(char::is_ascii_alphabetic)
        {
            return Some(vec![(c.to_ascii_lowercase() as u8) & 0x1f]);
        }
    }
    let arrow = |c: char| if app_cursor { format!("\x1bO{c}") } else { format!("\x1b[{c}") };
    let s: String = match key {
        K::Enter => "\r".into(),
        K::Backspace => "\x7f".into(),
        K::Tab if m.shift => "\x1b[Z".into(),
        K::Tab => "\t".into(),
        K::Escape => "\x1b".into(),
        K::ArrowUp => arrow('A'),
        K::ArrowDown => arrow('B'),
        K::ArrowRight => arrow('C'),
        K::ArrowLeft => arrow('D'),
        K::Home => "\x1b[H".into(),
        K::End => "\x1b[F".into(),
        K::Delete => "\x1b[3~".into(),
        K::Insert => "\x1b[2~".into(),
        K::PageUp => "\x1b[5~".into(),
        K::PageDown => "\x1b[6~".into(),
        _ => return None,
    };
    Some(s.into_bytes())
}

/// The terminal panel at the bottom of the window.
pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(root) = app.session.repo.as_ref().map(|r| r.root().to_path_buf()) else {
        app.terminal_open = false;
        return;
    };
    egui::Panel::bottom("terminal_panel")
        .default_size(240.0)
        .size_range(100.0..=900.0)
        .resizable(true)
        .frame(Frame::NONE.fill(t.bg).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            let mut close = false;
            let mut restart = false;
            Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 3)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("TERMINAL").small().strong());
                    if let Some(term) = &app.terminal {
                        ui.label(RichText::new(&term.dir).small().color(t.dim));
                        if term.ended() {
                            ui.label(RichText::new("ended").small().color(t.warning));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::icons::button(ui, crate::icons::Icon::Close, "Close the terminal", None, 12.0).clicked() {
                            close = true;
                        }
                        if ui.small_button("Restart").clicked() {
                            restart = true;
                        }
                    });
                });
            });
            if close {
                app.terminal = None;
                app.terminal_open = false;
                return;
            }
            let here = root.display().to_string();
            if restart || app.terminal.as_ref().is_some_and(|x| x.dir != here) {
                app.terminal = None;
            }
            let font = FontId::monospace(FONT);
            let (cw, ch) = ui.fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
            let area = ui.available_rect_before_wrap().shrink(4.0);
            let cols = ((area.width() / cw.max(1.0)) as u16).max(10);
            let rows = ((area.height() / ch.max(1.0)) as u16).max(2);
            if app.terminal.is_none() {
                match Terminal::start(&root, rows, cols, ui.ctx().clone()) {
                    Ok(term) => app.terminal = Some(term),
                    Err(e) => {
                        ui.label(RichText::new(e).color(t.danger));
                        return;
                    }
                }
            }
            let (rect, resp) = ui.allocate_exact_size(area.size(), Sense::click());
            if resp.clicked() || std::mem::take(&mut app.terminal_focus_request) {
                ui.memory_mut(|m| m.request_focus(resp.id));
            }
            let focused = ui.memory(|m| m.has_focus(resp.id));
            app.terminal_focused = focused;
            let Some(term) = app.terminal.as_mut() else { return };
            term.resize(rows, cols);
            if focused {
                // Keep Tab and arrows in the terminal.
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(resp.id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
                });
                let app_cursor = term.screen.lock().map(|p| p.screen().application_cursor()).unwrap_or(false);
                let events = ui.input(|i| i.events.clone());
                for e in events {
                    match e {
                        egui::Event::Text(s) => term.send(s.as_bytes()),
                        egui::Event::Paste(s) => term.send(s.as_bytes()),
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            if let Some(b) = key_bytes(key, modifiers, app_cursor) {
                                term.send(&b);
                            }
                        }
                        _ => {}
                    }
                }
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if resp.hovered() && scroll.abs() > 0.5 {
                let lines = (scroll / ch).round() as i64;
                term.scroll = (term.scroll as i64 + lines).clamp(0, SCROLLBACK as i64) as usize;
            }
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, if t.dark { Color32::from_rgb(0x14, 0x15, 0x1A) } else { Color32::from_rgb(0xFB, 0xFB, 0xFD) });
            if focused {
                p.rect_stroke(rect, 0.0, Stroke::new(1.0, t.accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            }
            let Ok(mut parser) = term.screen.lock() else { return };
            parser.screen_mut().set_scrollback(term.scroll);
            term.scroll = parser.screen().scrollback();
            let screen = parser.screen();
            let fg_default = t.text;
            for r in 0..rows {
                let mut job = LayoutJob::default();
                for c in 0..cols {
                    let Some(cell) = screen.cell(r, c) else { continue };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let mut fg = color(cell.fgcolor(), fg_default);
                    let mut bg = color(cell.bgcolor(), Color32::TRANSPARENT);
                    if cell.inverse() {
                        std::mem::swap(&mut fg, &mut bg);
                        if fg == Color32::TRANSPARENT {
                            fg = t.bg;
                        }
                        if bg == Color32::TRANSPARENT {
                            bg = fg_default;
                        }
                    }
                    if cell.dim() {
                        fg = fg.gamma_multiply(0.6);
                    }
                    let text = if cell.has_contents() { cell.contents() } else { " " };
                    job.append(
                        text,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: fg,
                            background: bg,
                            italics: cell.italic(),
                            underline: if cell.underline() { Stroke::new(1.0, fg) } else { Stroke::NONE },
                            ..Default::default()
                        },
                    );
                }
                let galley = ui.painter().layout_job(job);
                p.galley(pos2(rect.left(), rect.top() + f32::from(r) * ch), galley, fg_default);
            }
            if term.scroll == 0 && !screen.hide_cursor() {
                let (cr, cc) = screen.cursor_position();
                let cur = egui::Rect::from_min_size(pos2(rect.left() + f32::from(cc) * cw, rect.top() + f32::from(cr) * ch), vec2(cw, ch));
                if focused {
                    p.rect_filled(cur, 0.0, t.accent.gamma_multiply(0.55));
                } else {
                    p.rect_stroke(cur, 0.0, Stroke::new(1.0, t.dim), egui::StrokeKind::Inside);
                }
            }
            if term.scroll > 0 {
                p.text(
                    rect.right_top() + vec2(-8.0, 4.0),
                    egui::Align2::RIGHT_TOP,
                    format!("{} lines up", term.scroll),
                    FontId::proportional(11.0),
                    t.dim,
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_colours() {
        assert_eq!(key_bytes(egui::Key::C, egui::Modifiers::CTRL, false), Some(vec![3]));
        assert_eq!(key_bytes(egui::Key::ArrowUp, egui::Modifiers::NONE, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(egui::Key::ArrowUp, egui::Modifiers::NONE, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(egui::Key::A, egui::Modifiers::NONE, false), None);
        for i in 0..=255u8 {
            let _ = color(vt100::Color::Idx(i), Color32::WHITE);
        }
    }

    #[cfg(unix)]
    #[test]
    fn runs_a_shell() {
        let d = gitcraft_git::testing::TempDir::new("term").unwrap();
        let ctx = egui::Context::default();
        let Ok(mut term) = Terminal::start(d.path(), 10, 60, ctx) else { return };
        term.send(b"echo gitcraft-$((40+2))\r");
        let mut seen = false;
        for _ in 0..200 {
            if term.contents().contains("gitcraft-42") {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen, "{}", term.contents());
        term.resize(20, 80);
        term.send(b"exit\r");
    }
}
