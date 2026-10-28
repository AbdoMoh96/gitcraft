//! The Git client feature catalog: the incumbent's toolbar, panel and menu features (feature names
//! only, from its public documentation and black-box use) mapped to our command ids. `parity()`
//! compares it with the registry; the gap is the work list (`cargo xtask parity` →
//! docs/parity-checklist.md).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::Registry;

/// `Area|Group|Feature|command id`
pub const CATALOG: &str = "\
Toolbar|Actions|Undo|edit.undo
Toolbar|Actions|Redo|edit.redo
Toolbar|Actions|Pull|remote.pull
Toolbar|Actions|Pull (fast-forward if possible)|remote.pullFastForward
Toolbar|Actions|Pull (rebase)|remote.pullRebase
Toolbar|Actions|Fetch All|remote.fetch
Toolbar|Actions|Push|remote.push
Toolbar|Actions|Force Push|remote.forcePush
Toolbar|Actions|Branch|branch.create
Toolbar|Actions|Stash|stash.save
Toolbar|Actions|Pop|stash.pop
Toolbar|Actions|Terminal|ui.terminal
Toolbar|Actions|Open Repository Folder|ui.revealRepo
Repository|Management|Open Repository|repo.open
Repository|Management|Clone Repository|repo.clone
Repository|Management|Init Repository|repo.init
Repository|Management|Close Repository|repo.close
Repository|Management|Refresh|repo.refresh
Repository|Management|Recent Repositories|ui.recent
Repository|Management|Repository Tabs|ui.tabs
Repository|Management|Workspaces|workspace.open
Repository|Management|Profiles|profile.switch
Repository|Management|Author Identity|config.identity
Graph|Navigation|Commit Graph|graph.select
Graph|Navigation|Next Commit|graph.selectNext
Graph|Navigation|Previous Commit|graph.selectPrevious
Graph|Navigation|Go to HEAD|graph.selectHead
Graph|Navigation|Load More Commits|graph.loadMore
Graph|Navigation|Search Commits|view.search
Graph|Navigation|Solo Branch|graph.solo
Graph|Navigation|Hide Branch|graph.hide
Graph|Navigation|Graph Columns|graph.columns
Graph|Navigation|Commit Details|commit.show
Graph|Commit Menu|Checkout Commit|commit.checkout
Graph|Commit Menu|Create Branch Here|branch.create
Graph|Commit Menu|Cherry Pick Commit|cherryPick.commit
Graph|Commit Menu|Revert Commit|revert.commit
Graph|Commit Menu|Reset to Commit (soft, mixed, hard)|reset.to
Graph|Commit Menu|Create Tag Here|tag.create
Graph|Commit Menu|Copy Commit SHA|ui.copySha
Graph|Commit Menu|Interactive Rebase|rebase.interactive
Graph|Commit Menu|Edit Commit Message|commit.reword
Graph|Commit Menu|Squash Commits|commit.squash
Graph|Commit Menu|Drop Commit|commit.drop
Graph|Commit Menu|Compare Commits|diff.compare
Graph|Drag and Drop|Drag a Branch onto a Branch (merge, rebase, fast-forward)|branch.dropOnto
Branches|Menu|Checkout Branch|branch.checkout
Branches|Menu|Merge into Current Branch|merge.into
Branches|Menu|Fast-forward|merge.fastForward
Branches|Menu|Rebase Current Branch onto|rebase.onto
Branches|Menu|Rename Branch|branch.rename
Branches|Menu|Delete Branch|branch.delete
Branches|Menu|Delete Remote Branch|branch.deleteRemote
Branches|Menu|Set Upstream|branch.setUpstream
Branches|Menu|Create Pull Request|pr.create
Left Panel|Sections|Local Branches|refs.list
Left Panel|Sections|Add Remote|remote.add
Left Panel|Sections|Remove Remote|remote.remove
Left Panel|Sections|Delete Tag|tag.delete
Left Panel|Sections|Push Tag|tag.push
Left Panel|Sections|Apply Stash|stash.apply
Left Panel|Sections|Delete Stash|stash.drop
Left Panel|Sections|Filter Branches|ui.filterRefs
Left Panel|Sections|Toggle Left Panel|view.leftPanel
Left Panel|Sections|Pull Requests|pr.list
Left Panel|Sections|Issues|issues.list
Left Panel|Sections|Submodules|submodule.list
Left Panel|Sections|Worktrees|worktree.list
Left Panel|Sections|CI Status|ci.list
Commit Panel|Staging|Stage File|stage.file
Commit Panel|Staging|Unstage File|unstage.file
Commit Panel|Staging|Stage All Changes|stage.all
Commit Panel|Staging|Unstage All Changes|unstage.all
Commit Panel|Staging|Discard File Changes|discard.file
Commit Panel|Staging|Discard All Changes|discard.all
Commit Panel|Staging|Ignore File|file.ignore
Commit Panel|Commit|Summary and Description|commit.message
Commit Panel|Commit|Amend Previous Commit|commit.amend
Commit Panel|Commit|Commit|commit.create
Commit Panel|Commit|Stage All and Commit|commit.stageAllAndCommit
Commit Panel|Commit|Sign Commits|commit.sign
Commit Panel|Commit|Commit Template|commit.template
Commit Panel|Commit|Generate Commit Message|commit.generateMessage
Commit Panel|Commit|Skip Hooks|commit.noVerify
Diff View|Changes|Open File Diff|file.select
Diff View|Changes|Close Diff|file.close
Diff View|Changes|Hunk, Inline and Split Views|view.diffMode
Diff View|Changes|Context Lines|view.context
Diff View|Changes|Stage Hunk|stage.hunk
Diff View|Changes|Unstage Hunk|unstage.hunk
Diff View|Changes|Discard Hunk|discard.hunk
Diff View|Changes|Stage Lines|stage.lines
Diff View|Changes|Unstage Lines|unstage.lines
Diff View|Changes|Discard Lines|discard.lines
Diff View|Changes|File Blame|file.blame
Diff View|Changes|Blame View|view.blame
Diff View|Changes|File History|view.history
Diff View|Changes|Edit File|file.edit
Diff View|Changes|Ignore Whitespace|view.ignoreWhitespace
Diff View|Changes|Word Highlighting|view.wordDiff
Diff View|Changes|Syntax Highlighting|view.syntax
Merge|Conflicts|Conflict Detection|repo.status
Merge|Conflicts|Conflict Prevention|conflict.predict
Merge|Conflicts|Take Ours|conflict.takeOurs
Merge|Conflicts|Take Theirs|conflict.takeTheirs
Merge|Conflicts|Mark Resolved|conflict.markResolved
Merge|Conflicts|Conflict Versions|conflict.versions
Merge|Conflicts|Merge Tool Output Editor|conflict.edit
Merge|Conflicts|Continue|op.continue
Merge|Conflicts|Abort|op.abort
Merge|Conflicts|Skip Commit|rebase.skip
Tools|General|Command Palette|ui.commandPalette
Tools|General|Keyboard Shortcuts|ui.shortcuts
Tools|General|Activity Log|repo.activity
Tools|General|Commit History Query|graph.log
Tools|General|Git LFS|lfs.track
Tools|General|Git Flow|gitflow.init
Tools|General|Git Hooks|hooks.list
Tools|General|Create Patch|patch.create
Tools|General|Apply Patch|patch.apply
Settings|Preferences|Interface Theme|ui.theme
Settings|Preferences|Preferences|ui.preferences
Integrations|Hosting|Hosting Service Accounts|integrations.connect
Integrations|Hosting|Cloud Patches|cloudpatch.create
Integrations|Hosting|Launchpad|launchpad.open
";

