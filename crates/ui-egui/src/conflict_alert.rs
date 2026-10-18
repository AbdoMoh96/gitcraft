//! Conflict warnings before they happen: in the background, after every reload, the current
//! branch is merged in memory with its upstream (when behind) and the target branch
//! (`conflict.predict`); when either would conflict, the toolbar shows an alert listing the files.

use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Sense, Stroke, vec2};
use gitcraft_engine::Session;
use serde_json::{Value, json};

use crate::GitApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// The background check and its last result.
#[derive(Default)]
pub struct ConflictAlert {
    /// What the result is for: the repository and its reload generation.
    key: Option<(std::path::PathBuf, u64)>,
    rx: Option<Receiver<Result<Value, String>>>,
    pub result: Option<Value>,
}

impl ConflictAlert {
    /// A check is running.
    pub fn pending(&self) -> bool {
        self.rx.is_some()
    }
}

/// One branch the current one would conflict with.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub target: String,
    pub upstream: bool,
    pub files: Vec<String>,
}

/// The conflicting checks of a `conflict.predict` result.
pub fn warnings(v: &Value) -> Vec<Warning> {
    let Some(checks) = v.get("checks").and_then(Value::as_array) else { return Vec::new() };
    checks
        .iter()
        .filter(|c| c.get("conflicts").and_then(Value::as_bool).unwrap_or(false))
        .map(|c| Warning {
            target: c.get("target").and_then(Value::as_str).unwrap_or("").to_string(),
            upstream: c.get("why").and_then(Value::as_str) == Some("upstream"),
            files: c
                .get("files")
                .and_then(Value::as_array)
                .map(|f| f.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
        })
        .collect()
}

impl GitApp {
    /// Start a check when the repository changed since the last one; collect a finished one.
    pub(crate) fn poll_conflict_alert(&mut self) {
        if let Some(rx) = &self.conflict_alert.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.conflict_alert.result = r.ok();
                    self.conflict_alert.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.conflict_alert.rx = None,
            }
        }
        let Some(repo) = self.session.repo.clone().filter(|_| self.prefs.conflict_check) else {
            self.conflict_alert = ConflictAlert::default();
            return;
        };
        // Mid-merge the conflicts are real, not predicted.
        if self.session.data.status.state != gitcraft_engine::git::RepoState::Clean {
            self.conflict_alert.result = None;
            return;
        }
        let key = Some((repo.root().to_path_buf(), self.session.generation()));
        if key == self.conflict_alert.key {
            return;
        }
        if self.conflict_alert.key.as_ref().map(|k| &k.0) != key.as_ref().map(|k| &k.0) {
            // Another repository: the old result says nothing about this one.
            self.conflict_alert.result = None;
        }
        self.conflict_alert.key = key;
        let (tx, rx) = channel();
        let (status, refs) = (self.session.data.status.clone(), self.session.data.refs.clone());
        let started = std::thread::Builder::new().name("gitcraft conflict check".into()).spawn(move || {
            let mut s = Session::detached(repo);
            s.data.status = status;
            s.data.refs = refs;
            let _ = tx.send(s.run("conflict.predict", &json!({})).map_err(|e| e.to_string()));
        });
        match started {
            Ok(_) => self.conflict_alert.rx = Some(rx),
            Err(e) => log::warn!("conflict check: {e}"),
        }
    }

    /// The current warnings (empty while none).
    pub fn conflict_warnings(&self) -> Vec<Warning> {
        self.conflict_alert.result.as_ref().map(warnings).unwrap_or_default()
    }
}

/// The toolbar alert: shown only while a merge would conflict; a click lists the files.
pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let ws = app.conflict_warnings();
    if ws.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let branch = app.session.data.status.branch.clone().unwrap_or_else(|| "HEAD".into());
    let label = match ws.as_slice() {
        [w] => format!("Conflicts with {}", w.target),
        _ => format!("Conflicts with {} branches", ws.len()),
    };
    let text_w = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), egui::FontId::proportional(12.5), t.warning).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(text_w + 40.0, 28.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, t.warning_bg);
    p.rect_stroke(rect, 6.0, Stroke::new(if resp.hovered() { 1.5 } else { 1.0 }, t.warning), egui::StrokeKind::Inside);
    icons::paint(p, egui::Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), vec2(15.0, 15.0)), Icon::Warning, t.warning, t.warning);
    p.text(rect.left_center() + vec2(28.0, 0.0), egui::Align2::LEFT_CENTER, &label, egui::FontId::proportional(12.5), t.warning);
    let resp = resp.on_hover_text("Merging would stop with conflicts. Click for the files.");
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(300.0);
        ui.set_max_width(460.0);
        for w in &ws {
            let what = if w.upstream { format!("Pulling {} into {branch}", w.target) } else { format!("Merging {} into {branch}", w.target) };
            ui.label(
                RichText::new(format!("{what} would conflict in {} file{}:", w.files.len(), if w.files.len() == 1 { "" } else { "s" })).strong(),
            );
            egui::ScrollArea::vertical().id_salt(("conflict_files", &w.target)).max_height(180.0).show(ui, |ui| {
                for f in &w.files {
                    ui.label(RichText::new(format!("  {f}")).monospace().color(t.warning));
                }
            });
            ui.add_space(4.0);
        }
        ui.label(RichText::new("Checked in memory: nothing in your work tree was changed.").small().weak());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_only_for_conflicting_checks() {
        let v = json!({"checks": [
            {"target": "origin/main", "why": "target", "conflicts": true, "files": ["a", "b"]},
            {"target": "origin/feature", "why": "upstream", "conflicts": false, "files": []},
        ]});
        assert_eq!(warnings(&v), vec![Warning { target: "origin/main".into(), upstream: false, files: vec!["a".into(), "b".into()] }]);
        assert!(warnings(&json!({})).is_empty());
        assert!(warnings(&json!({"checks": [{"conflicts": true, "files": [1, null]}]}))[0].files.is_empty());
    }

    #[test]
    fn background_check_finds_a_conflict_with_the_target() {
        let (_d, repo) = gitcraft_git::testing::sample("alert").unwrap();
        // main and feature both change README.md; check from feature against main.
        gitcraft_git::testing::write(&repo, "README.md", "main side\n").unwrap();
        gitcraft_git::testing::commit_all(&repo, "main readme").unwrap();
        repo.checkout_branch("feature").unwrap();
        gitcraft_git::testing::write(&repo, "README.md", "feature side\n").unwrap();
        gitcraft_git::testing::commit_all(&repo, "feature readme").unwrap();
        let mut s = Session::new(repo.git().clone());
        s.set_repo(repo).unwrap();
        let mut app = GitApp::new(s, crate::Prefs::default(), crate::Services::default());
        for _ in 0..200 {
            app.poll_conflict_alert();
            if app.conflict_alert.result.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let r = app.conflict_alert.result.clone().unwrap();
        if r["supported"] == json!(true) {
            let w = app.conflict_warnings();
            assert_eq!(w.len(), 1);
            assert_eq!(w[0].target, "main");
            assert_eq!(w[0].files, vec!["README.md".to_string()]);
        }
        // Turned off: no alert.
        app.prefs.conflict_check = false;
        app.poll_conflict_alert();
        assert!(app.conflict_warnings().is_empty());
    }
}
