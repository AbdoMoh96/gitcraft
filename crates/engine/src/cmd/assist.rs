//! Help writing and reading history:
//! - AI text through a command the user chooses (`gitcraft.aiCommand`: any program that reads a
//!   prompt on standard input and prints the answer, such as a local model's CLI). Nothing is sent
//!   anywhere unless that is set.
//! - Insights: who committed what and when, computed from the history.
//! - Team overlap: other people's remote branches that change the files you are changing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use super::{need_commits, need_repo, target_commit};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// Largest diff sent to the AI command, and largest answer kept.
const MAX_INPUT: usize = 200 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024;

fn ai_command(s: &Session) -> Result<String, CmdError> {
    s.repo()?
        .config("gitcraft.aiCommand")
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| CmdError::Disabled("no AI command is set: Preferences › AI (git config gitcraft.aiCommand)".into()))
}

/// Run the AI command with `prompt` on standard input; what it printed.
fn ask(command: &str, prompt: &str) -> Result<String, CmdError> {
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| CmdError::Failed(format!("the AI command: {e}")))?;
    if let Some(mut w) = child.stdin.take() {
        let input = prompt.to_string();
        std::thread::spawn(move || {
            let _ = w.write_all(input.as_bytes());
        });
    }
    let mut out = Vec::new();
    if let Some(o) = child.stdout.take() {
        let _ = o.take(MAX_OUTPUT).read_to_end(&mut out);
    }
    let status = child.wait().map_err(|e| CmdError::Failed(e.to_string()))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(CmdError::Failed(format!("the AI command failed: {}", err.trim())));
    }
    let text = String::from_utf8_lossy(&out).trim().to_string();
    if text.is_empty() {
        return Err(CmdError::Failed("the AI command printed nothing".into()));
    }
    Ok(text)
}

fn clip(s: &str) -> &str {
    match s.char_indices().nth(MAX_INPUT) {
        Some((i, _)) => s.get(..i).unwrap_or(s),
        None => s,
    }
}

/// Strip code fences and quotes a model may wrap its answer in.
fn clean_message(text: &str) -> String {
    let t = text.trim().trim_start_matches("```").trim_end_matches("```").trim();
    let t = t.strip_prefix("text\n").unwrap_or(t);
    t.trim_matches('"').trim().to_string()
}

fn generate_message(s: &mut Session, _: &Value) -> CmdResult {
    let cmd = ai_command(s)?;
    let repo = s.repo()?.clone();
    let diff = repo.text(&["diff", "--cached", "--no-color", "--no-ext-diff", "--stat", "--patch"])?;
    if diff.trim().is_empty() {
        return Err(CmdError::Params("nothing is staged".into()));
    }
    let prompt = format!(
        "Write a git commit message for the staged changes below. First line: a summary of at most 72 characters in the imperative mood. Then, if useful, a blank line and a short description of why. Reply with the message only.\n\n{}",
        clip(&diff)
    );
    let msg = clean_message(&ask(&cmd, &prompt)?);
    let (summary, description) =
        msg.split_once('\n').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or((msg.clone(), String::new()));
    if !s.is_detached() {
        s.draft.summary = summary.clone();
        s.draft.description = description.clone();
    }
    Ok(json!({"message": "Wrote a commit message", "summary": summary, "description": description}))
}

fn explain_commit(s: &mut Session, v: &Value) -> CmdResult {
    let cmd = ai_command(s)?;
    let oid = target_commit(s, v)?;
    let repo = s.repo()?.clone();
    let oid = gitcraft_git::check_name("commit", &oid)?.to_string();
    let show = repo.text(&["show", "--no-color", "--no-ext-diff", "--stat", "--patch", oid.as_str()])?;
    let prompt =
        format!("Explain what this git commit changes and why it might have been made, in a few sentences for a teammate.\n\n{}", clip(&show));
    let text = ask(&cmd, &prompt)?;
    Ok(json!({"oid": oid, "explanation": text}))
}

