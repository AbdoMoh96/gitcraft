//! Hosting, AI and team views: the left panel's pull requests, issues and CI; and the dialogs for
//! creating a pull request, the launchpad, accounts, insights, team overlap and AI answers.

use egui::{RichText, Sense, vec2};
use gitcraft_engine::git::forge::{self, ForgeKind};
use serde_json::{Value, json};

use crate::dialogs::{Close, Dialog};
use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::{GitApp, util};

/// Background commands whose results the views keep (`GitApp::hosting`).
pub const KEPT: &[&str] = &[
    "pr.list",
    "issues.list",
    "ci.list",
    "launchpad.open",
    "hosting.repos",
    "integrations.connect",
    "commit.explain",
    "pr.describe",
    "conflict.suggest",
];

/// The hosting service of the repository's default remote, from its URL.
pub fn forge_kind(app: &GitApp) -> Option<ForgeKind> {
    let d = &app.session.data;
    let r = d.remotes.iter().find(|r| r.name == "origin").or_else(|| d.remotes.first())?;
    let (host, _) = forge::parse_remote_url(&r.fetch_url)?;
    forge::kind_of_host(&host)
}

/// Run a background command once per repository (the first time a section shows).
fn load_once(app: &mut GitApp, ui: &egui::Ui, id: &'static str, params: Value) {
    let root = app.session.repo.as_ref().map(|r| r.root().display().to_string()).unwrap_or_default();
    let key = egui::Id::new(("hosting_loaded", id, root));
    let tried = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    if !tried && !app.hosting.contains_key(id) && !app.busy(id) {
        ui.ctx().data_mut(|d| d.insert_temp(key, true));
        let _ = app.run(id, params);
    }
}

fn section_header(ui: &mut egui::Ui, id: &str, title: &str, count: Option<usize>, busy: bool) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    let key = egui::Id::new(("refs_section", id));
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    let chev = egui::Rect::from_center_size(rect.left_center() + vec2(9.0, 0.0), vec2(12.0, 12.0));
    icons::paint(ui.painter(), chev, if open { Icon::ChevronDown } else { Icon::Chevron }, t.dim, t.dim);
    let ic = egui::Rect::from_center_size(rect.left_center() + vec2(26.0, 0.0), vec2(15.0, 15.0));
    icons::paint(ui.painter(), ic, Icon::Cloud, t.text, t.accent);
    ui.painter().text(rect.left_center() + vec2(40.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(12.0), t.text);
    let right = if busy { "…".to_string() } else { count.map(|c| c.to_string()).unwrap_or_default() };
    ui.painter().text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, right, egui::FontId::proportional(11.0), t.dim);
    if resp.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_temp(key, open));
    }
    let mut refresh = false;
    resp.context_menu(|ui| {
        if ui.button("Refresh").clicked() {
            refresh = true;
            ui.close();
        }
    });
    (open, refresh)
}

fn link_row(ui: &mut egui::Ui, text: &str, badge: &str, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    let right = rect.right() - 6.0;
    let g = ui.painter().layout_no_wrap(badge.to_string(), egui::FontId::proportional(10.5), t.dim);
    let bw = g.size().x;
    ui.painter().galley(egui::pos2(right - bw, rect.center().y - g.size().y / 2.0), g, t.dim);
    let clip = egui::Rect::from_min_max(egui::pos2(rect.left() + 22.0, rect.top()), egui::pos2(right - bw - 6.0, rect.bottom()));
    let g = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.5), t.text);
    ui.painter().with_clip_rect(clip).galley(egui::pos2(clip.left(), rect.center().y - g.size().y / 2.0), g, t.text);
    resp.on_hover_text(tip)
}

