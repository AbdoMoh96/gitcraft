use std::collections::HashSet;

use gitcraft_git::testing::{TempDir, sample, write};
use serde_json::{Value, json};

use crate::{Selected, Session};

fn session(tag: &str) -> (TempDir, Session) {
    let (dir, repo) = sample(tag).unwrap();
    let mut s = Session::new(repo.git().clone());
    s.set_repo(repo).unwrap();
    (dir, s)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.run(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

#[test]
fn registry_ids_are_unique_and_described() {
    let reg = crate::cmd::registry();
    let mut seen = HashSet::new();
    for c in reg.all() {
        assert!(seen.insert(c.id), "duplicate id {}", c.id);
        assert!(!c.label.is_empty() && !c.location.is_empty(), "{}", c.id);
        assert!(c.id.contains('.'), "{}", c.id);
    }
    assert!(reg.by_shortcut("Mod+Enter").is_some_and(|c| c.id == "commit.create"));
}

#[test]
fn opening_loads_everything() {
    let (_d, s) = session("eng-open");
    assert_eq!(s.data.status.branch.as_deref(), Some("main"));
    assert_eq!(s.data.commits.len(), 3);
    assert_eq!(s.data.local_branches().count(), 2);
    assert_eq!(s.data.tags().count(), 1);
    assert_eq!(s.data.graph.rows.len(), 3);
    assert!(s.data.graph.width >= 1);
    // A clean repository opens on HEAD; HEAD carries the main label.
    let head = s.data.status.head.clone().unwrap();
    assert_eq!(s.sel.row, Selected::Commit(head.clone()));
    let labels = &s.data.labels[&head];
    assert!(labels.iter().any(|&i| s.data.refs[i].name == "main"));
    assert_eq!(s.inspect()["branches"], json!(["feature", "main"]));
}

#[test]
fn stage_commit_undo_redo() {
    let (_d, mut s) = session("eng-commit");
    let repo = s.repo().unwrap().clone();
    write(&repo, "README.md", "# Changed\n").unwrap();
    write(&repo, "notes.txt", "new\n").unwrap();
    run(&mut s, "repo.refresh", json!({}));
    assert_eq!(s.data.status.unstaged().count(), 2);
    assert!(s.run("commit.create", &json!({"summary": "x"})).is_err(), "nothing staged");
    run(&mut s, "stage.file", json!({"path": "README.md"}));
    assert_eq!(s.data.status.staged().count(), 1);
    run(&mut s, "commit.message", json!({"summary": "Update readme", "description": "Longer text"}));
    let before = s.data.status.head.clone();
    let r = run(&mut s, "commit.create", json!({}));
    let oid = r["oid"].as_str().unwrap().to_string();
    assert_eq!(s.data.status.head.as_deref(), Some(oid.as_str()));
    assert_eq!(s.data.commit(&oid).unwrap().summary, "Update readme");
    assert_eq!(s.data.commit(&oid).unwrap().body, "Longer text");
    assert!(s.draft.summary.is_empty());
    assert_eq!(s.sel.row, Selected::Commit(oid.clone()));
    // Undo puts the commit's changes back in the index.
    assert_eq!(s.can_undo().as_deref(), Some("Commit"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.head, before);
    assert!(s.data.status.file("README.md").unwrap().is_staged());
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.data.status.head.as_deref(), Some(oid.as_str()));
    // Stage all and commit.
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "Notes"}));
    assert!(s.data.status.is_clean());
    // Amend prefills the message.
    run(&mut s, "commit.amend", json!({"value": true}));
    assert_eq!(s.draft.summary, "Notes");
    let r = run(&mut s, "commit.create", json!({"summary": "Notes, amended"}));
    assert_eq!(s.data.commit(r["oid"].as_str().unwrap()).unwrap().summary, "Notes, amended");
    assert_eq!(s.data.commits.len(), 5);
    // Edit an older commit's message, then undo and redo it.
    let head = s.data.status.head.clone();
    let older = s.data.commits[2].oid.clone();
    let r = run(&mut s, "commit.reword", json!({"oid": older, "summary": "Reworded", "description": "Body"}));
    let new = r["oid"].as_str().unwrap().to_string();
    assert_eq!(s.data.commits[2].oid, new);
    assert_eq!(s.data.commits[2].message(), "Reworded\n\nBody");
    assert_eq!(s.sel.row, Selected::Commit(new));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.head, head);
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.data.commits[2].summary, "Reworded");
    assert!(s.run("commit.reword", &json!({"oid": older, "summary": " "})).is_err());
}