/// Commands implemented by the UI (`ui.*`), not the engine. `gitcraft-ui-egui` checks that it
/// handles each of these.
pub const UI_COMMANDS: &[&str] = &[
    "ui.revealRepo",
    "ui.terminal",
    "ui.recent",
    "ui.tabs",
    "ui.copySha",
    "ui.filterRefs",
    "ui.commandPalette",
    "ui.shortcuts",
    "ui.theme",
    "ui.preferences",
];

pub struct Entry {
    pub area: &'static str,
    pub group: &'static str,
    pub feature: &'static str,
    pub id: &'static str,
}

pub fn entries() -> Vec<Entry> {
    CATALOG
        .lines()
        .filter_map(|l| {
            let mut f = l.split('|');
            Some(Entry { area: f.next()?, group: f.next()?, feature: f.next()?, id: f.next()? })
        })
        .collect()
}

fn implemented(reg: &Registry, id: &str) -> bool {
    reg.get(id).is_some() || UI_COMMANDS.contains(&id)
}

/// Implemented / total, overall and per area, and the missing features.
pub fn parity(reg: &Registry) -> Value {
    let all = entries();
    let done = all.iter().filter(|e| implemented(reg, e.id)).count();
    let mut areas: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for e in &all {
        let a = areas.entry(e.area).or_default();
        a.1 += 1;
        if implemented(reg, e.id) {
            a.0 += 1;
        }
    }
    let missing: Vec<Value> =
        all.iter().filter(|e| !implemented(reg, e.id)).map(|e| json!({"area": e.area, "group": e.group, "feature": e.feature, "id": e.id})).collect();
    json!({
        "implemented": done,
        "total": all.len(),
        "percent": if all.is_empty() { 0.0 } else { (done as f64 * 1000.0 / all.len() as f64).round() / 10.0 },
        "areas": areas.iter().map(|(k, (d, t))| json!({"area": k, "implemented": d, "total": t})).collect::<Vec<_>>(),
        "missing": missing,
    })
}

