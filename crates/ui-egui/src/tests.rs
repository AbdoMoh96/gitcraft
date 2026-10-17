use gitcraft_engine::Session;
use gitcraft_git::testing::{TempDir, sample, write};
use serde_json::json;

use crate::{GitApp, Prefs, Services};

fn app(tag: &str) -> (TempDir, GitApp) {
    let (d, repo) = sample(tag).unwrap();
    let mut s = Session::new(repo.git().clone());
    s.set_repo(repo).unwrap();
    (d, GitApp::new(s, Prefs::default(), Services::default()))
}

#[test]
fn every_catalog_ui_command_is_handled() {
    let (_d, mut a) = app("ui-cmds");
    for id in gitcraft_engine::catalog::UI_COMMANDS {
        let r = a.ui_command(id, &json!({}));
        if let Err(e) = &r {
            assert!(!e.starts_with("unknown command"), "{id} is not handled: {e}");
        }
    }
}

#[test]
fn prefs_round_trip_and_recent() {
    let mut p = Prefs::default();
    for i in 0..20 {
        p.add_recent(std::path::Path::new(&format!("/r/{i}")));
    }
    p.add_recent(std::path::Path::new("/r/5"));
    assert_eq!(p.recent.len(), crate::MAX_RECENT);
    assert_eq!(p.recent[0], "/r/5");
    let back = Prefs::from_json(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(back, p);
    // Unknown and missing fields are fine.
    assert_eq!(Prefs::from_json(br#"{"future": 1}"#).unwrap(), Prefs::default());
}

#[test]
fn slow_commands_run_in_the_background() {
    let (d, mut a) = app("ui-jobs");
    // A clone of the sample as an upstream for the app's repository.
    let up = a.session.repo().unwrap().clone();
    let clone = up.git().clone_repo(&up.root().to_string_lossy(), &d.path().join("clone")).unwrap();
    a.open_repo(clone.root()).unwrap();
    write(&up, "NEW.md", "x\n").unwrap();
    gitcraft_git::testing::commit_all(&up, "Upstream").unwrap();
    let r = a.run("remote.fetch", json!({})).unwrap();
    assert_eq!(r["started"], json!("remote.fetch"));
    assert_eq!(a.jobs.len(), 1);
    let ctx = egui::Context::default();
    for _ in 0..500 {
        a.logic(&ctx);
        if a.jobs.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(a.jobs.is_empty());
    assert_eq!(a.session.data.status.behind, 1);
    assert!(a.session.activity.iter().any(|x| x.command == "remote.fetch" && x.ok));
}

/// Lays out the whole window headlessly (no GPU) in each state: no repository, the graph, a diff,
/// a commit's details, every dialog and the palette.
#[test]
fn every_view_lays_out() {
    let (_d, mut a) = app("ui-layout");
    write(a.session.repo().unwrap(), "README.md", "changed\nlines\n").unwrap();
    let _ = a.session.refresh();
    let ctx = egui::Context::default();
    crate::theme::style(&ctx);
    let frame = |a: &mut GitApp| {
        let input =
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            a.logic(ui.ctx());
            a.ui(ui);
        });
        // No renderer here: drop the font atlas uploads on purpose.
        out.textures_delta.clear();
    };
    frame(&mut a);
    a.run("graph.select", json!({"oid": "wip"})).unwrap();
    frame(&mut a);
    a.run("file.select", json!({"path": "README.md", "area": "unstaged"})).unwrap();
    for mode in ["hunk", "inline", "split"] {
        a.run("view.diffMode", json!({"value": mode})).unwrap();
        frame(&mut a);
    }
    a.run("view.blame", json!({"value": true})).unwrap();
    frame(&mut a);
    a.run("view.blame", json!({"value": false})).unwrap();
    a.run("file.close", json!({})).unwrap();
    a.run("graph.selectHead", json!({})).unwrap();
    frame(&mut a);
    for name in [
        "repo.open",
        "repo.clone",
        "repo.init",
        "branch.create",
        "branch.rename",
        "tag.create",
        "remote.add",
        "stash.save",
        "shortcuts",
        "preferences",
    ] {
        a.ui_command("ui.dialog", &json!({"name": name})).unwrap();
        frame(&mut a);
        assert!(a.dialog.is_some(), "{name}");
        a.dialog = None;
    }
    a.ui_command("ui.commandPalette", &json!({})).unwrap();
    frame(&mut a);
    assert!(a.palette.is_some());
    a.palette = None;
    a.run("repo.close", json!({})).unwrap();
    frame(&mut a);
}

/// Dragging a branch label onto another branch's row opens the drop menu.
#[test]
fn drag_a_branch_onto_another() {
    let (_d, mut a) = app("ui-drag");
    let ctx = egui::Context::default();
    crate::theme::style(&ctx);
    let frame = |a: &mut GitApp, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            a.logic(ui.ctx());
            a.ui(ui);
        });
        out.textures_delta.clear();
    };
    frame(&mut a, vec![]);
    frame(&mut a, vec![]);
    let pill = |name: &str| {
        let i = a.session.data.refs.iter().position(|r| r.name == name && r.kind == gitcraft_git::RefKind::Local).unwrap();
        ctx.read_response(egui::Id::new(("pill", name.to_string(), i))).unwrap().rect.center()
    };
    let (from, to) = (pill("feature"), pill("main"));
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    frame(&mut a, vec![egui::Event::PointerMoved(from), button(from, true)]);
    frame(&mut a, vec![egui::Event::PointerMoved(from + egui::vec2(0.0, -12.0))]);
    frame(&mut a, vec![egui::Event::PointerMoved(to)]);
    assert!(egui::DragAndDrop::has_any_payload(&ctx));
    frame(&mut a, vec![button(to, false)]);
    let m = a.drop_menu.clone().expect("the drop menu is open");
    assert_eq!((m.from.as_str(), m.onto.as_str()), ("feature", "main"));
    assert!(m.options.iter().any(|o| o.1 == "merge"));
    frame(&mut a, vec![egui::Event::PointerMoved(to + egui::vec2(0.0, 40.0))]);
    assert!(a.drop_menu.is_some());
    a.run("branch.dropOnto", json!({"from": m.from, "onto": m.onto, "action": "merge"})).unwrap();
    assert!(a.session.repo().unwrap().is_ancestor("feature", "main"));
}

