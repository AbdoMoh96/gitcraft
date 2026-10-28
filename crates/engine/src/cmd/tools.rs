//! Repository tools: ignoring files, editing a file, patches, Git LFS, hooks, external diff and
//! merge tools, and Git Flow (feature, release and hotfix branches, with the git-flow config keys
//! so repositories set up by other tools work as they are).

use gitcraft_git::{ConfigScope, Repo, RepoState};
use serde_json::{Value, json};

use super::{done, need_commits, need_repo, outcome};
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// Largest file the in-app editor saves.
const MAX_EDIT: usize = 16 * 1024 * 1024;

/// A path inside the work tree (no `..`, not absolute).
fn inside(path: &str) -> Result<&str, CmdError> {
    let path = gitcraft_git::check_path(path)?;
    if path.split(['/', '\\']).any(|c| c == "..") || std::path::Path::new(path).is_absolute() || path.starts_with(['/', '\\']) {
        return Err(CmdError::Params("the path must be inside the repository".into()));
    }
    Ok(path)
}

fn ignore(s: &mut Session, v: &Value) -> CmdResult {
    let local = p::bool(v, "local").unwrap_or(false);
    let patterns: Vec<String> = match p::str(v, "pattern").filter(|x| !x.trim().is_empty()) {
        Some(pat) => vec![pat.trim().to_string()],
        None => {
            let paths = p::paths(v)?;
            let ext = p::bool(v, "extension").unwrap_or(false);
            paths
                .iter()
                .map(|x| {
                    let x = inside(x)?;
                    if ext {
                        let e = x.rsplit_once('.').map(|(_, e)| e).filter(|e| !e.contains('/') && !e.is_empty());
                        e.map(|e| format!("*.{e}")).ok_or_else(|| CmdError::Params(format!("`{x}` has no extension")))
                    } else {
                        Ok(gitcraft_git::tools::ignore_pattern_for(x))
                    }
                })
                .collect::<Result<_, _>>()?
        }
    };
    let repo = s.repo()?.clone();
    let mut file = None;
    for pat in &patterns {
        file = Some(repo.ignore(pat, local)?);
    }
    let where_ = if local { "this clone's exclude list" } else { ".gitignore" };
    Ok(
        json!({"message": format!("Ignored {} in {where_}", patterns.join(", ")), "patterns": patterns, "file": file.map(|f| f.display().to_string())}),
    )
}

fn edit(s: &mut Session, v: &Value) -> CmdResult {
    let path = inside(p::req_str(v, "path")?)?.to_string();
    let content = p::str(v, "content").ok_or_else(|| CmdError::Params("`content` (string) is required".into()))?;
    if content.len() > MAX_EDIT {
        return Err(CmdError::Params("the content is too large".into()));
    }
    let full = s.repo()?.root().join(&path);
    if full.is_dir() {
        return Err(CmdError::Params(format!("`{path}` is a folder")));
    }
    if let Some(dir) = full.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CmdError::Failed(format!("{}: {e}", dir.display())))?;
    }
    std::fs::write(&full, content).map_err(|e| CmdError::Failed(format!("{path}: {e}")))?;
    done(format!("Saved {path}"))
}

fn patch_create(s: &mut Session, v: &Value) -> CmdResult {
    let rev = p::str(v, "oid").or_else(|| p::str(v, "range")).map(str::trim).filter(|r| !r.is_empty());
    let patch = s.repo()?.make_patch(rev, p::bool(v, "staged").unwrap_or(false))?;
    match p::str(v, "path").map(str::trim).filter(|x| !x.is_empty()) {
        Some(out) => {
            let pb = std::path::PathBuf::from(out);
            let pb = if pb.is_absolute() { pb } else { s.repo()?.root().join(pb) };
            if pb.exists() {
                return Err(CmdError::Params(format!("{} exists already", pb.display())));
            }
            std::fs::write(&pb, &patch).map_err(|e| CmdError::Failed(format!("{}: {e}", pb.display())))?;
            Ok(json!({"message": format!("Wrote the patch to {}", pb.display()), "path": pb.display().to_string(), "size": patch.len()}))
        }
        None => Ok(json!({"patch": patch})),
    }
}