#[test]
fn branches_checkout_merge_and_undo() {
    let (_d, mut s) = session("eng-branch");
    run(&mut s, "branch.create", json!({"name": "topic"}));
    assert_eq!(s.data.status.branch.as_deref(), Some("topic"));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.branch.as_deref(), Some("main"));
    assert!(!s.data.local_branches().any(|r| r.name == "topic"));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.data.status.branch.as_deref(), Some("topic"));
    run(&mut s, "branch.checkout", json!({"name": "main"}));
    run(&mut s, "branch.rename", json!({"from": "topic", "to": "topic2"}));
    run(&mut s, "branch.delete", json!({"name": "topic2"}));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.data.local_branches().any(|r| r.name == "topic2"));
    let r = run(&mut s, "merge.into", json!({"ref": "feature"}));
    assert_eq!(r["conflicts"], json!(false));
    assert!(s.repo().unwrap().root().join("src/lib.rs").exists());
    // Reset back and undo the reset.
    let head = s.data.status.head.clone().unwrap();
    run(&mut s, "reset.to", json!({"oid": "HEAD~1", "mode": "hard"}));
    assert_ne!(s.data.status.head.as_deref(), Some(head.as_str()));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.head.as_deref(), Some(head.as_str()));
    // Detached checkout of the selected commit.
    run(&mut s, "graph.select", json!({"oid": "v0.1.0"}));
    run(&mut s, "commit.checkout", json!({}));
    assert_eq!(s.data.status.branch, None);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.branch.as_deref(), Some("main"));
}

#[test]
fn tags_stash_and_navigation() {
    let (_d, mut s) = session("eng-tags");
    run(&mut s, "tag.create", json!({"name": "v1", "message": "One"}));
    assert!(s.data.tags().any(|t| t.name == "v1"));
    run(&mut s, "tag.delete", json!({"name": "v1"}));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.data.tags().any(|t| t.name == "v1"));
    let repo = s.repo().unwrap().clone();
    write(&repo, "README.md", "stash me\n").unwrap();
    run(&mut s, "repo.refresh", json!({}));
    run(&mut s, "stash.save", json!({"message": "wip"}));
    assert!(s.data.status.is_clean());
    assert_eq!(s.data.stashes.len(), 1);
    run(&mut s, "stash.pop", json!({}));
    assert!(s.data.stashes.is_empty());
    assert!(!s.data.status.is_clean());
    // Up and down move through WIP and the commits.
    run(&mut s, "graph.select", json!({"oid": "wip"}));
    run(&mut s, "graph.selectNext", json!({}));
    assert_eq!(s.sel.row, Selected::Commit(s.data.commits[0].oid.clone()), "the row under WIP");
    run(&mut s, "graph.selectPrevious", json!({}));
    assert_eq!(s.sel.row, Selected::Wip);
    run(&mut s, "graph.selectPrevious", json!({}));
    assert_eq!(s.sel.row, Selected::Wip);
    let r = run(&mut s, "view.search", json!({"text": "greet"}));
    assert_eq!(r["matches"], json!(1));
}

#[test]
fn diff_view_and_partial_staging() {
    let (_d, mut s) = session("eng-diff");
    let repo = s.repo().unwrap().clone();
    let body: String = (1..=30).map(|i| format!("row {i}\n")).collect();
    write(&repo, "big.txt", &body).unwrap();
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "big"}));
    write(&repo, "big.txt", &body.replace("row 2\n", "row two\n").replace("row 28\n", "row 28!\n")).unwrap();
    run(&mut s, "repo.refresh", json!({}));
    run(&mut s, "file.select", json!({"path": "big.txt", "area": "unstaged"}));
    let d = s.selected_diff().unwrap().unwrap();
    assert_eq!(d.hunks.len(), 2);
    run(&mut s, "stage.hunk", json!({"path": "big.txt", "hunk": 0}));
    let staged = s.diff_for(&crate::FileSel { path: "big.txt".into(), area: crate::Area::Staged }, 3).unwrap();
    assert_eq!(staged.hunks.len(), 1);
    // The file is in both lists now; the diff view follows the unstaged half.
    assert_eq!(s.selected_diff().unwrap().unwrap().hunks.len(), 1);
    run(&mut s, "unstage.hunk", json!({"path": "big.txt", "hunk": 0}));
    assert_eq!(s.data.status.staged().count(), 0);
    run(&mut s, "discard.hunk", json!({"path": "big.txt", "hunk": 1}));
    let left = s.diff_for(&crate::FileSel { path: "big.txt".into(), area: crate::Area::Unstaged }, 3).unwrap();
    assert_eq!(left.hunks.len(), 1);
    // Inline mode loads the whole file as one hunk.
    run(&mut s, "view.diffMode", json!({"value": "inline"}));
    let d = s.selected_diff().unwrap().unwrap();
    assert_eq!(d.hunks.len(), 1);
    assert!(d.hunks[0].lines.len() >= 30);
    // A commit's file.
    let head = s.data.status.head.clone().unwrap();
    let r = run(&mut s, "commit.show", json!({"oid": head}));
    assert_eq!(r["files"][0]["path"], json!("big.txt"));
    let r = run(&mut s, "diff.file", json!({"path": "big.txt", "oid": head}));
    assert_eq!(r["hunks"][0]["newLen"], json!(30));
    let r = run(&mut s, "file.blame", json!({"path": "src/main.rs"}));
    assert_eq!(r["lines"].as_array().unwrap().len(), 3);
    run(&mut s, "view.history", json!({"path": "src/main.rs"}));
    assert_eq!(s.data.commits.len(), 2);
    run(&mut s, "view.historyClear", json!({}));
    assert_eq!(s.data.commits.len(), 4);
}