/// Pull requests, issues and CI runs in the left panel (when the remote is GitHub or GitLab).
pub fn sections(app: &mut GitApp, ui: &mut egui::Ui) {
    let Some(kind) = forge_kind(app) else { return };
    let t = Tokens::get(ui.ctx());
    let what = if kind == ForgeKind::GitHub { "PULL REQUESTS" } else { "MERGE REQUESTS" };
    let prs = app.hosting.get("pr.list").and_then(|v| v["pullRequests"].as_array().cloned());
    let (open, refresh) = section_header(ui, "prs", what, prs.as_ref().map(Vec::len), app.busy("pr.list"));
    if refresh {
        let _ = app.run("pr.list", json!({}));
    }
    if open {
        load_once(app, ui, "pr.list", json!({}));
        for pr in prs.unwrap_or_default() {
            let n = pr["number"].as_u64().unwrap_or(0);
            let url = pr["url"].as_str().unwrap_or("").to_string();
            let draft = if pr["draft"] == json!(true) { " (draft)" } else { "" };
            let tip = format!(
                "#{n} {}{draft}\n{} → {} by {}",
                pr["title"].as_str().unwrap_or(""),
                pr["head"].as_str().unwrap_or(""),
                pr["base"].as_str().unwrap_or(""),
                pr["author"].as_str().unwrap_or("")
            );
            let resp = link_row(ui, pr["title"].as_str().unwrap_or(""), &format!("#{n}"), &tip);
            if resp.double_clicked() {
                let _ = util::open_url(&url);
            }
            resp.context_menu(|ui| {
                if ui.button("Open in Browser").clicked() {
                    let _ = util::open_url(&url);
                    ui.close();
                }
                if ui.button("Check Out").clicked() {
                    let _ = app.run("pr.checkout", json!({"number": n}));
                    ui.close();
                }
                if ui.button("Copy Link").clicked() {
                    app.copy(url.clone());
                    ui.close();
                }
            });
        }
    }
    let issues = app.hosting.get("issues.list").and_then(|v| v["issues"].as_array().cloned());
    let (open, refresh) = section_header(ui, "issues", "ISSUES", issues.as_ref().map(Vec::len), app.busy("issues.list"));
    if refresh {
        let _ = app.run("issues.list", json!({}));
    }
    if open {
        load_once(app, ui, "issues.list", json!({}));
        for is in issues.unwrap_or_default() {
            let n = is["number"].as_u64().unwrap_or(0);
            let url = is["url"].as_str().unwrap_or("").to_string();
            let labels: Vec<&str> = is["labels"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let resp = link_row(
                ui,
                is["title"].as_str().unwrap_or(""),
                &format!("#{n}"),
                &format!("by {}  {}", is["author"].as_str().unwrap_or(""), labels.join(", ")),
            );
            if resp.double_clicked() {
                let _ = util::open_url(&url);
            }
            resp.context_menu(|ui| {
                if ui.button("Open in Browser").clicked() {
                    let _ = util::open_url(&url);
                    ui.close();
                }
                if ui.button("Create a Branch for It…").clicked() {
                    let slug: String =
                        is["title"].as_str().unwrap_or("").to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
                    let slug = slug.split('-').filter(|x| !x.is_empty()).take(6).collect::<Vec<_>>().join("-");
                    app.dialog = Dialog::named("branch.create", &app.session, &json!({"name": format!("{n}-{slug}")}));
                    ui.close();
                }
                if ui.button("Copy Link").clicked() {
                    app.copy(url.clone());
                    ui.close();
                }
            });
        }
    }
    let runs = app.hosting.get("ci.list").and_then(|v| v["runs"].as_array().cloned());
    let (open, refresh) = section_header(ui, "ci", "CI", runs.as_ref().map(Vec::len), app.busy("ci.list"));
    if refresh {
        let _ = app.run("ci.list", json!({}));
    }
    if open {
        load_once(app, ui, "ci.list", json!({}));
        for r in runs.unwrap_or_default() {
            let url = r["url"].as_str().unwrap_or("").to_string();
            let conclusion = r["conclusion"].as_str().unwrap_or("");
            let state = if conclusion.is_empty() { r["status"].as_str().unwrap_or("").replace('_', " ") } else { conclusion.to_string() };
            let resp = link_row(ui, r["name"].as_str().unwrap_or(""), &state, &format!("{} on {}", state, r["branch"].as_str().unwrap_or("")));
            let c = match conclusion {
                "success" => t.added,
                "failure" | "cancelled" | "timed_out" => t.danger,
                _ => t.warning,
            };
            ui.painter().circle_filled(egui::pos2(resp.rect.left() + 11.0, resp.rect.center().y), 4.0, c);
            if resp.double_clicked() || resp.clicked() {
                let _ = util::open_url(&url);
            }
        }
    }
}

/// The text of a background answer, when it has arrived.
fn answer(app: &GitApp, id: &str, field: &str) -> Option<String> {
    app.hosting.get(id).and_then(|v| v.get(field)).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn create_pr_body(app: &mut GitApp, ui: &mut egui::Ui, d: &mut Dialog, first: bool) -> Close {
    let t = Tokens::get(ui.ctx());
    let Dialog::CreatePr { title, body, head, base, draft, asked } = d else { return Close::Cancel };
    ui.heading("Create Pull Request");
    ui.add_space(6.0);
    // An AI description that arrived fills empty fields.
    if *asked && let Some(v) = app.hosting.remove("pr.describe") {
        *asked = false;
        if let Some(x) = v["title"].as_str() {
            *title = x.to_string();
        }
        if let Some(x) = v["body"].as_str() {
            *body = x.to_string();
        }
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("FROM").small().strong());
        ui.add(egui::TextEdit::singleline(head).desired_width(160.0));
        ui.label(RichText::new("INTO").small().strong());
        ui.add(egui::TextEdit::singleline(base).desired_width(160.0));
    });
    crate::dialogs::field(ui, "TITLE", title, "What it does", first);
    ui.label(RichText::new("DESCRIPTION").small().strong());
    ui.add(egui::TextEdit::multiline(body).desired_rows(8).desired_width(f32::INFINITY));
    ui.horizontal(|ui| {
        ui.checkbox(draft, "Draft");
        let has_ai = app.session.repo.as_ref().and_then(|r| r.config("gitcraft.aiCommand")).is_some();
        if ui
            .add_enabled(has_ai && !app.busy("pr.describe"), egui::Button::new("Write with AI"))
            .on_disabled_hover_text("Set an AI command in Preferences")
            .clicked()
        {
            *asked = true;
            let _ = app.run("pr.describe", json!({"base": base.trim(), "head": head.trim()}));
        }
        if app.busy("pr.describe") {
            ui.add(egui::Spinner::new().size(12.0));
        }
    });
    ui.label(RichText::new("The branch is pushed first. Uses the gh / glab tool and its sign-in.").small().color(t.dim));
    let (ok, cancel) =
        crate::dialogs::buttons(ui, "Create Pull Request", !title.trim().is_empty() && !head.trim().is_empty() && !base.trim().is_empty(), false);
    if cancel {
        return Close::Cancel;
    }
    if ok {
        return Close::Run(
            "pr.create".into(),
            json!({"title": title.trim(), "body": body, "head": head.trim(), "base": base.trim(), "draft": *draft}),
        );
    }
    Close::Keep
}