fn describe_changes(s: &mut Session, v: &Value) -> CmdResult {
    let cmd = ai_command(s)?;
    let repo = s.repo()?.clone();
    let base = p::str(v, "base").map(str::trim).filter(|b| !b.is_empty()).unwrap_or("main").to_string();
    let head = p::str(v, "head").map(str::trim).filter(|b| !b.is_empty()).unwrap_or("HEAD").to_string();
    let range = format!("{}...{}", gitcraft_git::check_name("base", &base)?, gitcraft_git::check_name("head", &head)?);
    let log = repo.text(&["log", "--no-color", "--format=- %s", range.as_str()])?;
    let diff = repo.text(&["diff", "--no-color", "--no-ext-diff", "--stat", range.as_str()])?;
    let prompt = format!(
        "Write a pull request title and description for these changes. First line: the title. Then a blank line and the description in Markdown (what and why, how to test).\n\nCommits:\n{}\n\nFiles:\n{}",
        clip(&log),
        clip(&diff)
    );
    let text = clean_message(&ask(&cmd, &prompt)?);
    let (title, body) = text
        .split_once('\n')
        .map(|(a, b)| (a.trim().trim_start_matches('#').trim().to_string(), b.trim().to_string()))
        .unwrap_or((text.clone(), String::new()));
    Ok(json!({"title": title, "body": body}))
}

fn conflict_help(s: &mut Session, v: &Value) -> CmdResult {
    let cmd = ai_command(s)?;
    let path = p::req_str(v, "path")?.to_string();
    let repo = s.repo()?.clone();
    let text = repo.conflict_text(&path)?.ok_or_else(|| CmdError::Params(format!("`{path}` has no conflict markers")))?;
    let prompt = format!(
        "This file has git merge conflicts. Suggest how to resolve each conflict, then give the whole resolved file in one fenced code block.\n\nFile {path}:\n{}",
        clip(&text)
    );
    Ok(json!({"path": path, "suggestion": ask(&cmd, &prompt)?}))
}

/// Commits per author, per weekday and per week, and the most changed files, over `days`.
fn insights(s: &mut Session, v: &Value) -> CmdResult {
    let days = p::u64(v, "days").unwrap_or(90).clamp(1, 3650);
    let since = format!("--since={days}.days");
    let rev = p::str(v, "ref").map(str::trim).filter(|r| !r.is_empty()).unwrap_or("HEAD");
    let rev = gitcraft_git::check_name("ref", rev)?;
    let repo = s.repo()?.clone();
    let log = repo.text(&["log", rev, since.as_str(), "--no-merges", "--format=\x1e%an\x1f%ae\x1f%at", "--numstat", "--max-count=50000"])?;
    let mut authors: HashMap<String, (String, u64, u64, u64)> = HashMap::new();
    let mut weekdays = [0u64; 7];
    let mut weeks: BTreeMap<i64, u64> = BTreeMap::new();
    let mut files: HashMap<String, u64> = HashMap::new();
    let (mut commits, mut added, mut removed) = (0u64, 0u64, 0u64);
    for rec in log.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let mut lines = rec.lines();
        let head = lines.next().unwrap_or("");
        let mut f = head.split('\x1f');
        let (name, email, time) = (
            f.next().unwrap_or("").to_string(),
            f.next().unwrap_or("").to_string(),
            f.next().and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0),
        );
        commits += 1;
        let a = authors.entry(email.to_lowercase()).or_insert((name, 0, 0, 0));
        a.1 += 1;
        // 1970-01-01 was a Thursday.
        let wd = ((time.div_euclid(86_400) + 3).rem_euclid(7)) as usize;
        if let Some(w) = weekdays.get_mut(wd) {
            *w += 1;
        }
        *weeks.entry(time.div_euclid(7 * 86_400)).or_default() += 1;
        for l in lines {
            let mut c = l.split('\t');
            let (ad, rm, path) = (c.next().unwrap_or(""), c.next().unwrap_or(""), c.next().unwrap_or(""));
            if path.is_empty() {
                continue;
            }
            let (ad, rm) = (ad.parse::<u64>().unwrap_or(0), rm.parse::<u64>().unwrap_or(0));
            a.2 += ad;
            a.3 += rm;
            added += ad;
            removed += rm;
            *files.entry(path.to_string()).or_default() += 1;
        }
    }
    let mut people: Vec<Value> = authors
        .into_iter()
        .map(|(email, (name, c, ad, rm))| json!({"name": name, "email": email, "commits": c, "added": ad, "removed": rm}))
        .collect();
    people.sort_by(|a, b| b["commits"].as_u64().cmp(&a["commits"].as_u64()));
    let mut hot: Vec<(String, u64)> = files.into_iter().collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    hot.truncate(20);
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let by_day: Vec<Value> = names.iter().zip(weekdays.iter()).map(|(d, n)| json!({"day": d, "commits": n})).collect();
    Ok(json!({
        "days": days,
        "commits": commits,
        "added": added,
        "removed": removed,
        "authors": people,
        "weekdays": by_day,
        "weeks": weeks.iter().map(|(w, n)| json!({"week": w * 7 * 86_400, "commits": n})).collect::<Vec<_>>(),
        "files": hot.iter().map(|(f, n)| json!({"path": f, "commits": n})).collect::<Vec<_>>(),
    }))
}