/// The parity checklist as Markdown (docs/parity-checklist.md).
pub fn markdown(reg: &Registry) -> String {
    let p = parity(reg);
    let mut out = String::from("# Feature parity checklist\n\n");
    out.push_str("Generated by `cargo xtask parity` from `crates/engine/src/catalog.rs`: the incumbent Git client's features (names only) and the GitCraft command that implements each.\n\n");
    out.push_str(&format!("**{} of {} features ({}%).**\n\n| Area | Implemented |\n|---|---|\n", p["implemented"], p["total"], p["percent"]));
    if let Some(a) = p["areas"].as_array() {
        for x in a {
            out.push_str(&format!("| {} | {} / {} |\n", x["area"].as_str().unwrap_or(""), x["implemented"], x["total"]));
        }
    }
    let mut area = "";
    for e in entries() {
        if e.area != area {
            area = e.area;
            out.push_str(&format!("\n## {area}\n\n| Feature | Command | |\n|---|---|---|\n"));
        }
        let mark = if implemented(reg, e.id) { "✅" } else { "⬜" };
        out.push_str(&format!("| {} › {} | `{}` | {mark} |\n", e.group, e.feature, e.id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parity only rises: raise this floor when features land.
    const FLOOR: usize = 95;

    #[test]
    fn parity_floor() {
        let reg = crate::cmd::registry();
        let p = parity(&reg);
        let done = p["implemented"].as_u64().unwrap_or(0) as usize;
        assert!(done >= FLOOR, "parity fell to {done} (floor {FLOOR}); missing: {}", p["missing"]);
    }

    #[test]
    fn catalog_is_well_formed() {
        let lines = CATALOG.lines().count();
        assert_eq!(entries().len(), lines, "every catalog line has four fields");
        for e in entries() {
            assert!(!e.id.is_empty() && !e.feature.is_empty());
        }
    }
}
