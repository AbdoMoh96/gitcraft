//! Dialogs: the names and choices a command needs, and confirmations for destructive ones.
//! Enter submits, Escape (or a click outside) cancels.

use std::path::PathBuf;

use egui::{Id, RichText};
use gitcraft_engine::git::TodoAction;
use gitcraft_engine::{Selected, Session};
use serde_json::{Value, json};

use crate::theme::{Appearance, Tokens};
use crate::{GitApp, util};

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Open {
        path: String,
    },
    Clone {
        url: String,
        into: String,
    },
    Init {
        path: String,
        branch: String,
    },
    CreateBranch {
        name: String,
        from: Option<String>,
        checkout: bool,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    CreateTag {
        name: String,
        oid: Option<String>,
        message: String,
    },
    AddRemote {
        name: String,
        url: String,
    },
    Stash {
        message: String,
        untracked: bool,
    },
    Confirm {
        title: String,
        body: String,
        command: String,
        params: Value,
        action: String,
    },
    /// A local branch and its remote one both have new commits: how to combine them.
    Diverged {
        local: String,
        remote: String,
        ahead: u64,
        behind: u64,
        /// Local commits whose change isn't on the remote already (merges aside).
        unique: u64,
    },
    /// The todo list of an interactive rebase, oldest commit first.
    Rebase {
        oid: String,
        rows: Vec<RebaseRow>,
    },
    AddSubmodule {
        url: String,
        path: String,
    },
    /// Save a commit (or the uncommitted changes) as a patch file.
    SavePatch {
        oid: Option<String>,
        path: String,
    },
    ApplyPatch {
        path: String,
        commit: bool,
    },
    /// Git Flow: set it up, start and finish feature, release and hotfix branches.
    GitFlow {
        kind: String,
        name: String,
        main: String,
        develop: String,
    },
    Lfs {
        pattern: String,
        /// `lfs.list`'s answer (asked again after each change).
        info: Value,
    },
    Hooks {
        selected: String,
        script: String,
    },
    AddWorktree {
        path: String,
        branch: String,
        new_branch: bool,
    },
    /// The author identity for this repository and for every repository.
    Identity {
        local_name: String,
        local_email: String,
        global_name: String,
        global_email: String,
    },
    /// How commits are signed.
    Signing {
        global: bool,
        format: String,
        key: String,
        sign_commits: bool,
        sign_tags: bool,
    },
    /// The user's SSH keys, and making a new one.
    SshKeys {
        keys: Vec<(String, String)>,
        name: String,
        comment: String,
        passphrase: String,
    },
    /// People credited as co-authors of the next commit.
    CoAuthors {
        known: Vec<String>,
        chosen: Vec<String>,
        custom: String,
    },
    Shortcuts,
    Preferences,
}

/// One commit in the interactive rebase dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct RebaseRow {
    pub action: TodoAction,
    pub oid: String,
    pub summary: String,
    /// The new message (reword, or a squash group's).
    pub message: String,
}

fn home() -> String {
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default()
}