fn patch_apply(s: &mut Session, v: &Value) -> CmdResult {
    let text = match (p::str(v, "text"), p::str(v, "path")) {
        (Some(t), _) if !t.trim().is_empty() => t.to_string(),
        (_, Some(path)) if !path.trim().is_empty() => {
            let pb = std::path::PathBuf::from(path.trim());
            let meta = std::fs::metadata(&pb).map_err(|e| CmdError::Failed(format!("{}: {e}", pb.display())))?;
            if meta.len() > gitcraft_git::tools::MAX_PATCH as u64 {
                return Err(CmdError::Params("the patch is too large".into()));
            }
            String::from_utf8_lossy(&std::fs::read(&pb).map_err(|e| CmdError::Failed(format!("{}: {e}", pb.display())))?).into_owned()
        }
        _ => return Err(CmdError::Params("`path` or `text` is required".into())),
    };
    let o = s.repo()?.apply_patch(&text, p::bool(v, "commit").unwrap_or(true))?;
    outcome("Applied the patch", &o)
}

fn hooks_list(s: &mut Session, _: &Value) -> CmdResult {
    let repo = s.repo()?;
    Ok(json!({"dir": repo.hooks_dir().display().to_string(), "hooks": repo.hooks(), "known": gitcraft_git::tools::HOOK_NAMES}))
}

// ---- Git Flow ----

struct Flow {
    main: String,
    develop: String,
    prefix: [(String, String); 3],
    tag: String,
}

impl Flow {
    fn load(repo: &Repo) -> Option<Flow> {
        let develop = repo.config("gitflow.branch.develop")?;
        let main = repo.config("gitflow.branch.master").or_else(|| repo.config("gitflow.branch.main")).unwrap_or_else(|| "main".into());
        let get = |k: &str, d: &str| repo.config(&format!("gitflow.prefix.{k}")).unwrap_or_else(|| d.into());
        Some(Flow {
            main,
            develop,
            prefix: [
                ("feature".into(), get("feature", "feature/")),
                ("release".into(), get("release", "release/")),
                ("hotfix".into(), get("hotfix", "hotfix/")),
            ],
            tag: get("versiontag", ""),
        })
    }
    fn prefix(&self, kind: &str) -> Result<&str, CmdError> {
        self.prefix
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, p)| p.as_str())
            .ok_or_else(|| CmdError::Params("`kind` is feature, release or hotfix".into()))
    }
}

fn need_flow(s: &Session) -> Option<&'static str> {
    need_commits(s).or_else(|| s.data.setting("gitflow.branch.develop").is_none().then_some("Git Flow is not set up: run gitflow.init"))
}

fn flow_init(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    let main = match p::str(v, "main").map(str::trim).filter(|x| !x.is_empty()) {
        Some(m) => m.to_string(),
        None => ["main", "master"]
            .iter()
            .find(|b| repo.resolve(&format!("refs/heads/{b}")).is_some())
            .map(|b| b.to_string())
            .or_else(|| s.data.status.branch.clone())
            .ok_or_else(|| CmdError::Params("`main` is required".into()))?,
    };
    let develop = p::str(v, "develop").map(str::trim).filter(|x| !x.is_empty()).unwrap_or("develop").to_string();
    repo.check_branch_name(&main)?;
    repo.check_branch_name(&develop)?;
    if main == develop {
        return Err(CmdError::Params("the main and develop branches must differ".into()));
    }
    if repo.resolve(&format!("refs/heads/{main}")).is_none() {
        return Err(CmdError::Params(format!("there is no branch `{main}`")));
    }
    if repo.resolve(&format!("refs/heads/{develop}")).is_none() {
        repo.create_branch(&develop, Some(&main), false)?;
    }
    let l = ConfigScope::Local;
    repo.set_config(l, "gitflow.branch.master", Some(&main))?;
    repo.set_config(l, "gitflow.branch.develop", Some(&develop))?;
    for (k, param, d) in [
        ("feature", "featurePrefix", "feature/"),
        ("release", "releasePrefix", "release/"),
        ("hotfix", "hotfixPrefix", "hotfix/"),
        ("versiontag", "tagPrefix", ""),
    ] {
        let val = p::str(v, param).map(str::trim).unwrap_or(d);
        repo.set_config(l, &format!("gitflow.prefix.{k}"), Some(val).filter(|x| !x.is_empty()).or(if k == "versiontag" { None } else { Some(d) }))?;
    }
    done(format!("Git Flow is set up: {main} for releases, {develop} for development"))
}

