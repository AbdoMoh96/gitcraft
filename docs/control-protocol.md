# Control protocol

Status: M1 · Revision 1 (2026-10-17)

`gitcraft --control <port>` (or `GITCRAFT_CONTROL_PORT`) listens on `127.0.0.1:<port>`. Each request is
one line of JSON; each reply is one line.

```json
{"id": 1, "key": "…", "method": "engine.execute", "params": {"command": "branch.create", "params": {"name": "topic"}}}
{"id": 1, "ok": true, "result": {"message": "Created branch topic", "name": "topic"}}
```

## Keys

When the port is bound the app writes a random key to `<settings>/control-key.local-<port>` (mode
0600 on Unix) and deletes it when the window closes. `<settings>` is `$XDG_CONFIG_HOME/gitcraft` or
`~/.config/gitcraft` on Linux, `~/Library/Application Support/GitCraft` on macOS and
`%APPDATA%\GitCraft` on Windows; `GITCRAFT_PROFILE_DIR` overrides it. A request without the right key
gets `{"ok": false, "error": "unauthorized"}` and the connection is closed. The MCP bridge reads the
key file, or `GITCRAFT_CONTROL_KEY`, and sends it only to loopback addresses.

## Methods

| Method | Params | Result |
|---|---|---|
| `engine.execute` | `{"command": id, "params": {…}}` | the command's result. Network commands reply when they finish |
| `engine.commands` | | every command (id, label, location, shortcut, params, mutates, slow) |
| `repo.inspect` | | branch, head, upstream, ahead/behind, state, staged/unstaged/conflicted files, branches, tags, stashes, remotes, selection, draft, undo/redo |
| `ui.parity` | | feature parity |
| `ui.screenshot` | | `{"png": base64, "width", "height"}` of the window |
| `ui.state` | | open dialog, palette, background jobs, status line, theme, recent repositories |
| `ui.dialog` | `{"name": "branch.create" \| "tag.create" \| "repo.clone" \| …}` | opens a dialog |
| `ui.closeDialog`, `ui.commandPalette`, `ui.theme` `{"value"}`, `ui.recent` `{"index"}`, `ui.revealRepo`, `ui.copySha`, `ui.filterRefs` `{"text"}`, `ui.quit` | | |