impl Dialog {
    /// The dialog for a command id (or `shortcuts` / `preferences`), filled from the selection
    /// and `v` (`{"from": …}`, `{"oid": …}`, `{"name": …}`).
    pub fn named(name: &str, s: &Session, v: &Value) -> Option<Dialog> {
        let st = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let selected = match &s.sel.row {
            Selected::Commit(o) => Some(o.clone()),
            _ => None,
        };
        Some(match name {
            "repo.open" => Dialog::Open { path: st("path").unwrap_or_default() },
            "repo.clone" => Dialog::Clone { url: st("url").unwrap_or_default(), into: st("into").unwrap_or_else(home) },
            "repo.init" => Dialog::Init { path: st("path").unwrap_or_default(), branch: "main".into() },
            "branch.create" => Dialog::CreateBranch { name: st("name").unwrap_or_default(), from: st("from").or(selected), checkout: true },
            "branch.rename" => {
                let from = st("from").or_else(|| s.data.status.branch.clone())?;
                Dialog::RenameBranch { to: from.clone(), from }
            }
            "tag.create" => Dialog::CreateTag { name: String::new(), oid: st("oid").or(selected), message: String::new() },
            "remote.add" => Dialog::AddRemote { name: if s.data.remotes.is_empty() { "origin".into() } else { String::new() }, url: String::new() },
            "stash.save" => Dialog::Stash { message: String::new(), untracked: true },
            "diverged" => Dialog::Diverged {
                local: st("local")?,
                remote: st("remote")?,
                ahead: v.get("ahead").and_then(Value::as_u64).unwrap_or(0),
                behind: v.get("behind").and_then(Value::as_u64).unwrap_or(0),
                unique: v.get("unique").and_then(Value::as_u64).unwrap_or(1),
            },
            "rebase.interactive" => {
                let oid = st("oid").or(selected)?;
                let (_, commits) = s.repo.as_ref()?.rebase_plan(&oid).ok()?;
                let rows = commits
                    .into_iter()
                    .map(|c| RebaseRow { action: TodoAction::Pick, message: c.message(), summary: c.summary, oid: c.oid })
                    .collect();
                Dialog::Rebase { oid, rows }
            }
            "patch.create" => {
                let oid = st("oid").or(selected);
                let short = oid.as_deref().map(|o| o.get(..7).unwrap_or(o).to_string()).unwrap_or_else(|| "changes".into());
                let dir = s.repo.as_ref().and_then(|r| r.root().parent().map(|p| p.to_path_buf())).unwrap_or_default();
                Dialog::SavePatch { path: dir.join(format!("{short}.patch")).display().to_string(), oid }
            }
            "patch.apply" => Dialog::ApplyPatch { path: st("path").unwrap_or_default(), commit: true },
            "gitflow.init" => Dialog::GitFlow {
                kind: "feature".into(),
                name: String::new(),
                main: s.data.setting("gitflow.branch.master").map(str::to_string).unwrap_or_else(|| {
                    if s.data.local_branches().any(|r| r.name == "master") && !s.data.local_branches().any(|r| r.name == "main") {
                        "master".into()
                    } else {
                        "main".into()
                    }
                }),
                develop: s.data.setting("gitflow.branch.develop").unwrap_or("develop").to_string(),
            },
            "lfs.list" => {
                let r = s.repo.as_ref()?;
                let available = r.lfs_available();
                Dialog::Lfs {
                    pattern: String::new(),
                    info: json!({"available": available, "patterns": r.lfs_patterns(), "files": if available { r.lfs_files().unwrap_or_default() } else { Vec::new() }}),
                }
            }
            "hooks.list" => Dialog::Hooks { selected: String::new(), script: String::new() },
            "submodule.add" => Dialog::AddSubmodule { url: st("url").unwrap_or_default(), path: st("path").unwrap_or_default() },
            "worktree.add" => {
                Dialog::AddWorktree { path: st("path").unwrap_or_default(), branch: st("branch").unwrap_or_default(), new_branch: true }
            }
            "config.identity" => {
                let r = s.repo.as_ref()?;
                let get = |sc, k| r.config_in(sc, k).unwrap_or_default();
                use gitcraft_engine::git::ConfigScope::{Global, Local};
                Dialog::Identity {
                    local_name: get(Local, "user.name"),
                    local_email: get(Local, "user.email"),
                    global_name: get(Global, "user.name"),
                    global_email: get(Global, "user.email"),
                }
            }
            "config.signing" => {
                let r = s.repo.as_ref()?;
                Dialog::Signing {
                    global: true,
                    format: r.config("gpg.format").unwrap_or_else(|| "openpgp".into()),
                    key: r.config("user.signingkey").unwrap_or_default(),
                    sign_commits: r.config("commit.gpgsign").is_some_and(|x| x == "true"),
                    sign_tags: r.config("tag.gpgsign").is_some_and(|x| x == "true"),
                }
            }
            "ssh.keys" => Dialog::SshKeys {
                keys: gitcraft_engine::git::ssh::ssh_dir()
                    .map(|d| gitcraft_engine::git::ssh::list_keys(&d).into_iter().map(|k| (k.path, k.public)).collect())
                    .unwrap_or_default(),
                name: "id_ed25519".into(),
                comment: s.repo.as_ref().and_then(|r| r.config("user.email")).unwrap_or_default(),
                passphrase: String::new(),
            },
            "commit.coAuthors" => {
                let me = s.repo.as_ref().and_then(|r| r.config("user.email")).unwrap_or_default();
                let mut known: Vec<String> = Vec::new();
                for c in s.data.commits.iter().take(5000) {
                    let a = format!("{} <{}>", c.author.name, c.author.email);
                    if c.author.email != me && !known.contains(&a) && known.len() < 200 {
                        known.push(a);
                    }
                }
                Dialog::CoAuthors { known, chosen: s.draft.co_authors.clone(), custom: String::new() }
            }
            "shortcuts" | "ui.shortcuts" => Dialog::Shortcuts,
            "preferences" | "ui.preferences" => Dialog::Preferences,
            _ => return None,
        })
    }

    pub fn confirm(title: &str, body: &str, command: &str, params: Value, action: &str) -> Dialog {
        Dialog::Confirm { title: title.into(), body: body.into(), command: command.into(), params, action: action.into() }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Dialog::Open { .. } => "repo.open",
            Dialog::Clone { .. } => "repo.clone",
            Dialog::Init { .. } => "repo.init",
            Dialog::CreateBranch { .. } => "branch.create",
            Dialog::RenameBranch { .. } => "branch.rename",
            Dialog::CreateTag { .. } => "tag.create",
            Dialog::AddRemote { .. } => "remote.add",
            Dialog::Stash { .. } => "stash.save",
            Dialog::Confirm { .. } => "confirm",
            Dialog::Diverged { .. } => "diverged",
            Dialog::Rebase { .. } => "rebase.interactive",
            Dialog::AddSubmodule { .. } => "submodule.add",
            Dialog::SavePatch { .. } => "patch.create",
            Dialog::ApplyPatch { .. } => "patch.apply",
            Dialog::GitFlow { .. } => "gitflow.init",
            Dialog::Lfs { .. } => "lfs.list",
            Dialog::Hooks { .. } => "hooks.list",
            Dialog::AddWorktree { .. } => "worktree.add",
            Dialog::Identity { .. } => "config.identity",
            Dialog::Signing { .. } => "config.signing",
            Dialog::SshKeys { .. } => "ssh.keys",
            Dialog::CoAuthors { .. } => "commit.coAuthors",
            Dialog::Shortcuts => "shortcuts",
            Dialog::Preferences => "preferences",
        }
    }
}

