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
    let older = s.repo().unwrap().resolve("HEAD~1").unwrap();
    let r = run(&mut s, "commit.reword", json!({"oid": older, "summary": "Reworded", "description": "Body"}));
    let new = r["oid"].as_str().unwrap().to_string();
    assert_eq!(s.data.commit(&new).unwrap().message(), "Reworded\n\nBody");
    assert!(s.data.commit(&older).is_none());
    assert_eq!(s.sel.row, Selected::Commit(new.clone()));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.head, head);
    run(&mut s, "edit.redo", json!({}));
    assert!(s.data.commit(&new).is_some());
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
    // Predicted against a target before merging.
    let p = run(&mut s, "conflict.predict", json!({"target": "feature"}));
    if p["supported"] == json!(true) {
        assert_eq!(p["conflicts"], json!(true));
        assert_eq!(p["checks"][0]["files"], json!(["README.md"]));
    }
    assert!(s.run("conflict.predict", &json!({"target": "-x"})).is_err());
    // The default target is main: nothing to check on main itself.
    assert_eq!(run(&mut s, "conflict.predict", json!({}))["checks"], json!([]));
    let r = run(&mut s, "merge.into", json!({"ref": "feature"}));
    assert_eq!(r["conflicts"], json!(true));
    assert_eq!(s.inspect()["conflicts"], json!(["README.md"]));
    let v = run(&mut s, "conflict.versions", json!({"path": "README.md"}));
    assert_eq!(v["ours"], json!("main side\n"));
    assert_eq!(v["theirs"], json!("feature side\n"));
    assert!(s.run("conflict.versions", &json!({"path": "../x"})).is_err());
    assert_eq!(v["mergeable"], json!(true));
    assert!(v["chunks"].as_array().unwrap().iter().any(|c| c["kind"] == "conflict" && c["theirs"] == json!(["feature side\n"])));
    // The merge tool's output: only for files in conflict.
    assert!(s.run("conflict.edit", &json!({"path": "src/main.rs", "content": "x"})).is_err());
    run(&mut s, "conflict.edit", json!({"path": "README.md", "content": "main side\nfeature side\n"}));
    assert_eq!(std::fs::read_to_string(repo.root().join("README.md")).unwrap(), "main side\nfeature side\n");
    assert!(s.data.status.conflicted().next().is_none());
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
    // These would touch folders outside the test repository (ssh.generate writes to ~/.ssh).
    let skip = ["repo.open", "repo.init", "repo.clone", "repo.practice", "ssh.generate"];
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
    for id in skip.iter().filter(|i| **i != "ssh.generate") {
        for p in [json!({}), json!({"path": 5}), json!({"url": "--upload-pack=touch x", "path": ""})] {
            if let Err(e) = s.run(id, &p) {
                assert!(!e.to_string().contains("internal error"), "{id} {p}: {e}");
            }
        }
    }
}

#[test]
fn drag_branch_onto_branch() {
    let (_d, mut s) = session("eng-drop");
    let repo = s.repo.clone().unwrap();
    let head = repo.resolve("main").unwrap();
    run(&mut s, "branch.create", json!({"name": "behind", "from": "main~1", "checkout": false}));
    run(&mut s, "branch.create", json!({"name": "other", "from": "main~1", "checkout": false}));
    // Fast-forward a branch that is not checked out, then undo and redo it.
    run(&mut s, "branch.dropOnto", json!({"from": "main", "onto": "behind", "action": "fastForward"}));
    assert_eq!(repo.resolve("behind").as_deref(), Some(head.as_str()));
    run(&mut s, "edit.undo", json!({}));
    assert_ne!(repo.resolve("behind").as_deref(), Some(head.as_str()));
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(repo.resolve("behind").as_deref(), Some(head.as_str()));
    run(&mut s, "edit.undo", json!({}));
    // Merge into the checked-out branch.
    run(&mut s, "branch.dropOnto", json!({"from": "feature", "onto": "main", "action": "merge"}));
    assert!(repo.is_ancestor("feature", "main"));
    // Rebase a branch that is not checked out: it is checked out first.
    run(&mut s, "branch.dropOnto", json!({"from": "behind", "onto": "main", "action": "rebase"}));
    assert_eq!(s.data.status.branch.as_deref(), Some("behind"));
    assert!(repo.is_ancestor("main", "behind"));
    // Merge into a branch that is not checked out: it is checked out first.
    run(&mut s, "branch.dropOnto", json!({"from": "feature", "onto": "other", "action": "merge"}));
    assert_eq!(s.data.status.branch.as_deref(), Some("other"));
    assert!(repo.is_ancestor("feature", "other"));
    for bad in [
        json!({"from": "main", "onto": "main", "action": "merge"}),
        json!({"from": "main", "onto": "nope", "action": "merge"}),
        json!({"from": "v0.1.0", "onto": "main", "action": "rebase"}),
        json!({"from": "main", "onto": "behind", "action": "squash"}),
        json!({"from": "v0.1.0", "onto": "main", "action": "fastForward"}),
        json!({"from": "-x", "onto": "behind", "action": "merge"}),
    ] {
        assert!(s.run("branch.dropOnto", &bad).is_err(), "{bad}");
    }
}

