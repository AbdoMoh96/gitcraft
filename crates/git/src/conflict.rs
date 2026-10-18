//! A conflicted file as text with conflict markers, split into the parts both sides agree on and
//! the conflicts (current side, optional base, incoming side), for the merge tool.

use serde::{Deserialize, Serialize};

use crate::{GitError, Repo, Result, check_path};

/// The longest file the merge tool reads or writes.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

/// A run of a conflicted file. Lines keep their line endings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Chunk {
    /// Lines both sides agree on.
    Common { lines: Vec<String> },
    /// A conflict: the current side's lines (ours), the merge base's when git wrote them (the
    /// `diff3` style), and the incoming side's (theirs).
    Conflict { ours: Vec<String>, base: Option<Vec<String>>, theirs: Vec<String> },
}

/// Which marker a line is, for the default marker size (7).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    Start,
    Base,
    Middle,
    End,
}

fn marker(line: &str) -> Option<Marker> {
    let body = line.trim_end_matches(['\n', '\r']);
    let is = |c: char| {
        let run = body.chars().take_while(|&x| x == c).count();
        // Exactly seven, then the end of the line or a space and a label.
        run == 7 && body.chars().nth(7).is_none_or(|x| x == ' ')
    };
    if is('<') {
        Some(Marker::Start)
    } else if is('|') {
        Some(Marker::Base)
    } else if body == "=======" {
        Some(Marker::Middle)
    } else if is('>') {
        Some(Marker::End)
    } else {
        None
    }
}

/// Split text with conflict markers into chunks. Markers that don't form a whole conflict are kept
/// as ordinary lines.
pub fn parse(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out: Vec<Chunk> = Vec::new();
    let mut common: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(&line) = lines.get(i) {
        if marker(line) == Some(Marker::Start)
            && let Some((chunk, next)) = conflict_at(&lines, i)
        {
            if !common.is_empty() {
                out.push(Chunk::Common { lines: std::mem::take(&mut common) });
            }
            out.push(chunk);
            i = next;
            continue;
        }
        common.push(line.to_string());
        i += 1;
    }
    if !common.is_empty() {
        out.push(Chunk::Common { lines: common });
    }
    out
}

/// The conflict starting at `start` (a start marker) and the index after its end marker.
fn conflict_at(lines: &[&str], start: usize) -> Option<(Chunk, usize)> {
    let (mut ours, mut base, mut theirs) = (Vec::new(), None::<Vec<String>>, Vec::new());
    // 0: ours, 1: base, 2: theirs.
    let mut part = 0;
    let mut i = start.checked_add(1)?;
    while let Some(&line) = lines.get(i) {
        match (marker(line), part) {
            (Some(Marker::Base), 0) => {
                base = Some(Vec::new());
                part = 1;
            }
            (Some(Marker::Middle), 0 | 1) => part = 2,
            (Some(Marker::End), 2) => return Some((Chunk::Conflict { ours, base, theirs }, i.checked_add(1)?)),
            // A nested or out-of-order marker: not a conflict we understand.
            (Some(Marker::Start), _) => return None,
            _ => match part {
                0 => ours.push(line.to_string()),
                1 => base.get_or_insert_with(Vec::new).push(line.to_string()),
                _ => theirs.push(line.to_string()),
            },
        }
        i += 1;
    }
    None
}

/// `git merge-tree --write-tree --name-only -z` output: the tree id, then each conflicted file.
pub fn parse_merge_tree(out: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    // An empty field ends the file list (informational messages may follow).
    for f in out.split('\0').skip(1).take_while(|f| !f.is_empty()) {
        if !files.iter().any(|x| x == f) {
            files.push(f.to_string());
        }
    }
    files
}

/// Whether text has at least one whole conflict.
pub fn has_conflicts(chunks: &[Chunk]) -> bool {
    chunks.iter().any(|c| matches!(c, Chunk::Conflict { .. }))
}