fn flow_start(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    let f = Flow::load(&repo).ok_or_else(|| CmdError::Disabled("Git Flow is not set up".into()))?;
    let kind = p::req_str(v, "kind")?.trim().to_string();
    let name = p::req_str(v, "name")?.trim().to_string();
    let branch = format!("{}{name}", f.prefix(&kind)?);
    let base = match p::str(v, "from").map(str::trim).filter(|x| !x.is_empty()) {
        Some(b) => b.to_string(),
        None if kind == "hotfix" => f.main.clone(),
        None => f.develop.clone(),
    };
    repo.create_branch(&branch, Some(&base), true)?;
    Ok(json!({"message": format!("Started {branch} from {base}"), "branch": branch}))
}

fn flow_finish(s: &mut Session, v: &Value) -> CmdResult {
    let repo = s.repo()?.clone();
    let f = Flow::load(&repo).ok_or_else(|| CmdError::Disabled("Git Flow is not set up".into()))?;
    let kind = p::req_str(v, "kind")?.trim().to_string();
    let prefix = f.prefix(&kind)?.to_string();
    let name = match p::str(v, "name").map(str::trim).filter(|x| !x.is_empty()) {
        Some(n) => n.strip_prefix(prefix.as_str()).unwrap_or(n).to_string(),
        None => s
            .data
            .status
            .branch
            .as_deref()
            .and_then(|b| b.strip_prefix(prefix.as_str()))
            .map(str::to_string)
            .ok_or_else(|| CmdError::Params(format!("`name` is required (or check out a {kind} branch)")))?,
    };
    let branch = format!("{prefix}{name}");
    if repo.resolve(&format!("refs/heads/{branch}")).is_none() {
        return Err(CmdError::Params(format!("there is no branch `{branch}`")));
    }
    let st = repo.status()?;
    if st.state != RepoState::Clean || !st.is_clean() {
        return Err(CmdError::Params("commit or stash your changes first".into()));
    }
    let keep = p::bool(v, "keepBranch").unwrap_or(false);
    let mut steps: Vec<String> = Vec::new();
    // Merge `rev` into `into` with a merge commit; stop on conflicts.
    let merge_into = |rev: &str, into: &str, steps: &mut Vec<String>| -> Result<Option<CmdResult>, CmdError> {
        repo.checkout_branch(into)?;
        let o = repo.merge(rev, false, true)?;
        steps.push(format!("merged into {into}"));
        if o.conflicts {
            return Ok(Some(outcome(&format!("Finishing {branch}: the merge into {into}"), &o)));
        }
        Ok(None)
    };
    if kind == "feature" {
        if let Some(stop) = merge_into(&branch, &f.develop, &mut steps)? {
            return stop;
        }
    } else {
        if let Some(stop) = merge_into(&branch, &f.main, &mut steps)? {
            return stop;
        }
        // The tag (or main's merge) goes back into develop, so develop has the release too.
        let mut back = f.main.clone();
        if p::bool(v, "tag").unwrap_or(true) {
            let tag = format!("{}{name}", f.tag);
            let msg = p::str(v, "tagMessage")
                .map(str::to_string)
                .unwrap_or_else(|| format!("{} {name}", if kind == "release" { "Release" } else { "Hotfix" }));
            repo.create_tag(&tag, None, Some(&msg))?;
            steps.push(format!("tagged {tag}"));
            back = tag;
        }
        if let Some(stop) = merge_into(&back, &f.develop, &mut steps)? {
            return stop;
        }
    }
    if !keep {
        repo.delete_branch(&branch, false)?;
        steps.push("deleted the branch".into());
    }
    Ok(json!({"message": format!("Finished {branch}: {}", steps.join(", ")), "steps": steps}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("file.ignore", "Ignore File", "Commit Panel › Unstaged", ignore)
            .params(r#"{"path"?: string, "paths"?: [string], "pattern"?: string, "extension"?: bool (ignore *.ext), "local"?: bool (.git/info/exclude)}"#)
            .when(need_repo),
        CommandSpec::new("file.edit", "Edit File", "Diff View", edit).params(r#"{"path": string, "content": string}"#).when(need_repo),
        CommandSpec::new("patch.create", "Create Patch", "Graph › Commit Menu", patch_create)
            .params(r#"{"oid"?: commit, "range"?: "a..b", "staged"?: bool (with neither: the uncommitted changes), "path"?: output file (else returned)}"#)
            .pure()
            .when(need_commits),
        CommandSpec::new("patch.apply", "Apply Patch", "Repository", patch_apply)
            .params(r#"{"path"?: patch file, "text"?: patch, "commit"?: bool (default true: mail patches become commits)}"#)
            .when(need_repo),
        CommandSpec::new("lfs.list", "Git LFS", "Repository", |s, _| {
            let repo = s.repo()?;
            let available = repo.lfs_available();
            let files = if available { repo.lfs_files().unwrap_or_default() } else { Vec::new() };
            Ok(json!({"available": available, "patterns": repo.lfs_patterns(), "files": files}))
        })
        .pure()
        .when(need_repo),
        CommandSpec::new("lfs.track", "Track with LFS", "Repository", |s, v| {
            let pat = p::req_str(v, "pattern")?.trim().to_string();
            s.repo()?.lfs_track(&pat)?;
            done(format!("Files matching {pat} are stored with LFS (.gitattributes staged)"))
        })
        .params(r#"{"pattern": string (*.psd)}"#)
        .when(need_repo),
        CommandSpec::new("lfs.untrack", "Stop Tracking with LFS", "Repository", |s, v| {
            let pat = p::req_str(v, "pattern")?.trim().to_string();
            s.repo()?.lfs_untrack(&pat)?;
            done(format!("Stopped storing {pat} with LFS"))
        })
        .params(r#"{"pattern": string}"#)
        .when(need_repo),
        CommandSpec::new("lfs.pull", "Pull LFS Files", "Repository", |s, _| {
            let m = s.repo()?.lfs_pull()?;
            Ok(json!({"message": "Downloaded the LFS files", "git": m}))
        })
        .slow()
        .when(need_repo),
        CommandSpec::new("hooks.list", "Git Hooks", "Repository", hooks_list).pure().when(need_repo),
        CommandSpec::new("hooks.enable", "Turn a Hook On or Off", "Repository", |s, v| {
            let name = p::req_str(v, "name")?.trim().to_string();
            let on = p::bool(v, "value").unwrap_or(true);
            s.repo()?.set_hook_enabled(&name, on)?;
            done(format!("Turned the {name} hook {}", if on { "on" } else { "off" }))
        })
        .params(r#"{"name": hook, "value"?: bool (default true)}"#)
        .when(need_repo),
        CommandSpec::new("hooks.read", "Read Hook", "Repository", |s, v| {
            let name = p::req_str(v, "name")?.trim().to_string();
            Ok(json!({"name": name, "script": s.repo()?.read_hook(&name)?}))
        })
        .params(r#"{"name": hook}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("hooks.write", "Write Hook", "Repository", |s, v| {
            let name = p::req_str(v, "name")?.trim().to_string();
            let script = p::str(v, "script").ok_or_else(|| CmdError::Params("`script` is required".into()))?;
            s.repo()?.write_hook(&name, script)?;
            done(format!("Saved the {name} hook"))
        })
        .params(r#"{"name": hook, "script": string}"#)
        .when(need_repo),
        CommandSpec::new("tool.diff", "Open in External Diff Tool", "Diff View", |s, v| {
            let path = inside(p::req_str(v, "path")?)?.to_string();
            s.repo()?.launch_tool(false, &path)?;
            done(format!("Opened {path} in the diff tool"))
        })
        .params(r#"{"path": string}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("tool.merge", "Open in External Merge Tool", "Merge Tool", |s, v| {
            let path = inside(p::req_str(v, "path")?)?.to_string();
            s.repo()?.launch_tool(true, &path)?;
            done(format!("Opened {path} in the merge tool"))
        })
        .params(r#"{"path": string}"#)
        .pure()
        .when(need_repo),
        CommandSpec::new("gitflow.init", "Git Flow", "Repository", flow_init)
            .params(r#"{"main"?: branch, "develop"?: branch ("develop"), "featurePrefix"?, "releasePrefix"?, "hotfixPrefix"?, "tagPrefix"?: string}"#)
            .when(need_commits),
        CommandSpec::new("gitflow.start", "Start Feature, Release or Hotfix", "Repository", flow_start)
            .params(r#"{"kind": "feature" | "release" | "hotfix", "name": string, "from"?: commit}"#)
            .when(need_flow),
        CommandSpec::new("gitflow.finish", "Finish Feature, Release or Hotfix", "Repository", flow_finish)
            .params(r#"{"kind": "feature" | "release" | "hotfix", "name"?: string (default: the checked-out one), "tag"?: bool, "tagMessage"?: string, "keepBranch"?: bool}"#)
            .when(need_flow),
    ]
}