#[test]
fn checking_out_a_remote_branch_tracks_or_fast_forwards() {
    let (d, up) = sample("eng-remote-checkout").unwrap();
    let clone = up.git().clone_repo(&up.root().to_string_lossy(), &d.path().join("clone")).unwrap();
    let mut s = Session::new(clone.git().clone());
    s.set_repo(clone.clone()).unwrap();
    // A remote-only branch: a local branch is made to track it.
    let r = run(&mut s, "branch.checkout", json!({"name": "origin/feature"}));
    assert_eq!(r["name"], json!("feature"));
    assert_eq!(s.data.status.branch.as_deref(), Some("feature"));
    // Upstream main moves on; after a fetch, checking out origin/main fast-forwards local main.
    write(&up, "NEW.md", "x\n").unwrap();
    gitcraft_git::testing::commit_all(&up, "Upstream").unwrap();
    run(&mut s, "remote.fetch", json!({}));
    let theirs = clone.resolve("origin/main").unwrap();
    let mine = clone.resolve("main").unwrap();
    assert_ne!(mine, theirs);
    let r = run(&mut s, "branch.checkout", json!({"name": "origin/main"}));
    assert!(r["message"].as_str().unwrap().contains("fast-forwarded"), "{r}");
    assert_eq!(s.data.status.branch.as_deref(), Some("main"));
    assert_eq!(clone.resolve("main").as_deref(), Some(theirs.as_str()));
    // Undo puts main back and returns to feature.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.branch.as_deref(), Some("feature"));
    assert_eq!(clone.resolve("main").as_deref(), Some(mine.as_str()));
    // Up to date: just a checkout.
    run(&mut s, "branch.checkout", json!({"name": "origin/feature"}));
    assert_eq!(s.data.status.branch.as_deref(), Some("feature"));
}

#[test]
fn pull_remote_branch_fetches_tracks_and_integrates() {
    let (d, up) = sample("eng-pull-remote").unwrap();
    let clone = up.git().clone_repo(&up.root().to_string_lossy(), &d.path().join("clone")).unwrap();
    let mut s = Session::new(clone.git().clone());
    s.set_repo(clone.clone()).unwrap();
    // A branch only on the remote: tracked and checked out.
    let r = run(&mut s, "branch.pullRemote", json!({"name": "origin/feature"}));
    assert_eq!(r["name"], json!("feature"));
    // Upstream main moves on: fetched and fast-forwarded without a separate fetch.
    write(&up, "A.md", "a\n").unwrap();
    gitcraft_git::testing::commit_all(&up, "Upstream A").unwrap();
    let r = run(&mut s, "branch.pullRemote", json!({"name": "origin/main"}));
    assert!(r["message"].as_str().unwrap().contains("fast-forward"), "{r}");
    assert_eq!(clone.resolve("main"), up.resolve("main"));
    assert_eq!(clone.config("branch.main.merge").as_deref(), Some("refs/heads/main"));
    // Both sides move: diverged, nothing changes until a mode is picked.
    write(&up, "B.md", "b\n").unwrap();
    gitcraft_git::testing::commit_all(&up, "Upstream B").unwrap();
    write(&clone, "C.md", "c\n").unwrap();
    let mine = gitcraft_git::testing::commit_all(&clone, "Local C").unwrap();
    let r = run(&mut s, "branch.pullRemote", json!({"name": "origin/main"}));
    assert_eq!(r["diverged"], json!(true), "{r}");
    assert_eq!((r["ahead"].as_u64(), r["behind"].as_u64(), r["unique"].as_u64()), (Some(1), Some(1), Some(1)));
    assert_eq!(clone.resolve("main").as_deref(), Some(mine.as_str()));
    run(&mut s, "branch.pullRemote", json!({"name": "origin/main", "mode": "merge"}));
    assert!(clone.is_ancestor("origin/main", "main") && clone.is_ancestor(&mine, "main"));
    // Reset moves the local branch to the remote one.
    run(&mut s, "branch.pullRemote", json!({"name": "origin/feature"}));
    write(&clone, "D.md", "d\n").unwrap();
    gitcraft_git::testing::commit_all(&clone, "Local D").unwrap();
    clone.run_ok(&["reset", "-q", "--hard", "HEAD~2"]).unwrap();
    write(&clone, "E.md", "e\n").unwrap();
    gitcraft_git::testing::commit_all(&clone, "Local E").unwrap();
    run(&mut s, "branch.pullRemote", json!({"name": "origin/feature", "mode": "reset"}));
    assert_eq!(clone.resolve("feature"), clone.resolve("origin/feature"));
    for bad in [json!({"name": "nope"}), json!({"name": "origin/nope"}), json!({"name": "origin/main", "mode": "squash"})] {
        assert!(s.run("branch.pullRemote", &bad).is_err(), "{bad}");
    }
}