impl Repo {
    /// The blob ids of a conflicted file's stages (1 base, 2 ours, 3 theirs); `None` for a stage
    /// that isn't there (a file added on both sides has no base, one deleted on a side no stage).
    pub fn conflict_stages(&self, path: &str) -> Result<[Option<String>; 3]> {
        let path = check_path(path)?;
        let o = self.run_paths(&["ls-files", "-u", "-z", "--", path])?;
        let mut stages: [Option<String>; 3] = [None, None, None];
        for rec in o.text().split('\0') {
            // `<mode> <oid> <stage>\t<path>`
            let Some((meta, _)) = rec.split_once('\t') else { continue };
            let mut f = meta.split(' ');
            let (Some(_mode), Some(oid), Some(stage)) = (f.next(), f.next(), f.next()) else { continue };
            let slot = match stage {
                "1" => stages.get_mut(0),
                "2" => stages.get_mut(1),
                "3" => stages.get_mut(2),
                _ => None,
            };
            if let Some(s) = slot
                && crate::check_oid(oid).is_ok()
            {
                *s = Some(oid.to_string());
            }
        }
        Ok(stages)
    }

    /// A conflicted file merged again from its stages with conflict markers (`git merge-file`),
    /// so the merge tool works even after the work tree copy was edited. `None` when a side is
    /// missing or git can't merge blobs by id (before 2.43).
    pub fn conflict_text(&self, path: &str) -> Result<Option<String>> {
        let [base, ours, theirs] = self.conflict_stages(path)?;
        let (Some(ours), Some(theirs)) = (ours, theirs) else { return Ok(None) };
        let base = match base {
            Some(b) => b,
            // Added on both sides: merge against an empty file.
            None => self.run_input(&["hash-object", "-w", "--stdin"], b"")?.text().trim().to_string(),
        };
        crate::check_oid(&base)?;
        let o = self.run(&["merge-file", "-p", "--diff3", "--object-id", "-L", "Current", "-L", "Base", "-L", "Incoming", &ours, &base, &theirs])?;
        // merge-file exits with the number of conflicts; a negative status is an error.
        if o.code.is_none_or(|c| c < 0) || (o.stdout.is_empty() && !o.stderr.trim().is_empty()) {
            return Ok(None);
        }
        if o.stdout.len() > MAX_BYTES {
            return Err(GitError::Arg("the file is too large for the merge tool".into()));
        }
        Ok(Some(o.text()))
    }

    /// The files that would conflict if `theirs` were merged into `ours` (both commits or branch
    /// names), worked out in memory by `git merge-tree` without touching the index or work tree.
    /// `None` when this git can't (before 2.38) or the two have no common history.
    pub fn merge_conflicts(&self, ours: &str, theirs: &str) -> Result<Option<Vec<String>>> {
        let ours = crate::check_name("ours", ours)?;
        let theirs = crate::check_name("theirs", theirs)?;
        let o = self.run(&["merge-tree", "--write-tree", "--name-only", "--no-messages", "-z", ours, theirs])?;
        match o.code {
            Some(0) => Ok(Some(Vec::new())),
            Some(1) => Ok(Some(parse_merge_tree(&o.text()))),
            _ => Ok(None),
        }
    }

    /// The branch `origin/HEAD` points at (`origin/main`), the usual merge target.
    pub fn remote_head(&self, remote: &str) -> Option<String> {
        let remote = crate::check_name("remote", remote).ok()?;
        let r = format!("refs/remotes/{remote}/HEAD");
        let o = self.run(&["symbolic-ref", "-q", "--short", r.as_str()]).ok()?;
        let name = o.text().trim().to_string();
        (o.ok() && !name.is_empty()).then_some(name)
    }

