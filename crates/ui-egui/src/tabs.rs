//! Repository tabs: several repositories open at once, one shown. The shown tab lives in
//! `GitApp`'s own fields (`session`, the line pick, caches); the others are parked here and
//! swapped in when picked.

use std::path::{Path, PathBuf};

use egui::{Frame, Margin, Sense, Stroke, pos2, vec2};
use gitcraft_engine::Session;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{GitApp, LinePick, Reword, merge_tool::MergeTool};

/// Most tabs open at once.
pub const MAX_TABS: usize = 32;

/// A tab's state while another one is shown.
pub struct Parked {
    pub session: Session,
    pub pick: LinePick,
    pub reword: Option<Reword>,
    pub ref_filter: String,
    pub merge: Option<MergeTool>,
}

/// The tab strip's state.
pub struct Tabs {
    /// One slot per tab, in order; the shown tab's slot is `None` (its state is in `GitApp`).
    pub slots: Vec<Option<Parked>>,
    pub active: usize,
    /// Each tab's id, for background jobs to find their tab again.
    pub ids: Vec<u64>,
    next_id: u64,
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs { slots: vec![None], active: 0, ids: vec![0], next_id: 1 }
    }
}

impl Tabs {
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    pub fn active_id(&self) -> u64 {
        self.ids.get(self.active).copied().unwrap_or(0)
    }
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|&x| x == id)
    }
}

fn repo_root(s: &Session) -> Option<PathBuf> {
    s.repo.as_ref().map(|r| r.root().to_path_buf())
}

fn title(s: &Session) -> String {
    s.repo.as_ref().map(|r| r.name()).unwrap_or_else(|| "New Tab".into())
}

impl GitApp {
    /// Show `p`'s state and return the shown tab's.
    fn swap_in(&mut self, p: Parked) -> Parked {
        let out = Parked {
            session: std::mem::replace(&mut self.session, p.session),
            pick: std::mem::replace(&mut self.pick, p.pick),
            reword: std::mem::replace(&mut self.reword, p.reword),
            ref_filter: std::mem::replace(&mut self.ref_filter, p.ref_filter),
            merge: std::mem::replace(&mut self.merge, p.merge),
        };
        self.blame = None;
        self.details = None;
        self.drop_menu = None;
        self.label_popup = None;
        self.scroll_to_selection = true;
        out
    }

    /// Show tab `i`.
    pub fn switch_tab(&mut self, i: usize) -> Result<(), String> {
        if i == self.tabs.active {
            return Ok(());
        }
        let p = self.tabs.slots.get_mut(i).and_then(Option::take).ok_or("no such tab")?;
        let out = self.swap_in(p);
        if let Some(slot) = self.tabs.slots.get_mut(self.tabs.active) {
            *slot = Some(out);
        }
        self.tabs.active = i;
        // Catch up with changes made while it was hidden.
        if let Err(e) = self.session.refresh_if_changed() {
            log::warn!("checking for changes: {e}");
        }
        self.sync_tab_prefs();
        Ok(())
    }

    /// Open a new empty tab (the start screen) and show it.
    pub fn new_tab(&mut self) -> Result<usize, String> {
        if self.tabs.len() >= MAX_TABS {
            return Err(format!("at most {MAX_TABS} tabs"));
        }
        let mut session = Session::new(self.session.git.clone());
        session.view.context = self.prefs.context_lines.min(100);
        session.view.commit_limit = self.prefs.commit_limit.clamp(100, 100_000);
        let out = self.swap_in(Parked { session, pick: LinePick::default(), reword: None, ref_filter: String::new(), merge: None });
        if let Some(slot) = self.tabs.slots.get_mut(self.tabs.active) {
            *slot = Some(out);
        }
        self.tabs.slots.push(None);
        self.tabs.ids.push(self.tabs.next_id);
        self.tabs.next_id = self.tabs.next_id.wrapping_add(1);
        self.tabs.active = self.tabs.len().saturating_sub(1);
        Ok(self.tabs.active)
    }

    /// Close tab `i`; closing the last one leaves an empty tab.
    pub fn close_tab(&mut self, i: usize) -> Result<(), String> {
        if i >= self.tabs.len() {
            return Err("no such tab".into());
        }
        if self.tabs.len() == 1 {
            self.session.close();
            self.merge = None;
            self.pick = LinePick::default();
            self.reword = None;
            self.sync_tab_prefs();
            return Ok(());
        }
        if i == self.tabs.active {
            // Show a neighbour first, then drop the (now parked) tab.
            let next = if i + 1 < self.tabs.len() { i + 1 } else { i.saturating_sub(1) };
            self.switch_tab(next)?;
        }
        self.tabs.slots.remove(i);
        self.tabs.ids.remove(i);
        if self.tabs.active > i {
            self.tabs.active -= 1;
        }
        self.sync_tab_prefs();
        Ok(())
    }