#[test]
fn history_editing_squash_drop_move_and_undo() {
    let (_d, mut s) = session("eng-history");
    let repo = s.repo().unwrap().clone();
    for name in ["a", "b", "c"] {
        write(&repo, &format!("{name}.txt"), "x\n").unwrap();
        run(&mut s, "commit.stageAllAndCommit", json!({"summary": name}));
    }
    let summaries = |s: &Session| {
        s.repo()
            .unwrap()
            .log(&gitcraft_git::LogQuery { revs: vec!["HEAD".into()], ..Default::default() })
            .unwrap()
            .into_iter()
            .map(|c| c.summary)
            .collect::<Vec<_>>()
    };
    let head = s.data.status.head.clone().unwrap();
    let b = repo.resolve("HEAD~1").unwrap();
    let plan = run(&mut s, "rebase.plan", json!({"oid": b}));
    assert_eq!(plan["commits"].as_array().unwrap().len(), 2);
    // Squash c into b, then undo.
    run(&mut s, "commit.squash", json!({"oid": head, "message": "b and c"}));
    assert_eq!(summaries(&s)[..2], ["b and c", "a"]);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.data.status.head.as_deref(), Some(head.as_str()));
    run(&mut s, "edit.redo", json!({}));
    run(&mut s, "edit.undo", json!({}));
    // Drop b.
    run(&mut s, "commit.drop", json!({"oid": b}));
    assert_eq!(summaries(&s)[..2], ["c", "a"]);
    assert!(!repo.root().join("b.txt").exists());
    run(&mut s, "edit.undo", json!({}));
    // Move c down below b.
    run(&mut s, "commit.moveDown", json!({"oid": head}));
    assert_eq!(summaries(&s)[..3], ["b", "c", "a"]);
    run(&mut s, "edit.undo", json!({}));
    // A full todo list: reword a, fixup b into it, keep c.
    let a = repo.resolve("HEAD~2").unwrap();
    run(
        &mut s,
        "rebase.interactive",
        json!({"oid": a, "items": [
            {"action": "reword", "oid": a, "message": "A!"},
            {"action": "fixup", "oid": b},
            {"action": "pick", "oid": head},
        ]}),
    );
    assert_eq!(summaries(&s)[..2], ["c", "A!"]);
    assert!(repo.root().join("b.txt").exists());
    assert!(s.run("rebase.interactive", &json!({"oid": a, "items": "x"})).is_err());
    assert!(s.run("commit.squash", &json!({"oid": repo.resolve("HEAD~3").unwrap()})).is_err(), "the root commit");
}

