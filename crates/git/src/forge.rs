//! Hosting services (pull requests, issues, CI) through their own command-line tools: GitHub's `gh`
//! and GitLab's `glab`. GitCraft never sees a token: the tools keep the user's sign-in, and
//! GitCraft runs them non-interactively and reads their JSON output.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::Value;

use crate::{GitError, Repo, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ForgeKind {
    GitHub,
    GitLab,
}

impl ForgeKind {
    pub fn tool(self) -> &'static str {
        match self {
            ForgeKind::GitHub => "gh",
            ForgeKind::GitLab => "glab",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            ForgeKind::GitHub => "GitHub",
            ForgeKind::GitLab => "GitLab",
        }
    }
    pub fn parse(s: &str) -> Option<ForgeKind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "github" | "gh" => Some(ForgeKind::GitHub),
            "gitlab" | "glab" => Some(ForgeKind::GitLab),
            _ => None,
        }
    }
}

/// Where a repository is hosted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forge {
    pub kind: ForgeKind,
    pub remote: String,
    pub host: String,
    /// `owner/name` (GitLab: `group/subgroup/name`).
    pub path: String,
}

/// The host and project path in a remote URL (`https://host/a/b.git`, `git@host:a/b.git`,
/// `ssh://git@host:22/a/b`).
pub fn parse_remote_url(url: &str) -> Option<(String, String)> {
    let u = url.trim();
    let (host, path) = if let Some(rest) = u.split_once("://").map(|(_, r)| r) {
        let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        (host.split(':').next()?.to_string(), path.to_string())
    } else {
        // scp-like: user@host:path
        let (left, path) = u.split_once(':')?;
        let host = left.rsplit_once('@').map(|(_, h)| h).unwrap_or(left);
        (host.to_string(), path.to_string())
    };
    let path = path.trim_matches('/').trim_end_matches(".git").to_string();
    (!host.is_empty() && path.contains('/')).then_some((host.to_lowercase(), path))
}

/// The service a host belongs to (`github.com`, GitHub Enterprise hosts named `github.*`, and
/// `gitlab.*`).
pub fn kind_of_host(host: &str) -> Option<ForgeKind> {
    if host == "github.com" || host.starts_with("github.") || host.ends_with(".github.com") || host.contains("ghe.") {
        Some(ForgeKind::GitHub)
    } else if host == "gitlab.com" || host.starts_with("gitlab.") || host.contains(".gitlab.") {
        Some(ForgeKind::GitLab)
    } else {
        None
    }
}

