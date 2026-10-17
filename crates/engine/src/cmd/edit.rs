//! Undo and redo of repository actions: commits, checkouts, branch and tag changes, resets.
//!
//! Each undoable command records an [`UndoStep`] saying how to take it back and how to do it
//! again. Steps check that the repository is still where they left it before acting.

use gitcraft_git::{GitError, Repo, ResetMode};
use serde_json::{Value, json};

use super::need_repo;
use crate::{CmdError, CmdResult, CommandSpec, Session};

/// Where HEAD is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoStep {
    Commit {
        before: Option<String>,
        after: String,
        branch: Option<String>,
        amend: bool,
    },
    Checkout {
        from: Head,
        to: Head,
    },
    BranchCreate {
        name: String,
        oid: String,
    },
    BranchDelete {
        name: String,
        oid: String,
    },
    BranchRename {
        from: String,
        to: String,
    },
    TagCreate {
        name: String,
        oid: String,
    },
    TagDelete {
        name: String,
        oid: String,
    },
    Reset {
        before: String,
        after: String,
        mode: ResetMode,
    },
    /// A commit message edited: HEAD's branch moved from `before` to `after`.
    Reword {
        before: String,
        after: String,
    },
    /// Several steps taken as one (create a branch and check it out).
    Many(String, Vec<UndoStep>),
}

/// Steps kept.
const MAX_UNDO: usize = 100;

fn changed() -> CmdError {
    CmdError::Failed("the repository has changed since; this can no longer be undone".into())
}

fn head_is(repo: &Repo, oid: &str) -> Result<(), CmdError> {
    if repo.resolve("HEAD").as_deref() == Some(oid) { Ok(()) } else { Err(changed()) }
}

fn go(repo: &Repo, h: &Head) -> Result<(), GitError> {
    match h {
        Head::Branch(b) => repo.checkout_branch(b),
        Head::Detached(o) => repo.checkout_detached(o),
    }
}

fn reset_flag(mode: ResetMode) -> &'static str {
    match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        // Going back over a hard reset keeps whatever changed since, or refuses.
        ResetMode::Hard => "--keep",
    }
}

impl UndoStep {
    pub fn label(&self) -> String {
        match self {
            UndoStep::Commit { amend: true, .. } => "Amend".into(),
            UndoStep::Commit { .. } => "Commit".into(),
            UndoStep::Checkout { to: Head::Branch(b), .. } => format!("Checkout {b}"),
            UndoStep::Checkout { .. } => "Checkout".into(),
            UndoStep::BranchCreate { name, .. } => format!("Create branch {name}"),
            UndoStep::BranchDelete { name, .. } => format!("Delete branch {name}"),
            UndoStep::BranchRename { to, .. } => format!("Rename branch to {to}"),
            UndoStep::TagCreate { name, .. } => format!("Create tag {name}"),
            UndoStep::TagDelete { name, .. } => format!("Delete tag {name}"),
            UndoStep::Reset { .. } => "Reset".into(),
            UndoStep::Reword { .. } => "Edit commit message".into(),
            UndoStep::Many(label, _) => label.clone(),
        }
    }

    fn undo(&self, repo: &Repo) -> Result<(), CmdError> {
        match self {
            UndoStep::Commit { before, after, branch, .. } => {
                head_is(repo, after)?;
                match (before, branch) {
                    (Some(b), _) => repo.run_ok(&["reset", "-q", "--soft", b.as_str()])?,
                    // Undoing the first commit: the branch goes back to being unborn.
                    (None, Some(br)) => repo.run_ok(&["update-ref", "-d", &format!("refs/heads/{br}")])?,
                    (None, None) => return Err(changed()),
                };
            }
            UndoStep::Checkout { from, .. } => go(repo, from)?,
            UndoStep::BranchCreate { name, oid } => {
                if repo.resolve(&format!("refs/heads/{name}")).as_deref() != Some(oid) {
                    return Err(changed());
                }
                repo.delete_branch(name, true)?;
            }
            UndoStep::BranchDelete { name, oid } => {
                repo.create_branch(name, Some(oid), false)?;
            }
            UndoStep::BranchRename { from, to } => {
                repo.rename_branch(to, from)?;
            }
            UndoStep::TagCreate { name, .. } => repo.delete_tag(name)?,
            UndoStep::TagDelete { name, oid } => {
                repo.create_tag(name, Some(oid), None)?;
            }
            UndoStep::Reset { before, after, mode } => {
                head_is(repo, after)?;
                repo.run_ok(&["reset", "-q", reset_flag(*mode), before.as_str()])?;
            }
            UndoStep::Reword { before, after } => {
                head_is(repo, after)?;
                repo.run_ok(&["update-ref", "-m", "gitcraft: undo edit commit message", "HEAD", before.as_str(), after.as_str()])?;
            }
            UndoStep::Many(_, steps) => {
                for st in steps.iter().rev() {
                    st.undo(repo)?;
                }
            }
        }
        Ok(())
    }