fn item_list(app: &mut GitApp, ui: &mut egui::Ui, title: &str, items: &[Value]) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(format!("{title} ({})", items.len())).strong());
    if items.is_empty() {
        ui.label(RichText::new("Nothing here").small().weak());
    }
    for x in items.iter().take(50) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(x["repo"].as_str().unwrap_or("")).small().color(t.dim));
            let label = format!("#{} {}", x["number"], x["title"].as_str().unwrap_or(""));
            if ui.link(label).clicked() {
                let _ = util::open_url(x["url"].as_str().unwrap_or(""));
            }
        });
    }
    ui.add_space(6.0);
    let _ = app;
}

pub(crate) fn launchpad_body(app: &mut GitApp, ui: &mut egui::Ui) -> Close {
    ui.set_width(620.0);
    ui.heading("Launchpad");
    ui.label(
        RichText::new("Your open pull requests, reviews waiting for you and your issues, across GitHub.").small().color(Tokens::get(ui.ctx()).dim),
    );
    ui.add_space(6.0);
    if app.busy("launchpad.open") {
        ui.add(egui::Spinner::new());
    } else if ui.button("Refresh").clicked() {
        let _ = app.run("launchpad.open", json!({}));
    }
    if let Some(v) = app.hosting.get("launchpad.open").cloned() {
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            item_list(app, ui, "Waiting for your review", v["reviewRequests"].as_array().map(Vec::as_slice).unwrap_or(&[]));
            item_list(app, ui, "Your pull requests", v["myPullRequests"].as_array().map(Vec::as_slice).unwrap_or(&[]));
            item_list(app, ui, "Assigned to you", v["issues"].as_array().map(Vec::as_slice).unwrap_or(&[]));
        });
    }
    if crate::dialogs::buttons(ui, "Close", true, false) != (false, false) {
        return Close::Cancel;
    }
    Close::Keep
}

