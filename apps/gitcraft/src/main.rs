//! GitCraft desktop app.
//!
//! Usage: `gitcraft [--control <port>] [--sample] [folder]`; `--help` prints `USAGE`.
//!
//! `--control <port>` (or `GITCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control server:
//! `{"id":1,"key":"…","method":"engine.execute","params":{"command":"stage.all"}}`. Every request
//! needs the key the app writes to `<settings>/control-key.local-<port>` (mode 0600, removed on
//! exit). See `docs/control-protocol.md`.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod control_server;
mod logging;

use std::path::{Path, PathBuf};

use gitcraft_engine::Session;
use gitcraft_ui_egui::{GitApp, Prefs, Services};

const USAGE: &str = "\
gitcraft — a fast, open-source Git client

USAGE:
  gitcraft [folder]              open the repository containing folder (default: the last one)
  gitcraft --sample              open a throwaway sample repository (removed on exit)
  gitcraft --control <port>      accept automation on 127.0.0.1:<port> (see docs/control-protocol.md)
  gitcraft --help | --version

ENVIRONMENT:
  GITCRAFT_GIT            the git program (default: git on PATH)
  GITCRAFT_PROFILE_DIR    settings folder (default: the platform's config folder)
  GITCRAFT_CONTROL_PORT   same as --control
  GITCRAFT_LOG            log level: error, warn, info, debug
";

struct App {
    app: GitApp,
    /// Removed on exit.
    key_file: Option<control_server::KeyFile>,
    /// `--sample`: the throwaway repository, removed on exit.
    _sample: Option<gitcraft_git::testing::TempDir>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.logic(ctx);
        if self.app.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }
    fn on_exit(&mut self) {
        save_prefs(&self.app.prefs);
        if let Some(k) = &self.key_file {
            k.remove();
        }
    }
}

fn prefs_path() -> Option<PathBuf> {
    gitcraft_control_key::settings_dir().map(|d| d.join("ui.json"))
}

fn load_prefs() -> Prefs {
    prefs_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| Prefs::from_json(&b)).unwrap_or_default()
}

fn save_prefs(p: &Prefs) {
    let Some(path) = prefs_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(p) {
        Ok(bytes) => {
            // Write then rename: a crash mid-write never leaves half a file.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => log::warn!("preferences not saved: {e}"),
    }
}

/// The window icon: the GitCraft cat (`assets/gitcraft.png`), embedded at build time.
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/gitcraft.png")).unwrap_or_default()
}

fn main() -> std::process::ExitCode {
    logging::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("gitcraft {}", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    let port: Option<u16> = args
        .iter()
        .position(|a| a == "--control")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| std::env::var("GITCRAFT_CONTROL_PORT").ok())
        .and_then(|p| p.trim().parse().ok());
    let sample = args.iter().any(|a| a == "--sample");
    let folder = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || args.get(i - 1).is_none_or(|p| p != "--control")))
        .map(|(_, a)| PathBuf::from(a));

    let mut prefs = load_prefs();
    let mut keep_sample = None;
    let mut restore_tabs = false;
    let mut session = Session::default();
    if sample {
        match gitcraft_git::testing::sample("app") {
            Ok((dir, repo)) => {
                session = Session::new(repo.git().clone());
                if let Err(e) = session.set_repo(repo) {
                    log::error!("sample: {e}");
                }
                keep_sample = Some(dir);
            }
            Err(e) => log::error!("sample: {e}"),
        }
    } else if let Some(f) = &folder {
        if let Err(e) = session.open(f) {
            eprintln!("gitcraft: {e}");
        }
    } else if prefs.reopen_last && !prefs.tabs.is_empty() {
        restore_tabs = true;
    } else if prefs.reopen_last
        && let Some(last) = prefs.recent.first().cloned()
        && Path::new(&last).is_dir()
        && session.open(Path::new(&last)).is_err()
    {
        prefs.recent.retain(|r| *r != last);
    }
    if let Some(root) = session.repo.as_ref().map(|r| r.root().to_path_buf())
        && !sample
    {
        prefs.add_recent(&root);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GitCraft")
            .with_app_id("gitcraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 520.0])
            .with_drag_and_drop(true)
            .with_icon(app_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "GitCraft",
        options,
        Box::new(move |cc| {
            gitcraft_ui_egui::theme::style(&cc.egui_ctx);
            let services = Services {
                pick_folder: Some(Box::new(|title: &str| rfd::FileDialog::new().set_title(title).pick_folder())),
                avatar_cache: gitcraft_control_key::settings_dir().map(|d| d.join("avatars")),
            };
            let mut app = GitApp::new(session, prefs, services);
            // The last run's tabs (the first one is open already).
            if restore_tabs {
                let (tabs, active) = (app.prefs.tabs.clone(), app.prefs.active_tab);
                app.restore_tabs(&tabs, active);
            }
            let mut key_file = None;
            if let Some(port) = port {
                let settings = gitcraft_control_key::settings_dir();
                if let Some((rx, kf)) = control_server::start(port, cc.egui_ctx.clone(), settings.as_deref()) {
                    app = app.with_control(rx);
                    key_file = kf;
                }
            }
            Ok(Box::new(App { app, key_file, _sample: keep_sample }))
        }),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gitcraft: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