/// Run a hosting tool in `dir` with `args`; its standard output. Never prompts.
pub fn run_tool(kind: ForgeKind, dir: Option<&Path>, args: &[&str], input: Option<&str>) -> Result<String> {
    if args.iter().any(|a| a.contains('\0')) {
        return Err(GitError::Arg("bad argument".into()));
    }
    let mut c = Command::new(kind.tool());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    c.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GLAB_NO_PROMPT", "true")
        .env("NO_PROMPT", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PAGER", "cat")
        .env("GH_PAGER", "cat")
        .env("GLAB_PAGER", "cat")
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child =
        c.spawn().map_err(|e| GitError::NotFound(format!("{} ({}): {e}. Install it to use {} here.", kind.tool(), kind.name(), kind.name())))?;
    if let (Some(text), Some(mut w)) = (input, child.stdin.take()) {
        use std::io::Write;
        let _ = w.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| GitError::Io(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err };
        return Err(GitError::Failed {
            cmd: format!("{} {}", kind.tool(), args.first().copied().unwrap_or("")),
            code: out.status.code(),
            stderr: err,
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn json(text: &str) -> Result<Value> {
    serde_json::from_str(text.trim()).map_err(|e| GitError::Parse(format!("unexpected output: {e}")))
}

/// A pull (merge) request, the same shape for both services.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head: String,
    pub base: String,
    pub state: String,
    pub draft: bool,
    pub url: String,
    /// `owner/name` (search results span repositories).
    pub repo: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub labels: Vec<String>,
    pub url: String,
    pub repo: String,
}

/// A CI run (GitHub Actions run, GitLab pipeline).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CiRun {
    pub id: u64,
    pub name: String,
    pub branch: String,
    /// `queued`, `in_progress`, `completed`, `success`, `failed`, …
    pub status: String,
    /// `success`, `failure`, `cancelled`, … (empty while running).
    pub conclusion: String,
    pub url: String,
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn login(v: &Value) -> String {
    let a = v.get("author").or_else(|| v.get("user"));
    a.map(|a| {
        let l = s(a, "login");
        if l.is_empty() { s(a, "username") } else { l }
    })
    .unwrap_or_default()
}

/// Parse `gh pr list --json …` / `gh search prs --json …` / `glab mr list -F json`.
pub fn parse_prs(kind: ForgeKind, text: &str) -> Result<Vec<PullRequest>> {
    let v = json(text)?;
    let arr = v.as_array().cloned().unwrap_or_default();
    Ok(arr
        .iter()
        .take(500)
        .map(|x| match kind {
            ForgeKind::GitHub => PullRequest {
                number: x.get("number").and_then(Value::as_u64).unwrap_or(0),
                title: s(x, "title"),
                author: login(x),
                head: s(x, "headRefName"),
                base: s(x, "baseRefName"),
                state: s(x, "state").to_lowercase(),
                draft: x.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                url: s(x, "url"),
                repo: x.get("repository").map(|r| s(r, "nameWithOwner")).unwrap_or_default(),
            },
            ForgeKind::GitLab => PullRequest {
                number: x.get("iid").and_then(Value::as_u64).unwrap_or(0),
                title: s(x, "title"),
                author: login(x),
                head: s(x, "source_branch"),
                base: s(x, "target_branch"),
                state: s(x, "state").to_lowercase(),
                draft: x.get("draft").or_else(|| x.get("work_in_progress")).and_then(Value::as_bool).unwrap_or(false),
                url: s(x, "web_url"),
                repo: x.get("references").map(|r| s(r, "full")).and_then(|f| f.split_once('!').map(|(p, _)| p.to_string())).unwrap_or_default(),
            },
        })
        .collect())
}

pub fn parse_issues(kind: ForgeKind, text: &str) -> Result<Vec<Issue>> {
    let v = json(text)?;
    let arr = v.as_array().cloned().unwrap_or_default();
    Ok(arr
        .iter()
        .take(500)
        .map(|x| {
            let labels = x
                .get("labels")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|l| if l.is_string() { l.as_str().unwrap_or("").to_string() } else { s(l, "name") }).collect())
                .unwrap_or_default();
            Issue {
                number: x.get(if kind == ForgeKind::GitHub { "number" } else { "iid" }).and_then(Value::as_u64).unwrap_or(0),
                title: s(x, "title"),
                author: login(x),
                state: s(x, "state").to_lowercase(),
                labels,
                url: s(x, if kind == ForgeKind::GitHub { "url" } else { "web_url" }),
                repo: x.get("repository").map(|r| s(r, "nameWithOwner")).unwrap_or_default(),
            }
        })
        .collect())
}

pub fn parse_runs(kind: ForgeKind, text: &str) -> Result<Vec<CiRun>> {
    let v = json(text)?;
    let arr = v.as_array().cloned().unwrap_or_default();
    Ok(arr
        .iter()
        .take(200)
        .map(|x| match kind {
            ForgeKind::GitHub => CiRun {
                id: x.get("databaseId").and_then(Value::as_u64).unwrap_or(0),
                name: {
                    let w = s(x, "workflowName");
                    let t = s(x, "displayTitle");
                    if w.is_empty() { t } else { format!("{w}: {t}") }
                },
                branch: s(x, "headBranch"),
                status: s(x, "status"),
                conclusion: s(x, "conclusion"),
                url: s(x, "url"),
            },
            ForgeKind::GitLab => {
                let st = s(x, "status");
                let done = matches!(st.as_str(), "success" | "failed" | "canceled" | "skipped");
                CiRun {
                    id: x.get("id").and_then(Value::as_u64).unwrap_or(0),
                    name: format!("pipeline {}", s(x, "id")),
                    branch: s(x, "ref"),
                    status: if done { "completed".into() } else { st.clone() },
                    conclusion: if done { if st == "failed" { "failure".into() } else { st } } else { String::new() },
                    url: s(x, "web_url"),
                }
            }
        })
        .collect())
}

impl Repo {
    /// The hosting service of `remote` (else the default remote), when it's one GitCraft knows.
    pub fn forge(&self, remote: Option<&str>) -> Result<Forge> {
        let remotes = self.remotes()?;
        let name = match remote {
            Some(r) => r.to_string(),
            None => self.default_remote()?,
        };
        let r = remotes.iter().find(|x| x.name == name).ok_or_else(|| GitError::Arg(format!("no remote `{name}`")))?;
        let (host, path) = parse_remote_url(&r.fetch_url).ok_or_else(|| GitError::Arg(format!("{name}'s URL isn't a hosting service's")))?;
        let kind = kind_of_host(&host).ok_or_else(|| GitError::Arg(format!("{host} isn't GitHub or GitLab")))?;
        Ok(Forge { kind, remote: name, host, path })
    }

    fn forge_run(&self, f: &Forge, args: &[&str], input: Option<&str>) -> Result<String> {
        let mut a: Vec<&str> = args.to_vec();
        let repo_flag;
        // Name the project explicitly: the tools' own guess can pick another remote.
        if f.kind == ForgeKind::GitHub {
            repo_flag = format!("{}/{}", f.host, f.path);
            a.extend(["--repo", repo_flag.as_str()]);
        } else {
            repo_flag = f.path.clone();
            a.extend(["--repo", repo_flag.as_str()]);
        }
        run_tool(f.kind, Some(&self.root), &a, input)
    }

    pub fn pull_requests(&self, f: &Forge, state: &str) -> Result<Vec<PullRequest>> {
        let state = match state {
            "closed" | "merged" | "all" | "open" => state,
            _ => "open",
        };
        let out = match f.kind {
            ForgeKind::GitHub => self.forge_run(
                f,
                &["pr", "list", "--state", state, "--limit", "100", "--json", "number,title,author,headRefName,baseRefName,state,isDraft,url"],
                None,
            )?,
            ForgeKind::GitLab => {
                let mut args = vec!["mr", "list", "--per-page", "100", "-F", "json"];
                match state {
                    "closed" => args.push("--closed"),
                    "merged" => args.push("--merged"),
                    "all" => args.push("--all"),
                    _ => {}
                }
                self.forge_run(f, &args, None)?
            }
        };
        parse_prs(f.kind, &out)
    }

    /// Open a pull request from `head` into `base`; its URL.
    pub fn create_pull_request(&self, f: &Forge, title: &str, body: &str, head: &str, base: &str, draft: bool) -> Result<String> {
        for (what, v) in [("title", title), ("head", head), ("base", base)] {
            if v.trim().is_empty() || v.starts_with('-') || v.contains(['\n', '\r', '\0']) {
                return Err(GitError::Arg(format!("`{what}` is not valid")));
            }
        }
        if body.len() > 65_536 || body.contains('\0') {
            return Err(GitError::Arg("the description is too long".into()));
        }
        let mut args: Vec<&str> = match f.kind {
            ForgeKind::GitHub => vec!["pr", "create", "--title", title, "--body-file", "-", "--head", head, "--base", base],
            ForgeKind::GitLab => {
                vec!["mr", "create", "--title", title, "--description", body, "--source-branch", head, "--target-branch", base, "--yes"]
            }
        };
        if draft {
            args.push("--draft");
        }
        let out = self.forge_run(f, &args, if f.kind == ForgeKind::GitHub { Some(body) } else { None })?;
        Ok(out.lines().rev().find(|l| l.starts_with("http")).unwrap_or(out.trim()).trim().to_string())
    }

    /// Check out a pull request's branch.
    pub fn checkout_pull_request(&self, f: &Forge, number: u64) -> Result<()> {
        let n = number.to_string();
        match f.kind {
            ForgeKind::GitHub => self.forge_run(f, &["pr", "checkout", n.as_str()], None)?,
            ForgeKind::GitLab => self.forge_run(f, &["mr", "checkout", n.as_str()], None)?,
        };
        Ok(())
    }

    pub fn issues(&self, f: &Forge, state: &str) -> Result<Vec<Issue>> {
        let state = if matches!(state, "closed" | "all") { state } else { "open" };
        let out = match f.kind {
            ForgeKind::GitHub => {
                self.forge_run(f, &["issue", "list", "--state", state, "--limit", "100", "--json", "number,title,author,state,labels,url"], None)?
            }
            ForgeKind::GitLab => {
                let mut args = vec!["issue", "list", "--per-page", "100", "-F", "json"];
                if state == "closed" {
                    args.push("--closed");
                } else if state == "all" {
                    args.push("--all");
                }
                self.forge_run(f, &args, None)?
            }
        };
        parse_issues(f.kind, &out)
    }

    /// Recent CI runs, of `branch` when given.
    pub fn ci_runs(&self, f: &Forge, branch: Option<&str>) -> Result<Vec<CiRun>> {
        let b = branch.map(|b| crate::check_name("branch", b)).transpose()?;
        let out = match f.kind {
            ForgeKind::GitHub => {
                let mut args =
                    vec!["run", "list", "--limit", "30", "--json", "databaseId,displayTitle,workflowName,headBranch,status,conclusion,url"];
                if let Some(b) = b {
                    args.extend(["--branch", b]);
                }
                self.forge_run(f, &args, None)?
            }
            ForgeKind::GitLab => {
                let mut args = vec!["ci", "list", "-F", "json", "--per-page", "30"];
                if let Some(b) = b {
                    args.extend(["--ref", b]);
                }
                self.forge_run(f, &args, None)?
            }
        };
        parse_runs(f.kind, &out)
    }

    /// Share a patch as a secret gist (GitHub) or private snippet (GitLab); its URL.
    pub fn share_patch(&self, f: &Forge, name: &str, description: &str, patch: &str) -> Result<String> {
        let name = crate::check_name("file name", name)?;
        if description.contains(['\0', '\n']) || description.len() > 1000 {
            return Err(GitError::Arg("the description is not valid".into()));
        }
        let out = match f.kind {
            ForgeKind::GitHub => {
                run_tool(f.kind, Some(&self.root), &["gist", "create", "--filename", name, "--desc", description, "-"], Some(patch))?
            }
            ForgeKind::GitLab => {
                let repo = f.path.clone();
                run_tool(
                    f.kind,
                    Some(&self.root),
                    &["snippet", "create", "--repo", repo.as_str(), "--title", description, "--filename", name, "--visibility", "private"],
                    Some(patch),
                )?
            }
        };
        Ok(out.lines().rev().find(|l| l.contains("http")).unwrap_or(out.trim()).trim().to_string())
    }
}

/// Whether the tool is installed and signed in (`gh auth status` / `glab auth status`): the text
/// it printed either way.
pub fn auth_status(kind: ForgeKind) -> (bool, String) {
    match run_tool(kind, None, &["auth", "status"], None) {
        Ok(t) => (true, t.trim().to_string()),
        Err(e) => (false, e.to_string()),
    }
}

/// Your open pull requests, the ones waiting for your review and your assigned issues, across
/// repositories (GitHub only: `gh search`).
pub fn launchpad(kind: ForgeKind) -> Result<Value> {
    if kind != ForgeKind::GitHub {
        return Err(GitError::Arg("the launchpad needs GitHub (gh search)".into()));
    }
    let fields = "number,title,author,state,url,repository,isDraft";
    let mine =
        parse_prs(kind, &run_tool(kind, None, &["search", "prs", "--author", "@me", "--state", "open", "--limit", "50", "--json", fields], None)?)?;
    let review = parse_prs(
        kind,
        &run_tool(kind, None, &["search", "prs", "--review-requested", "@me", "--state", "open", "--limit", "50", "--json", fields], None)?,
    )?;
    let issues = parse_issues(
        kind,
        &run_tool(
            kind,
            None,
            &[
                "search",
                "issues",
                "--assignee",
                "@me",
                "--state",
                "open",
                "--limit",
                "50",
                "--json",
                "number,title,author,state,url,repository,labels",
            ],
            None,
        )?,
    )?;
    Ok(serde_json::json!({"myPullRequests": mine, "reviewRequests": review, "issues": issues}))
}

/// Repositories of the signed-in account (for cloning).
pub fn my_repositories(kind: ForgeKind) -> Result<Vec<Value>> {
    let out = match kind {
        ForgeKind::GitHub => {
            run_tool(kind, None, &["repo", "list", "--limit", "200", "--json", "nameWithOwner,description,url,sshUrl,isPrivate"], None)?
        }
        ForgeKind::GitLab => run_tool(kind, None, &["repo", "list", "--per-page", "100", "-F", "json"], None)?,
    };
    let v = json(&out)?;
    Ok(v.as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(500)
        .map(|x| match kind {
            ForgeKind::GitHub => serde_json::json!({"name": s(&x, "nameWithOwner"), "description": s(&x, "description"), "url": s(&x, "url"), "private": x.get("isPrivate")}),
            ForgeKind::GitLab => serde_json::json!({"name": s(&x, "path_with_namespace"), "description": s(&x, "description"), "url": s(&x, "http_url_to_repo"), "private": s(&x, "visibility") == "private"}),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_urls() {
        assert_eq!(parse_remote_url("https://github.com/owner/repo.git"), Some(("github.com".into(), "owner/repo".into())));
        assert_eq!(parse_remote_url("git@github.com:owner/repo.git"), Some(("github.com".into(), "owner/repo".into())));
        assert_eq!(
            parse_remote_url("ssh://git@gitlab.example.com:2222/group/sub/proj"),
            Some(("gitlab.example.com".into(), "group/sub/proj".into()))
        );
        assert_eq!(parse_remote_url("https://user:pw@GitHub.com/a/b"), Some(("github.com".into(), "a/b".into())));
        assert_eq!(parse_remote_url("/local/path"), None);
        assert_eq!(kind_of_host("github.com"), Some(ForgeKind::GitHub));
        assert_eq!(kind_of_host("gitlab.example.com"), Some(ForgeKind::GitLab));
        assert_eq!(kind_of_host("codeberg.org"), None);
    }

    #[test]
    fn parses_tool_output() {
        let gh = r#"[{"number":7,"title":"Fix it","author":{"login":"ada"},"headRefName":"fix","baseRefName":"main","state":"OPEN","isDraft":true,"url":"https://github.com/o/r/pull/7"}]"#;
        let p = parse_prs(ForgeKind::GitHub, gh).unwrap();
        assert_eq!((p[0].number, p[0].author.as_str(), p[0].draft, p[0].state.as_str()), (7, "ada", true, "open"));
        let gl = r#"[{"iid":3,"title":"MR","author":{"username":"bob"},"source_branch":"f","target_branch":"main","state":"opened","web_url":"u","references":{"full":"g/p!3"}}]"#;
        let p = parse_prs(ForgeKind::GitLab, gl).unwrap();
        assert_eq!((p[0].number, p[0].author.as_str(), p[0].repo.as_str()), (3, "bob", "g/p"));
        let i = parse_issues(
            ForgeKind::GitHub,
            r#"[{"number":1,"title":"Bug","author":{"login":"a"},"state":"OPEN","labels":[{"name":"bug"}],"url":"u"}]"#,
        )
        .unwrap();
        assert_eq!(i[0].labels, vec!["bug".to_string()]);
        let r = parse_runs(ForgeKind::GitLab, r#"[{"id":9,"ref":"main","status":"failed","web_url":"u"}]"#).unwrap();
        assert_eq!((r[0].status.as_str(), r[0].conclusion.as_str()), ("completed", "failure"));
        for junk in ["", "{", "null", "[1,2]", "[{\"number\":\"x\"}]", "\"s\""] {
            let _ = parse_prs(ForgeKind::GitHub, junk);
            let _ = parse_issues(ForgeKind::GitLab, junk);
            let _ = parse_runs(ForgeKind::GitHub, junk);
        }
    }
}