pub(crate) fn accounts_body(app: &mut GitApp, ui: &mut egui::Ui, ai: &mut String) -> Close {
    let t = Tokens::get(ui.ctx());
    ui.set_width(560.0);
    ui.heading("Accounts and AI");
    ui.label(
        RichText::new("GitCraft uses GitHub's gh and GitLab's glab tools for pull requests, issues and CI: they keep your sign-in, GitCraft never sees a token.")
            .small()
            .color(t.dim),
    );
    ui.add_space(6.0);
    if app.busy("integrations.connect") {
        ui.add(egui::Spinner::new());
    } else if ui.button("Check Again").clicked() {
        let _ = app.run("integrations.connect", json!({}));
    }
    let accounts = app.hosting.get("integrations.connect").and_then(|v| v["accounts"].as_array().cloned()).unwrap_or_default();
    for a in accounts {
        let signed_in = a["signedIn"] == json!(true);
        ui.horizontal(|ui| {
            let provider = a["provider"].as_str().unwrap_or("");
            ui.label(RichText::new(provider).strong());
            ui.label(RichText::new(if signed_in { "signed in" } else { "not signed in" }).color(if signed_in { t.added } else { t.warning }));
            if !signed_in {
                let cmd = a["signIn"].as_str().unwrap_or("").to_string();
                if ui.button("Sign In…").on_hover_text(format!("Runs `{cmd}` in the terminal")).clicked() {
                    sign_in(app, &cmd);
                }
            }
        })
        .response
        .on_hover_text(a["detail"].as_str().unwrap_or(""));
    }
    ui.separator();
    ui.label(RichText::new("AI COMMAND").small().strong());
    ui.label(
        RichText::new("A program that reads a prompt on its input and prints the answer (a local model's CLI, or one you trust). Used for commit messages, explanations, pull request descriptions and conflict help. Empty: AI is off and nothing is sent anywhere.")
            .small()
            .color(t.dim),
    );
    ui.add(egui::TextEdit::singleline(ai).hint_text("e.g. ollama run llama3").desired_width(f32::INFINITY));
    let (ok, cancel) = crate::dialogs::buttons(ui, "Save", true, false);
    if cancel {
        return Close::Cancel;
    }
    if ok {
        if app.session.repo.is_some() {
            return Close::Run("config.ai".into(), json!({"command": ai.trim()}));
        }
        app.status("Open a repository to save the AI command (it's kept in your git config)", true);
    }
    Close::Keep
}

/// Start a hosting tool's sign-in in the built-in terminal (it asks its own questions there).
fn sign_in(app: &mut GitApp, cmd: &str) {
    if app.session.repo.is_none() {
        app.status(format!("Open a repository, then run `{cmd}` in its terminal"), true);
        return;
    }
    app.prefs.builtin_terminal = true;
    let _ = app.ui_command("ui.terminal", &json!({"value": true}));
    app.pending_terminal_input = Some(format!("{cmd}\r"));
    app.dialog = None;
}