#[test]
fn whitespace_diffs_file_content_and_view_toggles() {
    let (_d, mut s) = session("eng-ws");
    let repo = s.repo().unwrap().clone();
    write(&repo, "src/main.rs", "fn main() {\n        println!(\"hello, world\");\n}\n").unwrap();
    run(&mut s, "repo.refresh", json!({}));
    let d = run(&mut s, "diff.file", json!({"path": "src/main.rs"}));
    assert_eq!(d["hunks"].as_array().unwrap().len(), 1);
    let d = run(&mut s, "diff.file", json!({"path": "src/main.rs", "ignoreWhitespace": true}));
    assert!(d["hunks"].as_array().unwrap().is_empty(), "{d}");
    run(&mut s, "view.ignoreWhitespace", json!({"value": true}));
    run(&mut s, "file.select", json!({"path": "src/main.rs", "area": "unstaged"}));
    assert!(s.selected_diff().unwrap().unwrap().hunks.is_empty());
    run(&mut s, "view.ignoreWhitespace", json!({}));
    assert!(!s.view.ignore_whitespace);
    assert_eq!(s.selected_diff().unwrap().unwrap().hunks.len(), 1);
    assert_eq!(run(&mut s, "view.wordDiff", json!({}))["value"], json!(false));
    assert_eq!(run(&mut s, "view.syntax", json!({"value": false}))["value"], json!(false));
    // File content at a revision, in the index and in the work tree.
    let c = run(&mut s, "file.content", json!({"path": "README.md"}));
    assert!(c["text"].as_str().unwrap().starts_with("# Sample"));
    let c = run(&mut s, "file.content", json!({"path": "src/main.rs", "rev": "worktree"}));
    assert!(c["text"].as_str().unwrap().contains("        println"));
    let c = run(&mut s, "file.content", json!({"path": "src/main.rs", "rev": "index"}));
    assert!(!c["text"].as_str().unwrap().contains("        println"));
    for bad in
        [json!({"path": "../x", "rev": "worktree"}), json!({"path": "/etc/passwd", "rev": "worktree"}), json!({"path": "README.md", "rev": "-x"})]
    {
        assert!(s.run("file.content", &bad).is_err(), "{bad}");
    }
}

#[test]
fn solo_hide_columns_and_compare() {
    let (_d, mut s) = session("eng-graphfilter");
    assert_eq!(s.data.commits.len(), 3);
    // feature has one commit main doesn't.
    run(&mut s, "graph.hide", json!({"name": "feature"}));
    assert_eq!(s.data.commits.len(), 2);
    assert!(!s.data.labels.values().flatten().any(|&i| s.data.refs[i].name == "feature"));
    run(&mut s, "graph.hide", json!({"name": "feature"}));
    assert_eq!(s.data.commits.len(), 3);
    assert!(s.run("graph.hide", &json!({"name": "main"})).is_err(), "the checked-out branch");
    run(&mut s, "graph.solo", json!({"name": "main"}));
    assert_eq!(s.data.commits.len(), 2);
    run(&mut s, "graph.solo", json!({"name": "feature", "add": true}));
    assert_eq!(s.data.commits.len(), 3);
    run(&mut s, "graph.showAll", json!({}));
    assert!(s.view.solo.is_empty());
    run(&mut s, "graph.refs", json!({"tags": false}));
    assert!(!s.data.labels.values().flatten().any(|&i| s.data.refs[i].kind == gitcraft_git::RefKind::Tag));
    run(&mut s, "graph.refs", json!({"tags": true}));
    assert_eq!(run(&mut s, "graph.columns", json!({"sha": false}))["columns"]["sha"], json!(false));
    assert!(s.run("graph.solo", &json!({"name": "nope"})).is_err());
    // Compare two commits, then a commit with the work tree.
    let r = run(&mut s, "diff.compare", json!({"from": "v0.1.0", "to": "feature"}));
    assert_eq!(r["files"][0]["path"], json!("src/lib.rs"));
    run(&mut s, "file.select", json!({"path": "src/lib.rs", "area": "compare"}));
    assert_eq!(s.selected_diff().unwrap().unwrap().added(), 3);
    write(s.repo().unwrap(), "README.md", "changed\n").unwrap();
    let r = run(&mut s, "diff.compare", json!({"from": "HEAD"}));
    assert_eq!(r["files"][0]["path"], json!("README.md"));
    assert!(s.sel.file.is_none());
    run(&mut s, "file.select", json!({"path": "README.md", "area": "compare"}));
    assert_eq!(s.selected_diff().unwrap().unwrap().added(), 1);
    run(&mut s, "diff.compareClear", json!({}));
    assert!(s.sel.file.is_none() && s.view.compare.is_none());
    assert!(s.run("diff.compare", &json!({"from": "HEAD", "to": "HEAD"})).is_err());
    assert!(s.run("file.select", &json!({"path": "x", "area": "compare"})).is_err());
}