    /// The tab showing the repository at `root`.
    pub fn tab_with(&self, root: &Path) -> Option<usize> {
        (0..self.tabs.len()).find(|&i| self.tab_session(i).and_then(repo_root).as_deref() == Some(root))
    }

    pub fn tab_session(&self, i: usize) -> Option<&Session> {
        if i == self.tabs.active { Some(&self.session) } else { self.tabs.slots.get(i)?.as_ref().map(|p| &p.session) }
    }

    /// Each tab's title and repository folder.
    pub fn tab_list(&self) -> Vec<(String, Option<PathBuf>)> {
        (0..self.tabs.len()).filter_map(|i| self.tab_session(i)).map(|s| (title(s), repo_root(s))).collect()
    }

    /// Remember the open tabs for the next start.
    pub(crate) fn sync_tab_prefs(&mut self) {
        let list = self.tab_list();
        self.prefs.tabs = list.iter().filter_map(|(_, p)| p.as_ref().map(|p| p.display().to_string())).collect();
        // The shown tab's index among the tabs that have a repository.
        self.prefs.active_tab = list.iter().take(self.tabs.active).filter(|(_, p)| p.is_some()).count();
    }

    /// Open the repositories of the last run's tabs (the first is already open, or none is).
    pub fn restore_tabs(&mut self, paths: &[String], active: usize) {
        for p in paths {
            let path = Path::new(p);
            if !path.is_dir() {
                continue;
            }
            if let Ok(root) = self.session.git.open(path).map(|r| r.root().to_path_buf())
                && self.tab_with(&root).is_some()
            {
                continue;
            }
            if self.session.repo.is_some() && self.new_tab().is_err() {
                break;
            }
            if let Err(e) = self.session.open(path) {
                log::warn!("reopening {p}: {e}");
            }
        }
        // Drop an empty last tab left by a folder that failed to open.
        if self.tabs.len() > 1 && self.session.repo.is_none() {
            let _ = self.close_tab(self.tabs.active);
        }
        let shown = (0..self.tabs.len()).filter(|&i| self.tab_session(i).is_some_and(|s| s.repo.is_some())).nth(active);
        let _ = self.switch_tab(shown.unwrap_or(0));
        self.after_open();
        self.sync_tab_prefs();
    }

    /// `ui.tabs`: list, open, show or close tabs.
    pub(crate) fn tabs_command(&mut self, v: &Value) -> Result<Value, String> {
        let index = || v.get("index").and_then(Value::as_u64).map(|i| i as usize).ok_or("`index` is required");
        match v.get("action").and_then(Value::as_str).unwrap_or("list") {
            "list" => {}
            "new" => {
                self.new_tab()?;
            }
            "switch" => self.switch_tab(index()?)?,
            "next" | "previous" => {
                let n = self.tabs.len().max(1);
                let step = if v.get("action").and_then(Value::as_str) == Some("next") { 1 } else { n - 1 };
                self.switch_tab((self.tabs.active + step) % n)?;
            }
            "close" => {
                let i = v.get("index").and_then(Value::as_u64).map(|i| i as usize).unwrap_or(self.tabs.active);
                self.close_tab(i)?;
            }
            other => return Err(format!("`action` is list, new, switch, next, previous or close (not `{other}`)")),
        }
        let tabs: Vec<Value> = self.tab_list().into_iter().map(|(t, p)| json!({"title": t, "path": p.map(|p| p.display().to_string())})).collect();
        Ok(json!({"tabs": tabs, "active": self.tabs.active}))
    }
}