/// What the dialog asks for when it closes.
enum Close {
    Keep,
    Cancel,
    Run(String, Value),
    Open(PathBuf),
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, focus: bool) -> egui::Response {
    ui.label(RichText::new(label).small().strong());
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
    if focus {
        r.request_focus();
    }
    ui.add_space(4.0);
    r
}

fn buttons(ui: &mut egui::Ui, action: &str, ok: bool, danger: bool) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    let mut submit = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fill = if danger { t.danger } else { crate::theme::APP_COLOR };
            let b = egui::Button::new(RichText::new(action).color(egui::Color32::WHITE).strong()).fill(fill);
            if ui.add_enabled(ok, b).clicked() {
                submit = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    let enter = ok && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    (submit || enter, cancel)
}

pub fn show(app: &mut GitApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let first = !ctx.memory(|m| m.data.get_temp::<bool>(Id::new("dialog_seen")).unwrap_or(false));
    ctx.memory_mut(|m| m.data.insert_temp(Id::new("dialog_seen"), true));
    let modal = egui::Modal::new(Id::new("gitcraft_dialog")).show(ctx, |ui| {
        ui.set_width(440.0);
        body(app, ui, &mut d, first)
    });
    let close = if modal.should_close() { Close::Cancel } else { modal.inner };
    match close {
        Close::Keep => app.dialog = Some(d),
        Close::Cancel => {}
        Close::Run(id, p) => {
            let _ = app.run(&id, p);
        }
        Close::Open(p) => {
            let _ = app.open_repo(&p);
        }
    }
    if app.dialog.is_none() {
        ctx.memory_mut(|m| m.data.remove::<bool>(Id::new("dialog_seen")));
    }
}