#[test]
fn identity_signing_template_and_commit_options() {
    let (_d, mut s) = session("eng-config");
    let repo = s.repo().unwrap().clone();
    let r = run(&mut s, "config.identity", json!({"name": "Grace Hopper", "email": "grace@example.com"}));
    assert_eq!(r["local"]["name"], json!("Grace Hopper"));
    assert_eq!(r["effective"]["email"], json!("grace@example.com"));
    run(&mut s, "config.identity", json!({"scope": "global", "name": "-Global Name"}));
    assert_eq!(repo.config_in(gitcraft_git::ConfigScope::Global, "user.name").as_deref(), Some("-Global Name"));
    assert!(s.run("config.identity", &json!({"email": "not-an-address"})).is_err());
    run(&mut s, "config.identity", json!({"name": ""}));
    assert_eq!(repo.config_in(gitcraft_git::ConfigScope::Local, "user.name"), None);
    // Signing settings round-trip; a commit can opt out of signing.
    let r = run(&mut s, "config.signing", json!({"format": "ssh", "key": "~/.ssh/id_ed25519.pub", "signCommits": false}));
    assert_eq!(r["format"], json!("ssh"));
    assert!(s.run("config.signing", &json!({"format": "pgp2"})).is_err());
    assert_eq!(run(&mut s, "commit.sign", json!({}))["value"], json!(true));
    run(&mut s, "commit.sign", json!({"value": false}));
    run(&mut s, "commit.noVerify", json!({"value": true}));
    // A failing pre-commit hook is skipped.
    let hooks = repo.git_dir().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(hooks.join("pre-commit"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    run(&mut s, "commit.coAuthors", json!({"authors": ["Ada Lovelace <ada@example.org>"]}));
    assert!(s.run("commit.coAuthors", &json!({"authors": ["no address"]})).is_err());
    write(&repo, "x.txt", "x\n").unwrap();
    let r = run(&mut s, "commit.stageAllAndCommit", json!({"summary": "With a hook"}));
    let c = repo.find_commit(r["oid"].as_str().unwrap()).unwrap();
    assert!(c.body.contains("Co-authored-by: Ada Lovelace <ada@example.org>"), "{}", c.body);
    assert!(s.draft.no_verify && s.draft.co_authors.is_empty(), "choices stay, co-authors go");
    #[cfg(unix)]
    {
        run(&mut s, "commit.noVerify", json!({"value": false}));
        write(&repo, "y.txt", "y\n").unwrap();
        assert!(s.run("commit.stageAllAndCommit", &json!({"summary": "Blocked"})).is_err(), "the hook runs again");
    }
    // A commit template fills an empty message.
    std::fs::write(repo.root().join(".tmpl"), "# comment\nfeat: \n\nWhy:\n").unwrap();
    let r = run(&mut s, "commit.template", json!({"path": ".tmpl"}));
    assert_eq!(r["draft"]["summary"], json!("feat:"));
    assert_eq!(s.draft.description, "Why:");
    assert!(run(&mut s, "ssh.keys", json!({}))["keys"].is_array());
    assert!(s.run("ssh.generate", &json!({"name": "../x"})).is_err());
}

#[test]
fn worktrees_from_commands() {
    let (d, mut s) = session("eng-worktree");
    assert!(s.data.worktrees.is_empty());
    let path = d.path().join("second").to_string_lossy().into_owned();
    run(&mut s, "worktree.add", json!({"path": path, "branch": "wt-branch", "newBranch": true}));
    assert_eq!(s.data.worktrees.len(), 2);
    assert!(s.data.local_branches().any(|r| r.name == "wt-branch"));
    let r = run(&mut s, "worktree.list", json!({}));
    assert_eq!(r["worktrees"].as_array().unwrap().len(), 2);
    let root = s.repo().unwrap().root().to_string_lossy().into_owned();
    assert!(s.run("worktree.remove", &json!({"path": root})).is_err());
    run(&mut s, "worktree.remove", json!({"path": path}));
    assert!(s.data.worktrees.is_empty());
    assert!(s.run("submodule.update", &json!({})).is_err(), "no submodules");
    assert_eq!(run(&mut s, "submodule.list", json!({}))["submodules"], json!([]));
}

#[test]
fn tools_ignore_edit_patch_hooks() {
    let (d, mut s) = session("eng-tools");
    let repo = s.repo().unwrap().clone();
    write(&repo, "build/out.log", "x\n").unwrap();
    write(&repo, "notes.tmp", "x\n").unwrap();
    run(&mut s, "repo.refresh", json!({}));
    run(&mut s, "file.ignore", json!({"path": "build/out.log"}));
    run(&mut s, "file.ignore", json!({"path": "notes.tmp", "extension": true, "local": true}));
    let left: Vec<&str> = s.data.status.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(left, [".gitignore"]);
    assert!(s.run("file.ignore", &json!({"path": "../x"})).is_err());
    run(&mut s, "file.edit", json!({"path": "src/new.rs", "content": "fn x() {}\n"}));
    assert!(s.data.status.file("src/new.rs").is_some());
    assert!(s.run("file.edit", &json!({"path": "/etc/x", "content": ""})).is_err());
    assert!(s.run("file.edit", &json!({"path": "src", "content": ""})).is_err(), "a folder");
    // A patch of a commit, written to a file and applied on another branch.
    let out = d.path().join("feature.patch").to_string_lossy().into_owned();
    run(&mut s, "patch.create", json!({"oid": "feature", "path": out}));
    assert!(s.run("patch.create", &json!({"oid": "feature", "path": out})).is_err(), "never overwrites");
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "wip"}));
    run(&mut s, "patch.apply", json!({"path": out}));
    assert_eq!(s.data.commits[0].summary, "Add a library");
    let text = run(&mut s, "patch.create", json!({"oid": "HEAD"}))["patch"].as_str().unwrap().to_string();
    assert!(text.contains("Subject: [PATCH] Add a library"));
    // Hooks.
    run(&mut s, "hooks.write", json!({"name": "commit-msg", "script": "#!/bin/sh\nexit 0\n"}));
    let h = run(&mut s, "hooks.list", json!({}));
    assert!(h["hooks"].as_array().unwrap().iter().any(|x| x["name"] == "commit-msg" && x["enabled"] == json!(true)));
    run(&mut s, "hooks.enable", json!({"name": "commit-msg", "value": false}));
    assert!(s.run("tool.diff", &json!({"path": "README.md"})).is_err(), "no diff.tool set");
    let l = run(&mut s, "lfs.list", json!({}));
    assert!(l["patterns"].as_array().unwrap().is_empty());
}