/// Remote branches (not merged into HEAD, changed in the last `days`) that touch the files you
/// are changing: uncommitted changes and commits not yet on your upstream.
fn team_overlap(s: &mut Session, v: &Value) -> CmdResult {
    let days = p::u64(v, "days").unwrap_or(30).clamp(1, 365) as i64;
    let repo = s.repo()?.clone();
    let mut mine: HashSet<String> = s.data.status.files.iter().map(|f| f.path.clone()).collect();
    if let Some(up) = &s.data.status.upstream {
        let range = format!("{}..HEAD", gitcraft_git::check_name("upstream", up)?);
        for l in repo.text(&["diff", "--name-only", "--no-renames", range.as_str()]).unwrap_or_default().lines() {
            mine.insert(l.to_string());
        }
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let head = repo.resolve("HEAD").ok_or_else(|| CmdError::Disabled("no commits yet".into()))?;
    let upstream = s.data.status.upstream.clone();
    let mut out = Vec::new();
    for r in s.data.remote_branches().take(200) {
        if Some(&r.name) == upstream.as_ref() || r.name.ends_with("/HEAD") || repo.is_ancestor(&r.commit, &head) {
            continue;
        }
        let Ok(c) = repo.find_commit(&r.commit) else { continue };
        if now - c.committer.time > days * 86_400 {
            continue;
        }
        let range = format!("HEAD...{}", r.commit);
        let theirs: Vec<String> =
            repo.text(&["diff", "--name-only", "--no-renames", range.as_str()]).unwrap_or_default().lines().map(str::to_string).collect();
        let shared: Vec<&String> = theirs.iter().filter(|f| mine.contains(*f)).collect();
        if !shared.is_empty() || p::bool(v, "all").unwrap_or(false) {
            out.push(json!({"branch": r.name, "author": c.author.name, "email": c.author.email, "time": c.committer.time, "files": shared, "changed": theirs.len()}));
        }
    }
    let n = out.iter().filter(|x| x["files"].as_array().is_some_and(|a| !a.is_empty())).count();
    Ok(
        json!({"message": format!("{n} remote branch{} change files you're changing", if n == 1 { "" } else { "es" }), "mine": mine.len(), "branches": out}),
    )
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("commit.generateMessage", "Generate Commit Message", "Commit Panel", generate_message)
            .params("{} (needs gitcraft.aiCommand)")
            .pure()
            .slow()
            .when(need_repo),
        CommandSpec::new("commit.explain", "Explain Commit", "Graph › Commit Menu", explain_commit)
            .params(r#"{"oid"?: commit (default: selected)} (needs gitcraft.aiCommand)"#)
            .pure()
            .slow()
            .when(need_commits),
        CommandSpec::new("pr.describe", "Write Pull Request Description", "Create Pull Request", describe_changes)
            .params(r#"{"base"?: branch (main), "head"?: branch (HEAD)} (needs gitcraft.aiCommand)"#)
            .pure()
            .slow()
            .when(need_commits),
        CommandSpec::new("conflict.suggest", "Suggest a Conflict Resolution", "Merge Tool", conflict_help)
            .params(r#"{"path": conflicted file} (needs gitcraft.aiCommand)"#)
            .pure()
            .slow()
            .when(need_repo),
        CommandSpec::new("repo.insights", "Insights", "Repository", insights)
            .params(r#"{"days"?: number (90), "ref"?: string (HEAD)}"#)
            .pure()
            .when(need_commits),
        CommandSpec::new("team.overlap", "Team: Changes Like Yours", "Toolbar", team_overlap)
            .params(r#"{"days"?: number (30), "all"?: bool (every recent branch)}"#)
            .pure()
            .when(need_commits),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_answers() {
        assert_eq!(clean_message("```\nFix the parser\n```"), "Fix the parser");
        assert_eq!(clean_message("\"Add tests\""), "Add tests");
        assert_eq!(clip("abc"), "abc");
    }
}
