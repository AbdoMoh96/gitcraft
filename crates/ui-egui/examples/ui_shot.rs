//! Headless window screenshots: no window, no focus stealing, works on a locked screen.
//!
//! `cargo run -p gitcraft-ui-egui --example ui_shot -- out.png [--size 1440x900] [--dark] [--cmd 'id={json}' …]`
//!
//! Lays out GitCraft over a sample repository (with a feature branch, a merge, a tag, a stash and
//! work in progress), runs the commands in order (`ui.*` ones too, e.g.
//! `--cmd 'ui.dialog={"name":"branch.create"}'`), renders with wgpu and writes the PNG.

use gitcraft_engine::Session;
use gitcraft_git::testing::{commit_all, sample, write};
use gitcraft_ui_egui::{Appearance, GitApp, Prefs, Services};
use serde_json::{Value, json};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().cloned().ok_or("usage: ui_shot out.png [--size WxH] [--dark] [--cmd 'id={json}' …]")?;
    let (w, h) = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
        .unwrap_or((1440.0, 900.0));
    let dark = args.iter().any(|a| a == "--dark");
    let cmds: Vec<(String, Value)> = args
        .windows(2)
        .filter(|w| w[0] == "--cmd")
        .map(|w| match w[1].split_once('=') {
            Some((id, p)) => (id.to_string(), serde_json::from_str(p).unwrap_or(json!({}))),
            None => (w[1].clone(), json!({})),
        })
        .collect();

    let (_dir, repo) = sample("shot").map_err(|e| e.to_string())?;
    // A little more history: a merged branch, a second feature, a stash, and work in progress.
    let step = |r: Result<String, gitcraft_git::GitError>| r.map(|_| ()).map_err(|e| e.to_string());
    repo.merge("feature", false, true).map_err(|e| e.to_string())?;
    repo.create_branch("docs/guide", None, true).map_err(|e| e.to_string())?;
    write(&repo, "docs/guide.md", "# Guide\n\nHow to use the sample.\n").map_err(|e| e.to_string())?;
    step(commit_all(&repo, "Write the user guide"))?;
    write(&repo, "docs/guide.md", "# Guide\n\nHow to use the sample.\n\n## Install\n\nRun the installer.\n").map_err(|e| e.to_string())?;
    step(commit_all(&repo, "Explain installation"))?;
    repo.checkout_branch("main").map_err(|e| e.to_string())?;
    repo.create_branch("fix/typo", None, true).map_err(|e| e.to_string())?;
    write(&repo, "README.md", "# Sample\n\nA repository for GitCraft, the Git client.\n").map_err(|e| e.to_string())?;
    step(commit_all(&repo, "Fix a typo in the readme"))?;
    repo.checkout_branch("main").map_err(|e| e.to_string())?;
    write(&repo, "src/main.rs", "fn main() {\n    let who = \"world\";\n    println!(\"hello, {who}\");\n}\n").map_err(|e| e.to_string())?;
    step(commit_all(&repo, "Name who we greet"))?;
    write(&repo, "notes.txt", "stashed idea\n").map_err(|e| e.to_string())?;
    repo.stash_save(Some("an idea for later"), true).map_err(|e| e.to_string())?;
    write(&repo, "src/main.rs", "fn main() {\n    let who = \"GitCraft\";\n    println!(\"hello, {who}!\");\n}\n").map_err(|e| e.to_string())?;
    write(&repo, "CHANGELOG.md", "# Changelog\n\n- First release\n").map_err(|e| e.to_string())?;
    repo.stage(&["CHANGELOG.md".into()]).map_err(|e| e.to_string())?;

    let mut session = Session::new(repo.git().clone());
    session.set_repo(repo).map_err(|e| e.to_string())?;
    let prefs = Prefs { appearance: if dark { Appearance::Dark } else { Appearance::Light }, ..Prefs::default() };
    let mut app = GitApp::new(session, prefs, Services::default());
    app.session.draft.summary = "Greet GitCraft by name".into();
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(w, h)).wgpu().build_ui_state(
        |ui, app: &mut GitApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    harness.ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    gitcraft_ui_egui::theme::style(&harness.ctx);
    harness.run_steps(3);
    for (id, p) in cmds {
        let r = harness.state_mut().run(&id, p);
        if let Err(e) = r {
            eprintln!("{id}: {e}");
        }
        harness.run_steps(3);
    }
    let img = harness.render()?;
    img.save(&out).map_err(|e| e.to_string())?;
    eprintln!("wrote {out}");
    Ok(())
}