#[test]
fn git_flow_feature_and_release() {
    let (_d, mut s) = session("eng-flow");
    assert!(s.run("gitflow.start", &json!({"kind": "feature", "name": "x"})).is_err(), "not set up");
    // The sample has a branch named `feature`, so feature branches can't live under `feature/`.
    run(&mut s, "gitflow.init", json!({"featurePrefix": "feat/"}));
    assert!(s.data.local_branches().any(|r| r.name == "develop"));
    run(&mut s, "gitflow.start", json!({"kind": "feature", "name": "login"}));
    assert_eq!(s.data.status.branch.as_deref(), Some("feat/login"));
    write(s.repo().unwrap(), "login.txt", "x\n").unwrap();
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "Login"}));
    let r = run(&mut s, "gitflow.finish", json!({"kind": "feature"}));
    assert!(r["message"].as_str().unwrap().contains("deleted"), "{r}");
    assert_eq!(s.data.status.branch.as_deref(), Some("develop"));
    assert!(!s.data.local_branches().any(|r| r.name == "feat/login"));
    run(&mut s, "gitflow.start", json!({"kind": "release", "name": "1.0"}));
    run(&mut s, "gitflow.finish", json!({"kind": "release", "name": "1.0"}));
    let repo = s.repo().unwrap().clone();
    assert!(s.data.tags().any(|t| t.name == "1.0"));
    assert!(repo.is_ancestor("1.0", "main") && repo.is_ancestor("1.0", "develop"));
    assert!(repo.root().join("login.txt").exists());
    assert!(s.run("gitflow.start", &json!({"kind": "bugfix", "name": "x"})).is_err());
}