    /// Resolve a conflicted file with `content` (the merge tool's output) and stage it.
    pub fn resolve_content(&self, path: &str, content: &[u8]) -> Result<()> {
        let path = check_path(path)?;
        if path.split(['/', '\\']).any(|c| c == ".." || c == ".git") || std::path::Path::new(path).is_absolute() {
            return Err(GitError::Arg(format!("`{path}` is not a path inside the repository")));
        }
        if content.len() > MAX_BYTES {
            return Err(GitError::Arg("the content is too large".into()));
        }
        let file = self.root().join(path);
        // Never write through a link: it could point outside the repository.
        if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(GitError::Arg(format!("`{path}` is a symbolic link")));
        }
        if let Some(dir) = file.parent()
            && !dir.is_dir()
        {
            return Err(GitError::Arg(format!("the folder of `{path}` does not exist")));
        }
        std::fs::write(&file, content).map_err(|e| GitError::Io(format!("writing {path}: {e}")))?;
        self.run_paths(&["add", "--", path]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{commit_all, repo, write};

    #[test]
    fn parses_merge_and_diff3_styles() {
        let text = "a\n<<<<<<< HEAD\nours\n=======\ntheirs 1\ntheirs 2\n>>>>>>> other\nb\n";
        let c = parse(text);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], Chunk::Common { lines: vec!["a\n".into()] });
        assert_eq!(c[1], Chunk::Conflict { ours: vec!["ours\n".into()], base: None, theirs: vec!["theirs 1\n".into(), "theirs 2\n".into()] });
        let d3 = "<<<<<<< Current\nx\n||||||| Base\nb\n=======\ny\n>>>>>>> Incoming\n";
        assert_eq!(parse(d3), vec![Chunk::Conflict { ours: vec!["x\n".into()], base: Some(vec!["b\n".into()]), theirs: vec!["y\n".into()] }]);
        // CRLF and an empty side.
        let crlf = "<<<<<<< a\r\n=======\r\nz\r\n>>>>>>> b\r\n";
        assert_eq!(parse(crlf), vec![Chunk::Conflict { ours: vec![], base: None, theirs: vec!["z\r\n".into()] }]);
    }

    #[test]
    fn broken_markers_are_text() {
        let text = "<<<<<<< x\nours\nno end\n";
        assert_eq!(parse(text), vec![Chunk::Common { lines: vec!["<<<<<<< x\n".into(), "ours\n".into(), "no end\n".into()] }]);
        assert!(!has_conflicts(&parse("======= \n<<<<<<<<\n")));
    }

    #[test]
    fn junk_never_panics() {
        for s in ["", "\0", "\0\0", "abc\0f\0\0msg"] {
            let _ = parse_merge_tree(s);
        }
        for s in ["", "\n", "<<<<<<<", "=======\n>>>>>>>\n", "<<<<<<< \n||||||| \n||||||| \n=======\n=======\n>>>>>>>", "é<<<<<<<é\n", "\r\r\n\0"] {
            let _ = parse(s);
        }
        let mut s = String::new();
        for i in 0..500u32 {
            s.push_str(["<<<<<<< a\n", "=======\n", ">>>>>>> b\n", "|||||||\n", "x\n"][(i * 7 % 5) as usize]);
            let _ = parse(&s);
        }
    }

    #[test]
    fn merges_from_stages_and_resolves() {
        let (_d, r) = repo("conflict-text").unwrap();
        write(&r, "f.txt", "one\ntwo\nthree\n").unwrap();
        commit_all(&r, "base").unwrap();
        r.create_branch("other", None, true).unwrap();
        write(&r, "f.txt", "one\nTWO theirs\nthree\n").unwrap();
        commit_all(&r, "theirs").unwrap();
        r.checkout_branch("main").unwrap();
        write(&r, "f.txt", "one\nTWO ours\nthree\n").unwrap();
        commit_all(&r, "ours").unwrap();
        // Predicted before merging, without touching the work tree.
        if let Some(files) = r.merge_conflicts("HEAD", "other").unwrap() {
            assert_eq!(files, vec!["f.txt".to_string()]);
            assert_eq!(r.merge_conflicts("HEAD", "HEAD").unwrap(), Some(vec![]));
        }
        assert_eq!(r.status().unwrap().files.len(), 0);
        assert!(r.merge_conflicts("HEAD", "--output=x").is_err());
        assert!(r.merge("other", false, false).unwrap().conflicts);
        let stages = r.conflict_stages("f.txt").unwrap();
        assert!(stages.iter().all(Option::is_some));
        // The work tree copy as git wrote it parses.
        let wt = std::fs::read_to_string(r.root().join("f.txt")).unwrap();
        assert!(has_conflicts(&parse(&wt)));
        // From the stages (git 2.43+), even after the work tree was edited.
        write(&r, "f.txt", "garbage\n").unwrap();
        if let Some(t) = r.conflict_text("f.txt").unwrap() {
            let c = parse(&t);
            assert!(c.contains(&Chunk::Conflict {
                ours: vec!["TWO ours\n".into()],
                base: Some(vec!["two\n".into()]),
                theirs: vec!["TWO theirs\n".into()]
            }));
        }
        assert!(r.resolve_content("../x", b"").is_err());
        assert_eq!(parse_merge_tree("t\0f\0g\0f\0\0info"), vec!["f".to_string(), "g".to_string()]);
        assert!(r.resolve_content(".git/config", b"").is_err());
        r.resolve_content("f.txt", b"one\nTWO ours\nTWO theirs\nthree\n").unwrap();
        assert_eq!(r.status().unwrap().conflicted().count(), 0);
        assert_eq!(std::fs::read_to_string(r.root().join("f.txt")).unwrap(), "one\nTWO ours\nTWO theirs\nthree\n");
    }
}