fn body(app: &mut GitApp, ui: &mut egui::Ui, d: &mut Dialog, first: bool) -> Close {
    let t = Tokens::get(ui.ctx());
    match d {
        Dialog::Open { path } => {
            ui.heading("Open Repository");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(path).hint_text("Folder inside a git repository").desired_width(330.0));
                if first {
                    r.request_focus();
                }
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Open a repository")
                {
                    *path = p.display().to_string();
                }
            });
            if !app.prefs.recent.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("RECENT").small().weak());
                for p in app.prefs.recent.clone().iter().take(6) {
                    if ui.link(p).clicked() {
                        return Close::Open(PathBuf::from(p));
                    }
                }
            }
            let (ok, cancel) = buttons(ui, "Open", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Open(PathBuf::from(path.trim()));
            }
        }
        Dialog::Clone { url, into } => {
            ui.heading("Clone Repository");
            ui.add_space(6.0);
            field(ui, "URL", url, "https://host/owner/repo.git or git@host:owner/repo.git", first);
            ui.label(RichText::new("WHERE").small().strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(into).desired_width(330.0));
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Clone into")
                {
                    *into = p.display().to_string();
                }
            });
            let name = url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".git").to_string();
            if !name.is_empty() {
                ui.label(RichText::new(format!("Clones into {}", PathBuf::from(into.as_str()).join(&name).display())).small().weak());
            }
            let (ok, cancel) = buttons(ui, "Clone", !url.trim().is_empty() && !name.is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run(
                    "repo.clone".into(),
                    json!({"url": url.trim(), "path": PathBuf::from(into.as_str()).join(&name).display().to_string()}),
                );
            }
        }
        Dialog::Init { path, branch } => {
            ui.heading("Init Repository");
            ui.add_space(6.0);
            ui.label(RichText::new("FOLDER").small().strong());
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(path).hint_text("New or existing folder").desired_width(330.0));
                if first {
                    r.request_focus();
                }
                if let Some(pick) = &app.services.pick_folder
                    && ui.button("Browse…").clicked()
                    && let Some(p) = pick("Init a repository in")
                {
                    *path = p.display().to_string();
                }
            });
            field(ui, "INITIAL BRANCH", branch, "main", false);
            let (ok, cancel) = buttons(ui, "Create Repository", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("repo.init".into(), json!({"path": path.trim(), "branch": branch.trim()}));
            }
        }
        Dialog::CreateBranch { name, from, checkout } => {
            ui.heading("Create Branch");
            ui.add_space(6.0);
            field(ui, "NAME", name, "feature/my-change", first);
            let at = from.as_deref().map(|f| f.get(..7).unwrap_or(f).to_string()).unwrap_or_else(|| "HEAD".into());
            ui.label(RichText::new(format!("Starts at {at}")).small().weak());
            ui.checkbox(checkout, "Check it out");
            let (ok, cancel) = buttons(ui, "Create Branch", !name.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("branch.create".into(), json!({"name": name.trim(), "from": from, "checkout": *checkout}));
            }
        }
        Dialog::RenameBranch { from, to } => {
            ui.heading(format!("Rename {from}"));
            ui.add_space(6.0);
            field(ui, "NEW NAME", to, "", first);
            let (ok, cancel) = buttons(ui, "Rename", !to.trim().is_empty() && to.trim() != from, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("branch.rename".into(), json!({"from": from, "to": to.trim()}));
            }
        }
        Dialog::CreateTag { name, oid, message } => {
            ui.heading("Create Tag");
            ui.add_space(6.0);
            field(ui, "NAME", name, "v1.0.0", first);
            ui.label(RichText::new("MESSAGE (an annotated tag; leave empty for a lightweight one)").small().strong());
            ui.add(egui::TextEdit::multiline(message).desired_rows(3).desired_width(f32::INFINITY));
            let at = oid.as_deref().map(|f| f.get(..7).unwrap_or(f).to_string()).unwrap_or_else(|| "HEAD".into());
            ui.label(RichText::new(format!("Tags {at}")).small().weak());
            let (ok, cancel) = buttons(ui, "Create Tag", !name.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("tag.create".into(), json!({"name": name.trim(), "oid": oid, "message": message}));
            }
        }
        Dialog::AddRemote { name, url } => {
            ui.heading("Add Remote");
            ui.add_space(6.0);
            field(ui, "NAME", name, "origin", first);
            field(ui, "URL", url, "https://host/owner/repo.git", false);
            let (ok, cancel) = buttons(ui, "Add Remote", !name.trim().is_empty() && !url.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("remote.add".into(), json!({"name": name.trim(), "url": url.trim()}));
            }
        }
        Dialog::Stash { message, untracked } => {
            ui.heading("Stash Changes");
            ui.add_space(6.0);
            field(ui, "MESSAGE", message, "optional", first);
            ui.checkbox(untracked, "Include untracked files");
            let (ok, cancel) = buttons(ui, "Stash", true, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("stash.save".into(), json!({"message": message, "untracked": *untracked}));
            }
        }
        Dialog::Confirm { title, body, command, params, action } => {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                crate::icons::paint(ui.painter(), r, crate::icons::Icon::Warning, t.warning, t.warning);
                ui.heading(title.as_str());
            });
            ui.add_space(6.0);
            ui.label(body.as_str());
            let (ok, cancel) = buttons(ui, action, true, true);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run(command.clone(), params.clone());
            }
        }
        Dialog::Diverged { local, remote, ahead, behind, unique } => {
            let plural = |n: u64| if n == 1 { "commit" } else { "commits" };
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                crate::icons::paint(ui.painter(), r, crate::icons::Icon::Pull, t.accent, t.accent);
                ui.heading(format!("{local} has diverged from {remote}"));
            });
            ui.add_space(6.0);
            ui.label(format!(
                "{local} has {ahead} {} that {remote} doesn't, and {remote} has {behind} {} that {local} doesn't. How should they be combined?",
                plural(*ahead),
                plural(*behind)
            ));
            // Every local change is on the remote already (its history was rewritten): a merge
            // would duplicate them, so rebasing (which skips them) comes first.
            let rewritten = *unique == 0;
            if rewritten {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("All of {local}'s changes are already on {remote} (its history was rewritten). Rebasing or resetting gives you {remote}'s history; merging would duplicate the commits.")).color(t.warning));
            }
            ui.add_space(10.0);
            let w = ui.available_width();
            let choice = |ui: &mut egui::Ui, label: String, note: String, fill: Option<egui::Color32>| -> bool {
                let text = RichText::new(label).strong().color(if fill.is_some() { egui::Color32::WHITE } else { t.text });
                let mut b = egui::Button::new(text).min_size(egui::vec2(w, 30.0));
                if let Some(f) = fill {
                    b = b.fill(f);
                }
                let r = ui.add(b);
                ui.label(RichText::new(note).small().color(t.dim));
                ui.add_space(6.0);
                r.clicked()
            };
            let params = |mode: &str| json!({"name": remote.clone(), "mode": mode});
            let primary = Some(crate::theme::APP_COLOR);
            let rebase_note = if rewritten {
                format!("Your commits are already on {remote}: the branch becomes {remote}.")
            } else {
                format!("Replays your {unique} {} on top of {remote}.", plural(*unique))
            };
            if rewritten && choice(ui, format!("Rebase {local} onto {remote}"), rebase_note.clone(), primary) {
                return Close::Run("branch.pullRemote".into(), params("rebase"));
            }
            let merge_fill = if rewritten { None } else { primary };
            if choice(ui, format!("Merge {remote} into {local}"), "Keeps both histories with a merge commit (like git pull).".into(), merge_fill) {
                return Close::Run("branch.pullRemote".into(), params("merge"));
            }
            if !rewritten && choice(ui, format!("Rebase {local} onto {remote}"), rebase_note, None) {
                return Close::Run("branch.pullRemote".into(), params("rebase"));
            }
            if choice(
                ui,
                format!("Reset {local} to {remote}"),
                format!("Drops your {ahead} local {} from the branch (uncommitted changes are kept).", plural(*ahead)),
                Some(t.danger),
            ) {
                return Close::Run("branch.pullRemote".into(), params("reset"));
            }
            let cancel = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.button("Cancel").clicked()).inner;
            if cancel || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                return Close::Cancel;
            }
        }
        Dialog::Rebase { oid, rows } => return rebase_body(ui, oid, rows, t),
        Dialog::AddSubmodule { url, path } => {
            ui.heading("Add Submodule");
            ui.add_space(6.0);
            field(ui, "URL", url, "https://host/owner/library.git", first);
            if path.is_empty() {
                let guess = url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".git").to_string();
                ui.label(RichText::new(format!("Folder: {guess}")).small().weak());
            }
            field(ui, "FOLDER IN THE REPOSITORY", path, "vendor/library", false);
            let target = if path.trim().is_empty() {
                url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").trim_end_matches(".git").to_string()
            } else {
                path.trim().to_string()
            };
            let (ok, cancel) = buttons(ui, "Add Submodule", !url.trim().is_empty() && !target.is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("submodule.add".into(), json!({"url": url.trim(), "path": target}));
            }
        }
        Dialog::SavePatch { oid, path } => {
            ui.heading("Create Patch");
            let what = match oid {
                Some(o) => format!("Commit {} as a mail patch (author and message kept)", o.get(..7).unwrap_or(o)),
                None => "The uncommitted changes".to_string(),
            };
            ui.label(RichText::new(what).small().color(t.dim));
            ui.add_space(6.0);
            field(ui, "SAVE TO", path, "/path/to/change.patch", first);
            ui.horizontal(|ui| {
                if ui.button("Copy to Clipboard Instead").clicked()
                    && let Ok(v) = app.session.run("patch.create", &json!({"oid": oid}))
                    && let Some(p) = v["patch"].as_str()
                {
                    app.copy(p.to_string());
                    app.status("Copied the patch", false);
                }
            });
            let (ok, cancel) = buttons(ui, "Save Patch", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("patch.create".into(), json!({"oid": oid, "path": path.trim()}));
            }
        }
        Dialog::ApplyPatch { path, commit } => {
            ui.heading("Apply Patch");
            ui.add_space(6.0);
            field(ui, "PATCH FILE", path, "/path/to/change.patch", first);
            ui.checkbox(commit, "Commit mail patches with their author and message (git am)");
            let (ok, cancel) = buttons(ui, "Apply", !path.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("patch.apply".into(), json!({"path": path.trim(), "commit": *commit}));
            }
        }
        Dialog::GitFlow { kind, name, main, develop } => {
            ui.heading("Git Flow");
            ui.add_space(6.0);
            let ready = app.session.data.setting("gitflow.branch.develop").is_some();
            if !ready {
                ui.label("Feature branches start from a development branch; releases and hotfixes are merged into the main branch and tagged.");
                field(ui, "MAIN BRANCH (releases)", main, "main", first);
                field(ui, "DEVELOPMENT BRANCH (created when missing)", develop, "develop", false);
                let (ok, cancel) = buttons(ui, "Set Up Git Flow", !main.trim().is_empty() && !develop.trim().is_empty(), false);
                if cancel {
                    return Close::Cancel;
                }
                if ok {
                    let _ = app.run("gitflow.init", json!({"main": main.trim(), "develop": develop.trim()}));
                }
                return Close::Keep;
            }
            ui.label(
                RichText::new(format!("main: {}   develop: {}", app.session.data.setting("gitflow.branch.master").unwrap_or("main"), develop))
                    .small()
                    .color(t.dim),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for k in ["feature", "release", "hotfix"] {
                    ui.radio_value(kind, k.to_string(), k);
                }
            });
            field(ui, "NAME", name, if kind == "feature" { "login-page" } else { "1.2.0" }, false);
            let current = app.session.data.status.branch.clone().unwrap_or_default();
            let prefix = app.session.data.setting(&format!("gitflow.prefix.{kind}")).unwrap_or("").to_string();
            let on_one = !prefix.is_empty() && current.starts_with(&prefix);
            ui.horizontal(|ui| {
                if ui.add_enabled(!name.trim().is_empty(), egui::Button::new(format!("Start {kind}"))).clicked() {
                    let _ = app.run("gitflow.start", json!({"kind": kind, "name": name.trim()}));
                }
                let finish_name = if name.trim().is_empty() && on_one { current.clone() } else { name.trim().to_string() };
                if ui
                    .add_enabled(!finish_name.is_empty(), egui::Button::new(format!("Finish {kind}")))
                    .on_hover_text(if kind == "feature" {
                        "Merge into develop and delete the branch"
                    } else {
                        "Merge into main, tag it, merge back into develop, delete the branch"
                    })
                    .clicked()
                {
                    let _ = app.run("gitflow.finish", json!({"kind": kind, "name": finish_name}));
                }
            });
            if on_one {
                ui.label(RichText::new(format!("{current} is checked out: Finish uses it when the name is empty.")).small().color(t.dim));
            }
            if buttons(ui, "Close", true, false) != (false, false) {
                return Close::Cancel;
            }
        }
        Dialog::Lfs { pattern, info } => {
            ui.heading("Git LFS");
            ui.add_space(6.0);
            let mut changed = false;
            if info["available"] != json!(true) {
                ui.label(RichText::new("git lfs isn't installed: get it from git-lfs.com, then come back.").color(t.warning));
            }
            ui.label(RichText::new("STORED WITH LFS").small().strong());
            let pats: Vec<String> =
                info["patterns"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if pats.is_empty() {
                ui.label(RichText::new("No patterns yet").weak());
            }
            for pat in pats {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&pat).monospace());
                    if ui.small_button("Stop").clicked() {
                        let _ = app.run("lfs.untrack", json!({"pattern": pat}));
                        changed = true;
                    }
                });
            }
            if changed && let Some(Dialog::Lfs { info: fresh, .. }) = Dialog::named("lfs.list", &app.session, &json!({})) {
                *info = fresh;
            }
            let n = info["files"].as_array().map(Vec::len).unwrap_or(0);
            ui.label(RichText::new(format!("{n} file{} in LFS", if n == 1 { "" } else { "s" })).small().color(t.dim));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(pattern).hint_text("*.psd").desired_width(200.0));
                if ui.add_enabled(!pattern.trim().is_empty(), egui::Button::new("Track")).clicked() {
                    let _ = app.run("lfs.track", json!({"pattern": pattern.trim()}));
                    pattern.clear();
                    changed = true;
                }
                if ui.button("Pull LFS Files").clicked() {
                    let _ = app.run("lfs.pull", json!({}));
                }
            });
            if buttons(ui, "Close", true, false) != (false, false) {
                return Close::Cancel;
            }
        }
        Dialog::Hooks { selected, script } => {
            ui.set_width(560.0);
            ui.heading("Git Hooks");
            ui.label(RichText::new("Scripts git runs at points like committing or pushing.").small().color(t.dim));
            ui.add_space(6.0);
            let hooks = app.session.repo.as_ref().map(|r| r.hooks()).unwrap_or_default();
            let mut pick: Option<String> = None;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    egui::ScrollArea::vertical().id_salt("hooks").max_height(300.0).show(ui, |ui| {
                        for name in gitcraft_engine::git::tools::HOOK_NAMES {
                            let h = hooks.iter().find(|h| h.name == *name);
                            ui.horizontal(|ui| {
                                let mut on = h.is_some_and(|h| h.enabled);
                                if ui.add_enabled(h.is_some(), egui::Checkbox::without_text(&mut on)).on_hover_text("On / off").changed() {
                                    let _ = app.run("hooks.enable", json!({"name": name, "value": on}));
                                }
                                let label = if h.is_some_and(|h| h.sample) { RichText::new(*name).color(t.dim) } else { RichText::new(*name) };
                                if ui.selectable_label(selected == name, label).clicked() {
                                    pick = Some(name.to_string());
                                }
                            });
                        }
                    });
                });
                ui.vertical(|ui| {
                    if selected.is_empty() {
                        ui.label(RichText::new("Pick a hook to read or write it.").weak());
                    } else {
                        ui.add(egui::TextEdit::multiline(script).code_editor().desired_rows(16).desired_width(f32::INFINITY));
                        if ui.button(format!("Save {selected} (and turn it on)")).clicked() {
                            let _ = app.run("hooks.write", json!({"name": selected, "script": script}));
                        }
                    }
                });
            });
            if let Some(n) = pick {
                *script = app.session.repo.as_ref().and_then(|r| r.read_hook(&n).ok()).unwrap_or_else(|| "#!/bin/sh\n".into());
                *selected = n;
            }
            let close = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.button("Close").clicked()).inner;
            if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                return Close::Cancel;
            }
        }
        Dialog::AddWorktree { path, branch, new_branch } => {
            ui.heading("Add Worktree");
            ui.label(RichText::new("Another folder with its own checkout of this repository.").small().color(t.dim));
            ui.add_space(6.0);
            field(ui, "BRANCH", branch, "feature/next", first);
            ui.checkbox(new_branch, "Create the branch (from HEAD)");
            if path.trim().is_empty() && !branch.trim().is_empty() {
                let repo = app.session.repo.as_ref().map(|r| r.name()).unwrap_or_default();
                ui.label(RichText::new(format!("Folder: {repo}-{}", branch.trim().replace('/', "-"))).small().weak());
            }
            field(ui, "FOLDER (relative: beside the repository)", path, "", false);
            let folder = if path.trim().is_empty() {
                let repo = app.session.repo.as_ref().map(|r| r.name()).unwrap_or_default();
                format!("{repo}-{}", branch.trim().replace('/', "-"))
            } else {
                path.trim().to_string()
            };
            let (ok, cancel) = buttons(ui, "Add Worktree", !branch.trim().is_empty(), false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("worktree.add".into(), json!({"path": folder, "branch": branch.trim(), "newBranch": *new_branch}));
            }
        }
        Dialog::Identity { local_name, local_email, global_name, global_email } => {
            ui.heading("Author Identity");
            ui.label(RichText::new("The name and e-mail address your commits are made with.").small().color(t.dim));
            ui.add_space(6.0);
            ui.label(RichText::new("THIS REPOSITORY (overrides the one below; leave empty to use it)").small().strong());
            field(ui, "NAME", local_name, global_name, first);
            field(ui, "E-MAIL", local_email, global_email, false);
            ui.add_space(4.0);
            ui.label(RichText::new("EVERY REPOSITORY").small().strong());
            field(ui, "NAME", global_name, "Your Name", false);
            field(ui, "E-MAIL", global_email, "you@example.com", false);
            let valid = |e: &str| e.trim().is_empty() || (e.contains('@') && !e.contains(['<', '>']));
            let ok = valid(local_email) && valid(global_email);
            let (save, cancel) = buttons(ui, "Save", ok, false);
            if cancel {
                return Close::Cancel;
            }
            if save {
                let _ = app.run("config.identity", json!({"scope": "global", "name": global_name.trim(), "email": global_email.trim()}));
                return Close::Run("config.identity".into(), json!({"scope": "local", "name": local_name.trim(), "email": local_email.trim()}));
            }
        }
        Dialog::Signing { global, format, key, sign_commits, sign_tags } => {
            ui.heading("Commit Signing");
            ui.add_space(6.0);
            ui.label(RichText::new("KEY FORMAT").small().strong());
            ui.horizontal(|ui| {
                for (f, label) in [("openpgp", "GPG (OpenPGP)"), ("ssh", "SSH"), ("x509", "X.509")] {
                    ui.radio_value(format, f.to_string(), label);
                }
            });
            let hint = match format.as_str() {
                "ssh" => "~/.ssh/id_ed25519.pub, or the key itself",
                "x509" => "certificate id",
                _ => "key id (gpg --list-secret-keys --keyid-format long)",
            };
            field(ui, "SIGNING KEY", key, hint, first);
            ui.checkbox(sign_commits, "Sign every commit by default");
            ui.checkbox(sign_tags, "Sign every annotated tag by default");
            ui.checkbox(global, "For every repository (otherwise only this one)");
            let (save, cancel) = buttons(ui, "Save", true, false);
            if cancel {
                return Close::Cancel;
            }
            if save {
                return Close::Run(
                    "config.signing".into(),
                    json!({"scope": if *global { "global" } else { "local" }, "format": format, "key": key.trim(), "signCommits": *sign_commits, "signTags": *sign_tags}),
                );
            }
        }
        Dialog::SshKeys { keys, name, comment, passphrase } => {
            ui.heading("SSH Keys");
            ui.add_space(6.0);
            if keys.is_empty() {
                ui.label(RichText::new("No SSH keys yet.").weak());
            }
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                for (path, public) in keys.iter() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(path.as_str()).monospace().small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Copy Public Key").on_hover_text("Paste it into your hosting service's SSH keys").clicked() {
                                app.copy(public.clone());
                            }
                        });
                    });
                }
            });
            ui.separator();
            ui.label(RichText::new("NEW KEY (ed25519)").small().strong());
            field(ui, "FILE NAME IN ~/.ssh", name, "id_ed25519", false);
            field(ui, "COMMENT", comment, "you@example.com", false);
            ui.label(RichText::new("PASSPHRASE (optional)").small().strong());
            ui.add(egui::TextEdit::singleline(passphrase).password(true).desired_width(f32::INFINITY));
            let exists = keys.iter().any(|(p, _)| p.ends_with(&format!("/{}", name.trim())) || p.ends_with(&format!("\\{}", name.trim())));
            if exists {
                ui.label(RichText::new("A key with that name exists.").small().color(t.warning));
            }
            let (ok, cancel) = buttons(ui, "Generate Key", !name.trim().is_empty() && !exists, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                let r = app.run("ssh.generate", json!({"name": name.trim(), "comment": comment.trim(), "passphrase": passphrase}));
                passphrase.clear();
                if r.is_ok()
                    && let Some(Dialog::SshKeys { keys: k, .. }) = Dialog::named("ssh.keys", &app.session, &json!({}))
                {
                    *keys = k;
                }
            }
        }
        Dialog::CoAuthors { known, chosen, custom } => {
            ui.heading("Co-authors");
            ui.label(RichText::new("Credited with Co-authored-by lines in the next commit.").small().color(t.dim));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                let mut all: Vec<String> = chosen.clone();
                all.extend(known.iter().filter(|k| !chosen.contains(k)).cloned());
                for a in all {
                    let mut on = chosen.contains(&a);
                    if ui.checkbox(&mut on, a.as_str()).changed() {
                        if on {
                            chosen.push(a.clone());
                        } else {
                            chosen.retain(|x| *x != a);
                        }
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(custom).hint_text("Name <email>").desired_width(330.0));
                let ok = custom.contains('<') && custom.trim().ends_with('>');
                if ui.add_enabled(ok, egui::Button::new("Add")).clicked() {
                    chosen.push(custom.trim().to_string());
                    custom.clear();
                }
            });
            let (ok, cancel) = buttons(ui, "Done", true, false);
            if cancel {
                return Close::Cancel;
            }
            if ok {
                return Close::Run("commit.coAuthors".into(), json!({"authors": chosen}));
            }
        }
        Dialog::Shortcuts => {
            ui.heading("Keyboard Shortcuts");
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("shortcuts").striped(true).num_columns(2).show(ui, |ui| {
                    ui.label(RichText::new(util::shortcut_label("Mod+P")).monospace());
                    ui.label("Command palette");
                    ui.end_row();
                    for c in app.session.registry.all().iter().filter(|c| !c.shortcut.is_empty()) {
                        ui.label(RichText::new(util::shortcut_label(c.shortcut)).monospace());
                        ui.label(c.label);
                        ui.end_row();
                    }
                });
            });
            if buttons(ui, "Close", true, false) != (false, false) {
                return Close::Cancel;
            }
        }
        Dialog::Preferences => {
            ui.heading("Preferences");
            ui.add_space(6.0);
            ui.label(RichText::new("INTERFACE THEME").small().strong());
            ui.horizontal(|ui| {
                for a in Appearance::ALL {
                    ui.radio_value(&mut app.prefs.appearance, a, a.label());
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("ZOOM").small().strong());
            let mut z = app.prefs.zoom * 100.0;
            if ui
                .add(egui::Slider::new(&mut z, 50.0..=300.0).step_by(5.0).suffix("%"))
                .on_hover_text(util::shortcut_label("Mod+=") + " / " + &util::shortcut_label("Mod+-"))
                .changed()
            {
                app.prefs.zoom = z / 100.0;
            }
            ui.add_space(6.0);
            ui.label(RichText::new("DIFF CONTEXT LINES").small().strong());
            let mut c = app.prefs.context_lines;
            if ui.add(egui::Slider::new(&mut c, 0..=20)).changed() {
                app.prefs.context_lines = c;
                app.session.view.context = c;
            }
            ui.add_space(6.0);
            ui.label(RichText::new("COMMITS LOADED IN THE GRAPH").small().strong());
            let mut n = app.prefs.commit_limit;
            if ui.add(egui::Slider::new(&mut n, 100..=50_000).logarithmic(true)).changed() {
                app.prefs.commit_limit = n;
            }
            ui.checkbox(&mut app.prefs.reopen_last, "Reopen the last repository at start");
            ui.checkbox(&mut app.prefs.conflict_check, "Warn when merging the target or upstream branch would conflict")
                .on_hover_text("Merges in memory in the background after each change; nothing on disk is touched");
            if ui
                .checkbox(&mut app.prefs.gravatar, "Show author pictures from Gravatar")
                .on_hover_text("Sends a hash of each author's e-mail address to gravatar.com")
                .changed()
            {
                app.avatars.set_enabled(app.prefs.gravatar);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("TERMINAL").small().strong());
            ui.add(
                egui::TextEdit::singleline(&mut app.prefs.terminal)
                    .hint_text("System default (e.g. kitty --directory {dir})")
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text("The command that opens a terminal; {dir} is the repository's folder");
            ui.add_space(6.0);
            ui.label(RichText::new("EDITOR").small().strong());
            ui.add(egui::TextEdit::singleline(&mut app.prefs.editor).hint_text("System default (e.g. code -g {file})").desired_width(f32::INFINITY))
                .on_hover_text("The command that opens a file; {file} is its path");
            ui.add_space(6.0);
            let git = app.session.git.version().unwrap_or_else(|e| e.to_string());
            ui.label(RichText::new(git).small().weak());
            let (ok, cancel) = buttons(ui, "Done", true, false);
            if ok || cancel {
                if app.session.view.commit_limit != app.prefs.commit_limit {
                    app.session.view.commit_limit = app.prefs.commit_limit;
                    let _ = app.session.refresh();
                }
                return Close::Cancel;
            }
        }
    }
    Close::Keep
}

/// The interactive rebase list: newest at the top like the graph; each row's action, up / down,
/// and the new message for rewords.
fn rebase_body(ui: &mut egui::Ui, oid: &str, rows: &mut [RebaseRow], t: Tokens) -> Close {
    ui.set_width(560.0);
    ui.heading("Interactive Rebase");
    ui.label(RichText::new("Newest first. Squash and Fixup combine a commit with the one below it; Drop leaves it out.").small().color(t.dim));
    ui.add_space(6.0);
    let mut swap: Option<(usize, usize)> = None;
    let n = rows.len();
    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
        for i in (0..n).rev() {
            let Some(r) = rows.get_mut(i) else { continue };
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("rebase_action", i)).width(80.0).selected_text(r.action.label()).show_ui(ui, |ui| {
                    for a in TodoAction::ALL {
                        ui.selectable_value(&mut r.action, a, a.label());
                    }
                });
                let top = (i + 1 >= n).then_some("already the newest");
                if crate::icons::button(ui, crate::icons::Icon::ChevronUp, "Move up (newer)", top, 12.0).clicked() {
                    swap = Some((i, i + 1));
                }
                let bottom = (i == 0).then_some("already the oldest");
                if crate::icons::button(ui, crate::icons::Icon::ChevronDown, "Move down (older)", bottom, 12.0).clicked() {
                    swap = Some((i, i.saturating_sub(1)));
                }
                ui.label(RichText::new(r.oid.get(..7).unwrap_or(&r.oid)).monospace().color(t.dim));
                let text = RichText::new(&r.summary);
                ui.label(if r.action == TodoAction::Drop { text.strikethrough().color(t.dim) } else { text });
            });
            if r.action == TodoAction::Reword {
                ui.add(egui::TextEdit::multiline(&mut r.message).desired_rows(2).desired_width(f32::INFINITY));
            }
        }
    });
    if let Some((a, b)) = swap {
        rows.swap(a, b);
    }
    let first_kept = rows.iter().find(|r| r.action != TodoAction::Drop).map(|r| r.action);
    let valid = !matches!(first_kept, Some(TodoAction::Squash | TodoAction::Fixup))
        && rows.iter().all(|r| r.action != TodoAction::Reword || !r.message.trim().is_empty());
    if !valid {
        ui.label(RichText::new("The oldest commit kept can't be squashed, and rewords need a message.").small().color(t.warning));
    }
    let (ok, cancel) = buttons(ui, "Start Rebase", valid, false);
    if cancel {
        return Close::Cancel;
    }
    if ok {
        let items: Vec<Value> = rows
            .iter()
            .map(|r| {
                let message = (r.action == TodoAction::Reword).then(|| r.message.clone());
                json!({"action": r.action.code(), "oid": r.oid, "message": message})
            })
            .collect();
        return Close::Run("rebase.interactive".into(), json!({"oid": oid, "items": items}));
    }
    Close::Keep
}