/// Hovering a pill that stands for several branches lists them all; a click checks one out.
#[test]
fn hover_lists_every_branch_on_a_commit() {
    let (_d, mut a) = app("ui-labels");
    a.run("branch.create", json!({"name": "topic", "checkout": true})).unwrap();
    let ctx = egui::Context::default();
    crate::theme::style(&ctx);
    let frame = |a: &mut GitApp, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            a.logic(ui.ctx());
            a.ui(ui);
        });
        out.textures_delta.clear();
    };
    frame(&mut a, vec![]);
    frame(&mut a, vec![]);
    let i = a.session.data.refs.iter().position(|r| r.name == "topic" && r.kind == gitcraft_git::RefKind::Local).unwrap();
    let pill = ctx.read_response(egui::Id::new(("pill", "topic".to_string(), i))).unwrap().rect;
    frame(&mut a, vec![egui::Event::PointerMoved(pill.center())]);
    frame(&mut a, vec![egui::Event::PointerMoved(pill.center())]);
    assert!(a.label_popup.is_some(), "hovering opens the list");
    let list = ctx.memory(|m| m.area_rect(egui::Id::new("label_popup"))).unwrap();
    // Down through the gap into the list: it stays open.
    frame(&mut a, vec![egui::Event::PointerMoved(egui::pos2(pill.center().x, list.top() + 2.0))]);
    assert!(a.label_popup.is_some());
    // Rows: topic (checked out, first), main, then the tag v0.1.0: click main.
    let main = egui::pos2(list.left() + 60.0, list.bottom() - 18.0 - 24.0);
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    frame(&mut a, vec![egui::Event::PointerMoved(main)]);
    frame(&mut a, vec![button(main, true)]);
    frame(&mut a, vec![button(main, false)]);
    let notes: Vec<_> = a.toasts.notes.iter().map(|n| n.text.clone()).collect();
    assert_eq!(a.session.data.status.branch.as_deref(), Some("main"), "{notes:?} {list:?} {pill:?}");
    assert!(a.label_popup.is_none());
    assert!(a.toasts.notes.iter().any(|n| n.text.contains("main")));
    // Moving away closes it.
    frame(&mut a, vec![egui::Event::PointerMoved(pill.center())]);
    frame(&mut a, vec![egui::Event::PointerMoved(pill.center())]);
    assert!(a.label_popup.is_some());
    frame(&mut a, vec![egui::Event::PointerMoved(egui::pos2(1000.0, 800.0))]);
    assert!(a.label_popup.is_none());
}
