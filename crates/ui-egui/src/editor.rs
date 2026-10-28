//! The in-app file editor: a work tree file opened in the center, saved with `file.edit`.

use egui::{FontId, Frame, Margin, RichText, Stroke};
use serde_json::json;

use crate::theme::Tokens;
use crate::{GitApp, util};

/// A file being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub path: String,
    pub text: String,
    /// The text as loaded or last saved (unsaved changes when different).
    pub saved: String,
}

/// Largest file opened in the editor.
const MAX_OPEN: usize = 4 * 1024 * 1024;

impl GitApp {
    /// Open a work tree file in the editor.
    pub fn edit_file(&mut self, path: &str) -> Result<(), String> {
        let bytes = self.session.file_bytes(path, "worktree").map_err(|e| e.to_string())?;
        if bytes.len() > MAX_OPEN {
            return Err("the file is too large to edit here: open it in an editor".into());
        }
        if bytes.iter().take(8000).any(|b| *b == 0) {
            return Err("binary files can't be edited here".into());
        }
        let text = String::from_utf8(bytes).map_err(|_| "the file isn't UTF-8 text: open it in an editor".to_string())?;
        self.editor = Some(Editor { path: path.to_string(), saved: text.clone(), text });
        Ok(())
    }
}

pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(mut ed) = app.editor.take() else { return };
    let dirty = ed.text != ed.saved;
    let mut save = false;
    let mut close = false;
    Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(8, 5)).stroke(Stroke::new(1.0, t.border)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Editing {}", ed.path)).strong());
            if dirty {
                ui.label(RichText::new("unsaved").small().color(t.warning));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").on_hover_text("Esc").clicked() {
                    close = true;
                }
                let b = egui::Button::new(RichText::new("Save").color(egui::Color32::WHITE).strong()).fill(crate::theme::APP_COLOR);
                if ui.add_enabled(dirty, b).on_hover_text(util::shortcut_label("Mod+S")).clicked() {
                    save = true;
                }
                if dirty && ui.button("Revert").clicked() {
                    ed.text = ed.saved.clone();
                }
            });
        });
    });
    egui::ScrollArea::both().id_salt(("editor", &ed.path)).auto_shrink([false, false]).show(ui, |ui| {
        let lang = if app.session.view.syntax { crate::highlight::language(&ed.path) } else { None };
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
            let mut job = egui::text::LayoutJob::default();
            for line in text.as_str().split_inclusive('\n') {
                let body = line.trim_end_matches('\n');
                crate::highlight::append_line(&mut job, body, lang, &[], egui::Color32::TRANSPARENT, t.text, FontId::monospace(12.5), t);
                if line.ends_with('\n') {
                    job.append("\n", 0.0, egui::TextFormat { font_id: FontId::monospace(12.5), color: t.text, ..Default::default() });
                }
            }
            job.wrap.max_width = wrap;
            ui.painter().layout_job(job)
        };
        ui.add(
            egui::TextEdit::multiline(&mut ed.text)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(40)
                .lock_focus(true)
                .layouter(&mut layouter),
        );
    });
    if let Some((m, k)) = util::parse_shortcut("Mod+S")
        && ui.input_mut(|i| i.consume_key(m, k))
    {
        save = dirty;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !dirty {
        close = true;
    }
    if save && app.run("file.edit", json!({"path": ed.path, "content": ed.text})).is_ok() {
        ed.saved = ed.text.clone();
    }
    if close {
        if ed.text != ed.saved {
            app.status(format!("{} has unsaved changes: save or revert first", ed.path), true);
            app.editor = Some(ed);
        }
        return;
    }
    app.editor = Some(ed);
}
