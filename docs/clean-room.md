# Clean-room record

Status: M1 · Revision 1 (2026-10-17)

GitCraft reimplements the workflow of the leading graphical Git client without using any of its code
or assets. This file records how the target was studied and what was learned, so anyone can check
that nothing protected came in.

## Rules

- **Allowed:** the package's metadata and file *listing*; the vendor's public documentation, website
  and feature names; running the app as a black box with synthetic repositories (screenshots stay in
  `plan/`, never committed); git's own public documentation.
- **Not allowed:** unpacking, reading, decompiling or copying anything inside the app bundle
  (`app.asar`, its JavaScript, styles, images, fonts, sounds, translations); reusing its wording beyond
  feature names; reproducing its icons, colours or layout pixel for pixel; bypassing its licence or
  account checks. Git's source code is GPL-2.0 and is not read either: behaviour comes from running git
  and its manuals.

## What was examined (2026-10-17)

The input was a Debian package, `gitkraken-amd64.deb`. Only `control.tar.xz` (package metadata) and
the *names* of the files in `data.tar.xz` were read:

| Fact | Where it came from |
|---|---|
| Version 12.6.0, section `devel`, about 700 MB installed | `control` |
| An Electron application (Chromium runtime libraries, `resources/app.asar`) | file listing |
| It bundles its own `git` (with `git-core` helpers) and `git-lfs` under `app.asar.unpacked/git/` | file listing |
| It registers URL handlers for itself and a companion CLI | `usr/share/applications/*.desktop` names |
| It suggests the GNOME keyring for credentials | `control` (`Suggests`) |

`app.asar` and every other file's contents were left unopened.

## What GitCraft took from it

- **Architecture, at the level of an idea:** a graphical client that drives the real `git` program is
  a sound design (the incumbent ships git for that). GitCraft runs the `git` on the user's `PATH`
  (`GITCRAFT_GIT` to choose another) and parses its porcelain output; it ships no git.
- **The feature list:** feature names from the vendor's public documentation and from using the app,
  recorded in `crates/engine/src/catalog.rs` and measured in `docs/parity-checklist.md`.

## What GitCraft did not take

No code, text, icons, images, fonts, colours, sounds or layouts. GitCraft's icons are drawn in code,
its colours and lane palette are its own (`crates/ui-egui/src/theme.rs`), and its commit graph layout
algorithm (`crates/graph`) was written from scratch.