/// The tab strip above the toolbar.
pub fn show(app: &mut GitApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let mut action: Option<Value> = None;
    egui::Panel::top("tabs")
        .exact_size(32.0)
        .frame(Frame::NONE.fill(t.panel).inner_margin(Margin { left: 6, right: 6, top: 4, bottom: 0 }).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let list = app.tab_list();
                let n = list.len();
                let w = ((ui.available_width() - 34.0) / n.max(1) as f32).clamp(90.0, 220.0);
                for (i, (name, path)) in list.iter().enumerate() {
                    let active = i == app.tabs.active;
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
                    let p = ui.painter();
                    let fill = if active {
                        t.bg
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        t.panel
                    };
                    p.rect_filled(rect, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, fill);
                    if active {
                        p.line_segment([rect.left_top() + vec2(6.0, 0.0), rect.right_top() - vec2(6.0, 0.0)], Stroke::new(2.0, t.accent));
                    } else if i + 1 != app.tabs.active && i + 1 < n {
                        p.line_segment(
                            [pos2(rect.right() + 1.0, rect.top() + 7.0), pos2(rect.right() + 1.0, rect.bottom() - 7.0)],
                            Stroke::new(1.0, t.border),
                        );
                    }
                    let icon = egui::Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(14.0, 14.0));
                    icons::paint(p, icon, Icon::Folder, if active { t.text } else { t.dim }, t.accent);
                    let close = egui::Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(16.0, 16.0));
                    let text_rect = egui::Rect::from_min_max(pos2(icon.right() + 6.0, rect.top()), pos2(close.left() - 2.0, rect.bottom()));
                    p.with_clip_rect(text_rect).text(
                        pos2(text_rect.left(), rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(12.5),
                        if active { t.text } else { t.dim },
                    );
                    let cr = ui.interact(close, ui.id().with(("tab_close", i)), Sense::click());
                    if resp.hovered() || cr.hovered() || active {
                        if cr.hovered() {
                            ui.painter().rect_filled(close, 4.0, t.chrome);
                        }
                        icons::paint(ui.painter(), close.shrink(3.0), Icon::Close, t.dim, t.accent);
                    }
                    if cr.clicked() || resp.middle_clicked() {
                        action = Some(json!({"action": "close", "index": i}));
                    } else if resp.clicked() {
                        action = Some(json!({"action": "switch", "index": i}));
                    }
                    let tip = path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Open, clone or start a repository".into());
                    resp.on_hover_text(tip);
                }
                ui.add_space(4.0);
                if icons::button(ui, Icon::Plus, &format!("New tab ({})", crate::util::shortcut_label("Mod+T")), None, 15.0).clicked() {
                    action = Some(json!({"action": "new"}));
                }
            });
        });
    if let Some(a) = action
        && let Err(e) = app.tabs_command(&a)
    {
        app.status(e, true);
    }
}

#[cfg(test)]
mod tests {
    use crate::{GitApp, Prefs, Services};
    use gitcraft_engine::Session;
    use gitcraft_git::testing::sample;
    use serde_json::json;

    #[test]
    fn tabs_keep_their_own_repository_and_state() {
        let (_d1, r1) = sample("tabs-a").unwrap();
        let (_d2, r2) = sample("tabs-b").unwrap();
        let mut s = Session::new(r1.git().clone());
        s.set_repo(r1.clone()).unwrap();
        let mut app = GitApp::new(s, Prefs::default(), Services::default());
        app.ref_filter = "main".into();
        assert_eq!(app.tabs.len(), 1);

        // Opening a second repository while one is shown opens a tab.
        app.open_repo(r2.root()).unwrap();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tabs.active, 1);
        assert_eq!(app.session.repo.as_ref().unwrap().root(), r2.root());
        assert_eq!(app.ref_filter, "");
        assert_eq!(app.prefs.tabs.len(), 2);

        // Back to the first: its state came back.
        app.tabs_command(&json!({"action": "switch", "index": 0})).unwrap();
        assert_eq!(app.session.repo.as_ref().unwrap().root(), r1.root());
        assert_eq!(app.ref_filter, "main");
        // Opening a repository that has a tab shows that tab.
        app.open_repo(r2.root()).unwrap();
        assert_eq!((app.tabs.len(), app.tabs.active), (2, 1));

        // New empty tab, next / previous, closing.
        let v = app.tabs_command(&json!({"action": "new"})).unwrap();
        assert_eq!(v["active"], json!(2));
        assert!(app.session.repo.is_none());
        app.tabs_command(&json!({"action": "next"})).unwrap();
        assert_eq!(app.tabs.active, 0);
        app.tabs_command(&json!({"action": "previous"})).unwrap();
        assert_eq!(app.tabs.active, 2);
        app.tabs_command(&json!({"action": "close"})).unwrap();
        assert_eq!((app.tabs.len(), app.tabs.active), (2, 1));
        app.tabs_command(&json!({"action": "close", "index": 0})).unwrap();
        assert_eq!((app.tabs.len(), app.tabs.active), (1, 0));
        assert_eq!(app.session.repo.as_ref().unwrap().root(), r2.root());
        // The last tab closes to the start screen.
        app.tabs_command(&json!({"action": "close"})).unwrap();
        assert_eq!(app.tabs.len(), 1);
        assert!(app.session.repo.is_none());
        // Junk.
        assert!(app.tabs_command(&json!({"action": "switch", "index": 99})).is_err());
        assert!(app.tabs_command(&json!({"action": "close", "index": 99})).is_err());
        assert!(app.tabs_command(&json!({"action": "nope"})).is_err());
        assert!(app.tabs_command(&json!({"action": "switch"})).is_err());

        // Restoring the last run's tabs.
        app.restore_tabs(&[r1.root().display().to_string(), "/no/such/folder".into(), r2.root().display().to_string()], 1);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.session.repo.as_ref().unwrap().root(), r2.root());
    }
}
