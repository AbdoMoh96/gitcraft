//! Interactive rebase without an editor: GitCraft writes the todo list itself and git takes it
//! through `GIT_SEQUENCE_EDITOR` (a copy of the prepared file over git's). New messages for
//! reworded or squashed commits are applied by `exec` lines that amend from a message file, so
//! git never opens an editor and never prompts.

use serde::Serialize;

use crate::log::Commit;
use crate::ops::Outcome;
use crate::{GitError, Repo, Result, check_oid};

/// What to do with one commit of the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TodoAction {
    /// Keep it as it is.
    Pick,
    /// Keep it with a new message.
    Reword,
    /// Combine it with the commit before it, keeping both messages.
    Squash,
    /// Combine it with the commit before it, keeping the earlier message only.
    Fixup,
    /// Leave it out.
    Drop,
}

impl TodoAction {
    pub const ALL: [TodoAction; 5] = [TodoAction::Pick, TodoAction::Reword, TodoAction::Squash, TodoAction::Fixup, TodoAction::Drop];
    pub fn code(self) -> &'static str {
        match self {
            TodoAction::Pick => "pick",
            TodoAction::Reword => "reword",
            TodoAction::Squash => "squash",
            TodoAction::Fixup => "fixup",
            TodoAction::Drop => "drop",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TodoAction::Pick => "Pick",
            TodoAction::Reword => "Reword",
            TodoAction::Squash => "Squash",
            TodoAction::Fixup => "Fixup",
            TodoAction::Drop => "Drop",
        }
    }
    pub fn parse(s: &str) -> Option<TodoAction> {
        let s = s.trim().to_ascii_lowercase();
        TodoAction::ALL.into_iter().find(|a| a.code() == s || (s.len() == 1 && a.code().starts_with(&s)))
    }
}

/// One line of the todo list, oldest commit first.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub action: TodoAction,
    pub oid: String,
    /// The new message: for `reword`, or for the last commit of a squash group (its combined
    /// message). `None` keeps git's.
    pub message: Option<String>,
}

/// Commits rebased at most in one go.
pub const MAX_PLAN: usize = 5_000;