#[test]
fn conflicts_resolve_and_continue() {
    let (_d, mut s) = session("eng-conflict");
    let repo = s.repo().unwrap().clone();
    run(&mut s, "branch.checkout", json!({"name": "feature"}));
    write(&repo, "README.md", "feature side\n").unwrap();
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "feature readme"}));
    run(&mut s, "branch.checkout", json!({"name": "main"}));
    write(&repo, "README.md", "main side\n").unwrap();
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "main readme"}));
    let r = run(&mut s, "merge.into", json!({"ref": "feature"}));
    assert_eq!(r["conflicts"], json!(true));
    assert_eq!(s.inspect()["conflicts"], json!(["README.md"]));
    let v = run(&mut s, "conflict.versions", json!({"path": "README.md"}));
    assert_eq!(v["ours"], json!("main side\n"));
    assert_eq!(v["theirs"], json!("feature side\n"));
    assert!(s.run("conflict.versions", &json!({"path": "../x"})).is_err());
    run(&mut s, "conflict.takeTheirs", json!({"path": "README.md"}));
    // The commit panel finishes the merge with git's prepared message.
    let r = run(&mut s, "commit.create", json!({}));
    let merge = s.data.commit(r["oid"].as_str().unwrap()).unwrap();
    assert!(merge.is_merge());
    assert!(merge.summary.starts_with("Merge branch 'feature'"));
}

#[test]
fn reloads_only_on_outside_changes() {
    let (_d, mut s) = session("eng-changed");
    assert!(!s.refresh_if_changed().unwrap());
    let g = s.generation();
    write(s.repo().unwrap(), "README.md", "outside edit\n").unwrap();
    assert!(s.refresh_if_changed().unwrap());
    assert!(s.generation() > g);
    assert!(!s.refresh_if_changed().unwrap());
}

#[test]
fn background_sessions_never_reload() {
    let (_d, s) = session("eng-detached");
    let mut worker = Session::detached(s.repo().unwrap().clone());
    assert!(worker.data.commits.is_empty());
    let r = worker.run("graph.log", &json!({"limit": 2})).unwrap();
    assert_eq!(r["commits"].as_array().unwrap().len(), 2);
    worker.run("tag.create", &json!({"name": "from-worker", "oid": "HEAD"})).unwrap();
    assert!(worker.data.refs.is_empty());
}

/// Every command, with junk parameters, in a real repository: errors are fine, panics are not.
#[test]
fn hostile_params_never_panic() {
    let junk = [
        Value::Null,
        json!({}),
        json!([1, 2, 3]),
        json!("string"),
        json!({"path": 5, "name": [], "oid": {}, "ref": null}),
        json!({"name": "--force", "ref": "--upload-pack=x", "oid": "-1", "remote": "-x", "url": "--config=x"}),
        json!({"path": "../../../../etc/passwd", "paths": []}),
        json!({"paths": ["a", 1, null]}),
        json!({"path": "README.md", "hunk": 999_999, "lines": [0, 1, 999_999], "context": 1e300}),
        json!({"path": "README.md", "hunk": -1, "lines": "x", "index": 1e20, "count": -5}),
        json!({"value": "nonsense", "mode": "sideways", "text": "x".repeat(100_000)}),
        json!({"name": "a\nb", "summary": "\0", "message": "\u{202e}", "oid": "zzzzzzz"}),
        json!({"name": "", "to": "", "from": "", "upstream": ""}),
    ];
    // These would touch folders outside the test repository.
    let skip = ["repo.open", "repo.init", "repo.clone"];
    let (_d, mut s) = session("eng-hostile");
    let repo = s.repo().unwrap().clone();
    let ids: Vec<&'static str> = s.registry.all().iter().map(|c| c.id).collect();
    for id in ids {
        if skip.contains(&id) {
            continue;
        }
        for p in &junk {
            if s.repo.is_none() {
                s.set_repo(repo.clone()).unwrap();
            }
            write(&repo, "README.md", "dirty\n").unwrap();
            let _ = s.refresh();
            if let Err(e) = s.run(id, p) {
                assert!(!e.to_string().contains("internal error"), "{id} {p}: {e}");
            }
        }
    }
    // The safe junk for the commands that create or open folders.
    for id in skip {
        for p in [json!({}), json!({"path": 5}), json!({"url": "--upload-pack=touch x", "path": ""})] {
            if let Err(e) = s.run(id, &p) {
                assert!(!e.to_string().contains("internal error"), "{id} {p}: {e}");
            }
        }
    }
}
