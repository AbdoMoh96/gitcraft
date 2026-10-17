# MCP server

Status: M1 · Revision 1 (2026-10-17)

```sh
gitcraft-cli mcp --repo /path/to/repo           # headless: an in-process session
gitcraft-cli mcp --connect 127.0.0.1:7990       # drives the running app (gitcraft --control 7990)
```

JSON-RPC 2.0 over stdio, MCP revision 2025-06-18 (2025-03-26 and 2024-11-05 accepted).

## Tools

| Tool | What it does |
|---|---|
| `list_commands` | every command, filterable |
| `execute` / `batch` | run any command(s) by id |
| `open_repository` | open a folder's repository |
| `status` | branch, files, branches, tags, stashes, remotes |
| `log` | history, filterable by ref, path, message or author |
| `show_commit` | a commit's message, author, parents, refs and files |
| `diff` | a file's diff (unstaged, staged or in a commit) with numbered hunks and lines |
| `stage` / `unstage` | files, or everything |
| `commit` | commit the index (or amend) |
| `checkout` / `create_branch` | branches |
| `fetch` / `pull` / `push` | remotes |
| `parity` | feature parity |
| `screenshot` | the app window (with `--connect`) |

Resources: `gitcraft://repository` (the `status` data) and `gitcraft://commands`.

## Example: stage two lines of a hunk

1. `diff {path: "src/lib.rs"}` → hunks with `lines[i].kind` (`add`, `del`, `context`).
2. `execute {command: "stage.lines", params: {path: "src/lib.rs", hunk: 0, lines: [3, 4]}}`.
3. `commit {summary: "Fix the bounds check"}`; `execute {command: "edit.undo"}` takes it back.
