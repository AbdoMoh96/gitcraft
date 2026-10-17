//! MCP tools: names, schemas and implementations on top of a [`Backend`].

use serde_json::{Value, json};

use crate::backend::Backend;

#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: false }
    }
    pub fn json(v: &Value) -> Self {
        Self::text(serde_json::to_string_pretty(v).unwrap_or_default())
    }
    pub fn error(t: impl Into<String>) -> Self {
        Self { content: vec![json!({"type": "text", "text": t.into()})], is_error: true }
    }
    pub fn image(png_base64: String, info: &Value) -> Self {
        Self {
            content: vec![json!({"type": "image", "data": png_base64, "mimeType": "image/png"}), json!({"type": "text", "text": info.to_string()})],
            is_error: false,
        }
    }
    pub fn to_value(&self) -> Value {
        json!({"content": self.content, "isError": self.is_error})
    }
}

fn s(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}
fn n(desc: &str) -> Value {
    json!({"type": "number", "description": desc})
}
fn b(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}
fn strs(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": desc})
}
fn obj(props: Value, req: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": req})
}
fn tool(name: &str, title: &str, desc: &str, schema: Value, ro: bool) -> Value {
    json!({"name": name, "title": title, "description": desc, "inputSchema": schema, "annotations": {"title": title, "readOnlyHint": ro, "openWorldHint": false}})
}

pub fn tool_definitions() -> Value {
    json!([
        tool(
            "list_commands",
            "List commands",
            "Every GitCraft command: id, label, UI location, shortcut and params. Filter with `query`.",
            obj(json!({"query": s("Case-insensitive filter on id/label/location")}), &[]),
            true
        ),
        tool(
            "execute",
            "Run a command",
            "Run one command by id with params, e.g. {command:\"stage.all\"} or {command:\"branch.create\", params:{name:\"topic\"}}.",
            obj(json!({"command": s("Command id"), "params": {"type": "object", "description": "Command parameters"}}), &["command"]),
            false
        ),
        tool(
            "batch",
            "Run several commands",
            "Run commands in order; stops at the first error unless keepGoing.",
            obj(
                json!({"commands": {"type": "array", "items": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}, "keepGoing": b("Run the rest after an error")}),
                &["commands"]
            ),
            false
        ),
        tool(
            "open_repository",
            "Open repository",
            "Open the git repository containing a folder.",
            obj(json!({"path": s("Folder path")}), &["path"]),
            false
        ),
        tool(
            "status",
            "Status",
            "Branch, upstream, ahead/behind, staged, unstaged and conflicted files, branches, tags, stashes, remotes.",
            obj(json!({}), &[]),
            true
        ),
        tool(
            "log",
            "History",
            "Commits newest first.",
            obj(
                json!({"limit": n("Default 50"), "ref": s("Start from this ref (default: every branch)"), "path": s("Only commits touching this file"), "grep": s("Message contains"), "author": s("Author contains")}),
                &[]
            ),
            true
        ),
        tool(
            "show_commit",
            "Show commit",
            "A commit's message, author, parents, refs and changed files.",
            obj(json!({"oid": s("Commit id or ref")}), &["oid"]),
            true
        ),
        tool(
            "diff",
            "Diff",
            "A file's diff with numbered hunks and lines: unstaged or staged changes, or a commit's change.",
            obj(
                json!({"path": s("File path"), "area": s("unstaged | staged | commit"), "oid": s("Commit (for area commit)"), "context": n("Context lines")}),
                &["path"]
            ),
            true
        ),
        tool("stage", "Stage", "Stage files (all changes when `paths` is omitted).", obj(json!({"paths": strs("Files to stage")}), &[]), false),
        tool(
            "unstage",
            "Unstage",
            "Unstage files (everything when `paths` is omitted).",
            obj(json!({"paths": strs("Files to unstage")}), &[]),
            false
        ),
        tool(
            "commit",
            "Commit",
            "Commit what is staged.",
            obj(json!({"summary": s("First line"), "description": s("Body"), "amend": b("Amend the last commit")}), &["summary"]),
            false
        ),
        tool(
            "checkout",
            "Checkout",
            "Check out a branch (local, or remote through a tracking branch).",
            obj(json!({"name": s("main | origin/feature")}), &["name"]),
            false
        ),
        tool(
            "create_branch",
            "Create branch",
            "Create a branch and check it out.",
            obj(json!({"name": s("Branch name"), "from": s("Start point (default HEAD)"), "checkout": b("Default true")}), &["name"]),
            false
        ),
        tool("fetch", "Fetch", "Fetch from a remote (all by default).", obj(json!({"remote": s("Remote name")}), &[]), false),
        tool("pull", "Pull", "Pull the current branch.", obj(json!({"mode": s("merge | ffOnly | rebase")}), &[]), false),
        tool(
            "push",
            "Push",
            "Push the current branch (sets the upstream when it has none).",
            obj(json!({"remote": s("Remote"), "branch": s("Branch"), "force": b("Force with lease")}), &[]),
            false
        ),
        tool(
            "parity",
            "Feature parity",
            "How much of the incumbent Git client's feature list GitCraft implements, and what is missing.",
            obj(json!({}), &[]),
            true
        ),
        tool("screenshot", "Screenshot", "A picture of the GitCraft window (needs the app: --connect).", obj(json!({}), &[]), true),
    ])
}