    fn redo(&self, repo: &Repo) -> Result<(), CmdError> {
        match self {
            UndoStep::Commit { before, after, branch, .. } => {
                match (before, branch) {
                    (Some(b), _) => {
                        head_is(repo, b)?;
                        repo.run_ok(&["reset", "-q", "--soft", after.as_str()])?;
                    }
                    (None, Some(br)) => {
                        repo.run_ok(&["update-ref", &format!("refs/heads/{br}"), after.as_str()])?;
                    }
                    (None, None) => return Err(changed()),
                };
            }
            UndoStep::Checkout { to, .. } => go(repo, to)?,
            UndoStep::BranchCreate { name, oid } => {
                repo.create_branch(name, Some(oid), false)?;
            }
            UndoStep::BranchDelete { name, .. } => repo.delete_branch(name, true)?,
            UndoStep::BranchRename { from, to } => {
                repo.rename_branch(from, to)?;
            }
            UndoStep::TagCreate { name, oid } => {
                repo.create_tag(name, Some(oid), None)?;
            }
            UndoStep::TagDelete { name, .. } => repo.delete_tag(name)?,
            UndoStep::Reset { before, after, mode } => {
                head_is(repo, before)?;
                repo.run_ok(&["reset", "-q", reset_flag(*mode), after.as_str()])?;
            }
            UndoStep::Reword { before, after } => {
                head_is(repo, before)?;
                repo.run_ok(&["update-ref", "-m", "gitcraft: redo edit commit message", "HEAD", after.as_str(), before.as_str()])?;
            }
            UndoStep::Many(_, steps) => {
                for st in steps {
                    st.redo(repo)?;
                }
            }
        }
        Ok(())
    }
}

impl Session {
    /// Record an undoable step (clears redo).
    pub(crate) fn push_undo(&mut self, step: UndoStep) {
        self.redo.clear();
        self.undo.push(step);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
    }

    /// Where HEAD is now.
    pub(crate) fn head(&self) -> Option<Head> {
        let repo = self.repo.as_ref()?;
        let st = repo.status().ok()?;
        match (st.branch, st.head) {
            (Some(b), _) => Some(Head::Branch(b)),
            (None, Some(o)) => Some(Head::Detached(o)),
            _ => None,
        }
    }

    pub fn can_undo(&self) -> Option<String> {
        self.undo.last().map(UndoStep::label)
    }

    pub fn can_redo(&self) -> Option<String> {
        self.redo.last().map(UndoStep::label)
    }
}

fn undo(s: &mut Session, _: &Value) -> CmdResult {
    let step = s.undo.last().cloned().ok_or_else(|| CmdError::Disabled("nothing to undo".into()))?;
    step.undo(s.repo()?)?;
    s.undo.pop();
    let label = step.label();
    s.redo.push(step);
    Ok(json!({"message": format!("Undid: {label}")}))
}

fn redo(s: &mut Session, _: &Value) -> CmdResult {
    let step = s.redo.last().cloned().ok_or_else(|| CmdError::Disabled("nothing to redo".into()))?;
    step.redo(s.repo()?)?;
    s.redo.pop();
    let label = step.label();
    s.undo.push(step);
    Ok(json!({"message": format!("Redid: {label}")}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("edit.undo", "Undo", "Toolbar", undo)
            .key("Mod+Z")
            .when(|s| need_repo(s).or_else(|| s.undo.is_empty().then_some("nothing to undo"))),
        CommandSpec::new("edit.redo", "Redo", "Toolbar", redo)
            .key(super::mac_or("Mod+Shift+Z", "Mod+Y / Mod+Shift+Z"))
            .when(|s| need_repo(s).or_else(|| s.redo.is_empty().then_some("nothing to redo"))),
    ]
}