/// Quote a string for the POSIX shell git runs editors and `exec` lines with.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Repo {
    /// The commits an interactive rebase from `oid` (inclusive) to HEAD would replay, oldest
    /// first, and the base it starts from (`None`: from the root). Refuses merges and commits not
    /// on the checked-out history.
    pub fn rebase_plan(&self, oid: &str) -> Result<(Option<String>, Vec<Commit>)> {
        let oid = check_oid(oid)?;
        let target = self.resolve(oid).ok_or_else(|| GitError::Arg(format!("no commit `{oid}`")))?;
        let head = self.resolve("HEAD").ok_or_else(|| GitError::Arg("there are no commits".into()))?;
        if !self.is_ancestor(&target, &head) {
            return Err(GitError::Arg("only commits on the checked-out branch can be rebased".into()));
        }
        let base = self.resolve(&format!("{target}^1"));
        let range = match &base {
            Some(b) => format!("{b}..{head}"),
            None => head.clone(),
        };
        let merges = self.text(&["rev-list", "--min-parents=2", "--count", range.as_str()])?;
        if merges.trim().parse::<u64>().unwrap_or(1) > 0 {
            return Err(GitError::Arg("the commits to rebase include a merge: rebase from after it".into()));
        }
        let count = self.text(&["rev-list", "--count", range.as_str()])?;
        if count.trim().parse::<usize>().unwrap_or(usize::MAX) > MAX_PLAN {
            return Err(GitError::Arg(format!("more than {MAX_PLAN} commits to rebase")));
        }
        let mut commits = self.log(&crate::LogQuery { revs: vec![range], limit: MAX_PLAN, ..crate::LogQuery::default() })?;
        commits.reverse();
        Ok((base, commits))
    }

    /// Rebase the commits from `oid` to HEAD as `items` say (any order; every planned commit
    /// once, or left out to drop it). Uncommitted changes are stashed and put back
    /// (`--autostash`).
    pub fn rebase_interactive(&self, oid: &str, items: &[TodoItem]) -> Result<Outcome> {
        if self.state() != crate::RepoState::Clean {
            return Err(GitError::Arg("finish or abort the operation in progress first".into()));
        }
        let (base, plan) = self.rebase_plan(oid)?;
        if items.is_empty() {
            return Err(GitError::Arg("the todo list is empty".into()));
        }
        let mut seen = std::collections::HashSet::new();
        let mut full: Vec<(TodoAction, String, Option<String>)> = Vec::with_capacity(items.len());
        for it in items {
            let o = check_oid(&it.oid)?;
            let c = plan.iter().find(|c| c.oid.starts_with(o)).ok_or_else(|| GitError::Arg(format!("`{o}` is not one of the commits to rebase")))?;
            if !seen.insert(c.oid.clone()) {
                return Err(GitError::Arg(format!("`{o}` is listed twice")));
            }
            let msg = it.message.as_deref().map(str::trim_end).filter(|m| !m.trim().is_empty()).map(str::to_string);
            if msg.as_ref().is_some_and(|m| m.len() > crate::MAX_ARG) {
                return Err(GitError::Arg("a commit message is too long".into()));
            }
            if it.action == TodoAction::Reword && msg.is_none() {
                return Err(GitError::Arg(format!("reword {}: write the new message", c.short())));
            }
            full.push((it.action, c.oid.clone(), msg));
        }
        match full.iter().find(|(a, ..)| *a != TodoAction::Drop) {
            Some((TodoAction::Squash | TodoAction::Fixup, ..)) => {
                return Err(GitError::Arg("the first commit kept can't be squashed: there is nothing before it".into()));
            }
            None if base.is_none() => return Err(GitError::Arg("dropping every commit from the root would leave no history".into())),
            _ => {}
        }
        // Commits left out of the list are dropped.
        for c in &plan {
            if !seen.contains(&c.oid) {
                full.push((TodoAction::Drop, c.oid.clone(), None));
            }
        }

        let dir = self.git_dir.join("gitcraft-rebase");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| GitError::Io(format!("{}: {e}", dir.display())))?;
        let git_bin = sh_quote(&self.git.bin.to_string_lossy());
        let mut todo = String::new();
        // The newest message given within the current squash group, applied when it ends.
        let mut pending: Option<String> = None;
        for (i, (action, oid, msg)) in full.iter().enumerate() {
            let verb = match action {
                TodoAction::Pick | TodoAction::Reword => "pick",
                TodoAction::Squash => "squash",
                TodoAction::Fixup => "fixup",
                TodoAction::Drop => "drop",
            };
            todo.push_str(&format!("{verb} {oid}\n"));
            if *action == TodoAction::Drop {
                continue;
            }
            if let Some(m) = msg {
                pending = Some(m.clone());
            }
            let group_continues = full
                .get(i + 1..)
                .and_then(|rest| rest.iter().find(|(a, ..)| *a != TodoAction::Drop))
                .is_some_and(|(a, ..)| matches!(a, TodoAction::Squash | TodoAction::Fixup));
            if group_continues {
                continue;
            }
            if let Some(m) = pending.take() {
                let file = dir.join(format!("msg-{i}.txt"));
                std::fs::write(&file, format!("{m}\n")).map_err(|e| GitError::Io(format!("{}: {e}", file.display())))?;
                todo.push_str(&format!(
                    "exec {git_bin} commit --amend --allow-empty --no-verify --quiet --cleanup=strip -F {}\n",
                    sh_quote(&file.to_string_lossy())
                ));
            }
        }
        let todo_file = dir.join("todo");
        std::fs::write(&todo_file, todo).map_err(|e| GitError::Io(format!("{}: {e}", todo_file.display())))?;

        let mut git = self.git.clone();
        git.env.push(("GIT_SEQUENCE_EDITOR".into(), format!("cp {}", sh_quote(&todo_file.to_string_lossy()))));
        let mut args = vec!["rebase", "--interactive", "--autostash", "--no-autosquash", "--empty=drop"];
        let b;
        match &base {
            Some(x) => {
                b = x.clone();
                args.push(b.as_str());
            }
            None => args.push("--root"),
        }
        let o = git.run_in(Some(&self.root), &args, None)?;
        if o.ok() {
            let _ = std::fs::remove_dir_all(&dir);
            return Ok(Outcome { message: o.message(), conflicts: false });
        }
        if self.status().map(|s| s.conflicted().next().is_some()).unwrap_or(false) {
            return Ok(Outcome { message: o.message(), conflicts: true });
        }
        Err(crate::runner::failed(&args, &o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{commit_all, repo, write};
    use crate::{LogQuery, RepoState};

    fn history(r: &Repo) -> Vec<String> {
        r.log(&LogQuery { revs: vec!["HEAD".into()], ..LogQuery::default() }).unwrap().into_iter().map(|c| c.message()).collect()
    }

    fn four(tag: &str) -> (crate::testing::TempDir, Repo, Vec<String>) {
        let (d, r) = repo(tag).unwrap();
        let mut ids = Vec::new();
        for (i, name) in ["one", "two", "three", "four"].iter().enumerate() {
            write(&r, &format!("{name}.txt"), &format!("{i}\n")).unwrap();
            ids.push(commit_all(&r, name).unwrap());
        }
        (d, r, ids)
    }

    fn item(action: TodoAction, oid: &str, message: Option<&str>) -> TodoItem {
        TodoItem { action, oid: oid.into(), message: message.map(str::to_string) }
    }

    #[test]
    fn reorder_reword_squash_fixup_drop() {
        let (_d, r, ids) = four("rebase-all");
        let (base, plan) = r.rebase_plan(&ids[1]).unwrap();
        assert_eq!(base.as_deref(), Some(ids[0].as_str()));
        assert_eq!(plan.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["two", "three", "four"]);
        // An uncommitted change survives (autostash).
        write(&r, "one.txt", "dirty\n").unwrap();
        let o = r
            .rebase_interactive(
                &ids[1],
                &[
                    item(TodoAction::Reword, &ids[3], Some("FOUR\n\nbody")),
                    item(TodoAction::Pick, &ids[1], None),
                    item(TodoAction::Fixup, &ids[2], None),
                ],
            )
            .unwrap();
        assert!(!o.conflicts);
        assert_eq!(history(&r), ["two", "FOUR\n\nbody", "one"]);
        assert!(r.root().join("three.txt").exists());
        assert_eq!(std::fs::read_to_string(r.root().join("one.txt")).unwrap(), "dirty\n");
        assert_eq!(r.state(), RepoState::Clean);
        assert!(!r.git_dir().join("gitcraft-rebase").exists());
    }

    #[test]
    fn squash_keeps_both_messages_or_a_new_one_and_drop_from_root() {
        let (_d, r, ids) = four("rebase-squash");
        r.rebase_interactive(&ids[2], &[item(TodoAction::Pick, &ids[2], None), item(TodoAction::Squash, &ids[3], None)]).unwrap();
        let h = history(&r);
        assert_eq!(h.len(), 3);
        assert!(h[0].contains("three") && h[0].contains("four") && !h[0].contains('#'), "{h:?}");
        let head = r.resolve("HEAD").unwrap();
        r.rebase_interactive(&ids[1], &[item(TodoAction::Pick, &ids[1], None), item(TodoAction::Squash, &head, Some("Together"))]).unwrap();
        assert_eq!(history(&r), ["Together", "one"]);
        // From the root: drop the first commit.
        let root = r.resolve("HEAD~1").unwrap();
        let top = r.resolve("HEAD").unwrap();
        r.rebase_interactive(&root, &[item(TodoAction::Drop, &root, None), item(TodoAction::Pick, &top, None)]).unwrap();
        assert_eq!(history(&r), ["Together"]);
        assert!(!r.root().join("one.txt").exists());
    }

    #[test]
    fn bad_todo_lists_are_refused() {
        let (_d, r, ids) = four("rebase-bad");
        let before = r.resolve("HEAD");
        for items in [
            vec![],
            vec![item(TodoAction::Squash, &ids[2], None), item(TodoAction::Pick, &ids[3], None)],
            vec![item(TodoAction::Pick, &ids[2], None), item(TodoAction::Pick, &ids[2], None)],
            vec![item(TodoAction::Pick, &ids[0], None)],
            vec![item(TodoAction::Reword, &ids[2], None)],
            vec![item(TodoAction::Pick, "-x", None)],
        ] {
            assert!(r.rebase_interactive(&ids[2], &items).is_err(), "{items:?}");
        }
        assert!(r.rebase_plan("zzzz").is_err());
        assert_eq!(r.resolve("HEAD"), before);
        assert_eq!(TodoAction::parse("f"), Some(TodoAction::Fixup));
        assert_eq!(TodoAction::parse("REWORD"), Some(TodoAction::Reword));
        assert_eq!(TodoAction::parse("x"), None);
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn conflicts_stop_and_abort() {
        let (_d, r) = repo("rebase-conflict").unwrap();
        write(&r, "f.txt", "a\n").unwrap();
        let a = commit_all(&r, "a").unwrap();
        write(&r, "f.txt", "b\n").unwrap();
        let b = commit_all(&r, "b").unwrap();
        write(&r, "f.txt", "c\n").unwrap();
        let c = commit_all(&r, "c").unwrap();
        // Swapping b and c conflicts.
        let o = r.rebase_interactive(&b, &[item(TodoAction::Pick, &c, None), item(TodoAction::Pick, &b, None)]).unwrap();
        assert!(o.conflicts);
        assert_eq!(r.state(), RepoState::Rebasing);
        r.abort_operation().unwrap();
        assert_eq!(r.resolve("HEAD").as_deref(), Some(c.as_str()));
        let _ = a;
    }
}