pub(crate) fn insights_body(app: &mut GitApp, ui: &mut egui::Ui, days: &mut u64, data: &mut Value) -> Close {
    let t = Tokens::get(ui.ctx());
    ui.set_width(600.0);
    ui.heading("Insights");
    ui.horizontal(|ui| {
        ui.label("The last");
        for d in [30u64, 90, 365] {
            if ui.selectable_label(*days == d, format!("{d} days")).clicked() {
                *days = d;
                *data = app.session.run("repo.insights", &json!({"days": d})).unwrap_or_default();
            }
        }
    });
    ui.label(RichText::new(format!("{} commits, +{} −{} lines", data["commits"], data["added"], data["removed"])).strong());
    ui.add_space(6.0);
    let bar = |ui: &mut egui::Ui, label: &str, n: u64, max: u64, color: egui::Color32| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(170.0, 18.0), egui::Label::new(RichText::new(label).small()).truncate());
            let w = 300.0 * n as f32 / max.max(1) as f32;
            let (r, _) = ui.allocate_exact_size(vec2(300.0, 12.0), Sense::hover());
            ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(w.max(1.0), 12.0)), 2.0, color);
            ui.label(RichText::new(n.to_string()).small().color(t.dim));
        });
    };
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        ui.label(RichText::new("BY AUTHOR").small().strong());
        let authors = data["authors"].as_array().cloned().unwrap_or_default();
        let max = authors.first().and_then(|a| a["commits"].as_u64()).unwrap_or(1);
        for a in authors.iter().take(15) {
            bar(ui, a["name"].as_str().unwrap_or(""), a["commits"].as_u64().unwrap_or(0), max, crate::theme::APP_COLOR);
        }
        ui.add_space(6.0);
        ui.label(RichText::new("BY DAY OF THE WEEK").small().strong());
        let wd = data["weekdays"].as_array().cloned().unwrap_or_default();
        let max = wd.iter().filter_map(|d| d["commits"].as_u64()).max().unwrap_or(1);
        for d in &wd {
            bar(ui, d["day"].as_str().unwrap_or(""), d["commits"].as_u64().unwrap_or(0), max, t.accent);
        }
        ui.add_space(6.0);
        ui.label(RichText::new("MOST CHANGED FILES").small().strong());
        let files = data["files"].as_array().cloned().unwrap_or_default();
        let max = files.first().and_then(|f| f["commits"].as_u64()).unwrap_or(1);
        for f in files.iter().take(15) {
            bar(ui, f["path"].as_str().unwrap_or(""), f["commits"].as_u64().unwrap_or(0), max, t.modified);
        }
    });
    if crate::dialogs::buttons(ui, "Close", true, false) != (false, false) {
        return Close::Cancel;
    }
    Close::Keep
}

pub(crate) fn team_body(app: &mut GitApp, ui: &mut egui::Ui, data: &mut Value) -> Close {
    let t = Tokens::get(ui.ctx());
    ui.set_width(560.0);
    ui.heading("Team: Changes Like Yours");
    ui.label(
        RichText::new("Remote branches changed in the last 30 days that touch the files you're changing (uncommitted, or not pushed yet). Fetch first to see the latest.")
            .small()
            .color(t.dim),
    );
    ui.add_space(6.0);
    if ui.button("Check Again").clicked() {
        *data = app.session.run("team.overlap", &json!({})).unwrap_or_else(|e| json!({"error": e.to_string()}));
    }
    if let Some(e) = data["error"].as_str() {
        ui.label(RichText::new(e).color(t.danger));
    }
    let branches = data["branches"].as_array().cloned().unwrap_or_default();
    if branches.is_empty() && data["error"].is_null() {
        ui.label(RichText::new("No one else is changing your files.").color(t.added));
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for b in branches {
            ui.horizontal(|ui| {
                ui.label(RichText::new(b["branch"].as_str().unwrap_or("")).strong());
                ui.label(
                    RichText::new(format!(
                        "{}, {}",
                        b["author"].as_str().unwrap_or(""),
                        util::relative_time(b["time"].as_i64().unwrap_or(0), util::now_s())
                    ))
                    .small()
                    .color(t.dim),
                );
            });
            let files: Vec<&str> = b["files"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            ui.label(RichText::new(files.join(", ")).small().color(t.warning));
            ui.add_space(4.0);
        }
    });
    if crate::dialogs::buttons(ui, "Close", true, false) != (false, false) {
        return Close::Cancel;
    }
    Close::Keep
}

/// An AI answer (an explanation, a conflict suggestion), shown when it arrives.
pub(crate) fn ai_text_body(app: &mut GitApp, ui: &mut egui::Ui, title: &str, id: &str, field: &str) -> Close {
    ui.set_width(600.0);
    ui.heading(title);
    ui.add_space(6.0);
    match answer(app, id, field) {
        Some(text) => {
            egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text.as_str())).wrap());
            });
            if ui.button("Copy").clicked() {
                app.copy(text);
            }
        }
        None if app.busy(id) => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Asking the AI command…");
            });
        }
        None => {
            ui.label(RichText::new("No answer (see Activity for the error).").weak());
        }
    }
    if crate::dialogs::buttons(ui, "Close", true, false) != (false, false) {
        app.hosting.remove(id);
        return Close::Cancel;
    }
    Close::Keep
}