fn exec(b: &mut dyn Backend, id: &str, params: Value) -> Result<Value, String> {
    b.call("engine.execute", json!({"command": id, "params": params}))
}

fn wrap(r: Result<Value, String>) -> ToolResult {
    match r {
        Ok(v) => ToolResult::json(&v),
        Err(e) => ToolResult::error(e),
    }
}

/// Copy the named arguments that are present.
fn pick(a: &Value, keys: &[&str]) -> Value {
    let mut o = serde_json::Map::new();
    for k in keys {
        if let Some(v) = a.get(*k).filter(|v| !v.is_null()) {
            o.insert((*k).to_string(), v.clone());
        }
    }
    Value::Object(o)
}

pub fn call_tool(b: &mut dyn Backend, name: &str, args: &Value) -> ToolResult {
    let a = if args.is_object() { args.clone() } else { json!({}) };
    let st = |k: &str| a.get(k).and_then(Value::as_str);
    match name {
        "list_commands" => match b.call("engine.commands", json!({})) {
            Ok(Value::Array(all)) => {
                let q = st("query").unwrap_or("").to_lowercase();
                let hits: Vec<Value> = all
                    .into_iter()
                    .filter(|c| {
                        q.is_empty()
                            || ["id", "label", "location"]
                                .iter()
                                .any(|k| c.get(*k).and_then(Value::as_str).is_some_and(|t| t.to_lowercase().contains(&q)))
                    })
                    .collect();
                ToolResult::json(&Value::Array(hits))
            }
            Ok(other) => ToolResult::json(&other),
            Err(e) => ToolResult::error(e),
        },
        "execute" => {
            let Some(id) = st("command") else { return ToolResult::error("`command` is required") };
            wrap(exec(b, id, a.get("params").cloned().unwrap_or(json!({}))))
        }
        "batch" => {
            let Some(list) = a.get("commands").and_then(Value::as_array) else { return ToolResult::error("`commands` (array) is required") };
            let keep = a.get("keepGoing").and_then(Value::as_bool).unwrap_or(false);
            let mut out = Vec::new();
            for c in list.iter().take(1000) {
                let id = c.get("command").and_then(Value::as_str).unwrap_or("");
                let r = exec(b, id, c.get("params").cloned().unwrap_or(json!({})));
                let failed = r.is_err();
                out.push(match r {
                    Ok(v) => json!({"command": id, "ok": true, "result": v}),
                    Err(e) => json!({"command": id, "ok": false, "error": e}),
                });
                if failed && !keep {
                    return ToolResult { content: vec![json!({"type": "text", "text": Value::Array(out).to_string()})], is_error: true };
                }
            }
            ToolResult::json(&Value::Array(out))
        }
        "open_repository" => wrap(exec(b, "repo.open", pick(&a, &["path"]))),
        "status" => wrap(b.call("repo.inspect", json!({}))),
        "log" => wrap(exec(b, "graph.log", pick(&a, &["limit", "ref", "path", "grep", "author"]))),
        "show_commit" => wrap(exec(b, "commit.show", pick(&a, &["oid"]))),
        "diff" => wrap(exec(b, "diff.file", pick(&a, &["path", "area", "oid", "context"]))),
        "stage" => match a.get("paths").filter(|p| p.as_array().is_some_and(|x| !x.is_empty())) {
            Some(p) => wrap(exec(b, "stage.file", json!({"paths": p}))),
            None => wrap(exec(b, "stage.all", json!({}))),
        },
        "unstage" => match a.get("paths").filter(|p| p.as_array().is_some_and(|x| !x.is_empty())) {
            Some(p) => wrap(exec(b, "unstage.file", json!({"paths": p}))),
            None => wrap(exec(b, "unstage.all", json!({}))),
        },
        "commit" => wrap(exec(b, "commit.create", pick(&a, &["summary", "description", "amend"]))),
        "checkout" => wrap(exec(b, "branch.checkout", pick(&a, &["name"]))),
        "create_branch" => wrap(exec(b, "branch.create", pick(&a, &["name", "from", "checkout"]))),
        "fetch" => wrap(exec(b, "remote.fetch", pick(&a, &["remote"]))),
        "pull" => wrap(exec(b, "remote.pull", pick(&a, &["mode"]))),
        "push" => wrap(exec(b, "remote.push", pick(&a, &["remote", "branch", "force"]))),
        "parity" => wrap(b.call("ui.parity", json!({}))),
        "screenshot" => match b.call("ui.screenshot", json!({})) {
            Ok(v) => match v.get("png").and_then(Value::as_str) {
                Some(png) if !png.is_empty() => ToolResult::image(png.to_string(), &json!({"width": v["width"], "height": v["height"]})),
                _ => ToolResult::error("the reply carried no picture"),
            },
            Err(e) => ToolResult::error(e),
        },
        other => ToolResult::error(format!("unknown tool `{other}`")),
    }
}