#[test]
fn stash_extras_multi_select_and_remotes() {
    let (d, mut s) = session("eng-extras");
    let repo = s.repo().unwrap().clone();
    write(&repo, "README.md", "a\n").unwrap();
    write(&repo, "src/main.rs", "b\n").unwrap();
    run(&mut s, "repo.refresh", json!({}));
    run(&mut s, "stash.save", json!({"message": "only readme", "paths": ["README.md"]}));
    assert!(s.data.status.file("README.md").is_none());
    assert!(s.data.status.file("src/main.rs").is_some());
    run(&mut s, "stash.save", json!({"message": "the rest"}));
    run(&mut s, "stash.rename", json!({"index": 1, "message": "renamed"}));
    assert_eq!(s.data.stashes.len(), 2);
    assert!(s.data.stashes[0].message.contains("renamed"), "{:?}", s.data.stashes);
    // A stash's changes, through compare.
    let r = run(&mut s, "diff.compare", json!({"from": "stash@{0}^1", "to": "stash@{0}"}));
    assert_eq!(r["files"][0]["path"], json!("README.md"));
    run(&mut s, "diff.compareClear", json!({}));
    // Delete several branches at once; undo brings both back.
    run(&mut s, "branch.create", json!({"name": "a", "checkout": false}));
    run(&mut s, "branch.create", json!({"name": "b", "checkout": false}));
    let r = run(&mut s, "branch.delete", json!({"names": ["a", "b", "nope"]}));
    assert_eq!(r["deleted"], json!(["a", "b"]));
    run(&mut s, "edit.undo", json!({}));
    assert!(s.data.local_branches().any(|r| r.name == "a") && s.data.local_branches().any(|r| r.name == "b"));
    // Cherry-pick several commits.
    run(&mut s, "branch.create", json!({"name": "two", "from": "feature"}));
    write(&repo, "t.txt", "t\n").unwrap();
    run(&mut s, "commit.stageAllAndCommit", json!({"summary": "T"}));
    let picks = vec![repo.resolve("two~1").unwrap(), repo.resolve("two").unwrap()];
    run(&mut s, "branch.checkout", json!({"name": "main"}));
    run(&mut s, "cherryPick.commit", json!({"oids": picks}));
    assert_eq!(s.data.commits[0].summary, "T");
    // Remotes: rename, change the URL.
    let up = d.path().join("up").to_string_lossy().into_owned();
    run(&mut s, "remote.add", json!({"name": "origin", "url": up}));
    run(&mut s, "remote.editUrl", json!({"name": "origin", "url": "/elsewhere"}));
    assert_eq!(repo.config("remote.origin.url").as_deref(), Some("/elsewhere"));
    run(&mut s, "remote.rename", json!({"from": "origin", "to": "upstream"}));
    assert_eq!(s.data.remotes[0].name, "upstream");
}

#[test]
fn scanning_finds_repositories() {
    let (d, mut s) = session("eng-scan");
    let base = d.path();
    let _ = gitcraft_git::testing::isolated_git(base).unwrap().init(&base.join("a/b/deep"), Some("main")).unwrap();
    std::fs::create_dir_all(base.join("node_modules/x/.git")).unwrap();
    let r = run(&mut s, "repo.scan", json!({"path": base.display().to_string()}));
    let found: Vec<String> = r["repositories"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
    assert!(found.iter().any(|f| f.ends_with("work")), "{found:?}");
    assert!(found.iter().any(|f| f.ends_with("deep")), "{found:?}");
    assert!(!found.iter().any(|f| f.contains("node_modules")));
    let r = run(&mut s, "repo.scan", json!({"path": base.display().to_string(), "depth": 1}));
    assert!(!r["repositories"].as_array().unwrap().iter().any(|f| f.as_str().unwrap().ends_with("deep")));
    assert!(s.run("repo.scan", &json!({"path": "/no/such/folder"})).is_err());
}

#[test]
fn practice_repository() {
    let (d, mut s) = session("eng-practice");
    let path = d.path().join("practice");
    run(&mut s, "repo.practice", json!({"path": path.display().to_string()}));
    assert_eq!(s.data.commits.len(), 3);
    assert!(s.data.local_branches().any(|r| r.name == "try-something"));
    assert_eq!(s.data.status.unstaged().count(), 1, "a change waiting to be committed");
    assert!(s.run("repo.practice", &json!({"path": path.display().to_string()})).is_err(), "not empty");
}
