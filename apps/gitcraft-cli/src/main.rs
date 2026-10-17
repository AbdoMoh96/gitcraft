//! `gitcraft-cli`: GitCraft from the command line.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod graph_text;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcraft_engine::Session;
use serde_json::{Value, json};

const USAGE: &str = "\
gitcraft-cli — GitCraft from the command line

USAGE:
  gitcraft-cli status [--repo DIR]            branch, upstream, files, branches, tags, stashes (JSON)
  gitcraft-cli log [--repo DIR] [--limit N] [--all | --ref REF]
                                              the commit graph as text, newest first
  gitcraft-cli run [--repo DIR] --cmd 'id={json}' [--cmd …]
                                              run commands in order; prints each result (JSON)
  gitcraft-cli commands [--json]              list every command
  gitcraft-cli parity [--markdown]            feature-catalog parity
  gitcraft-cli mcp [--repo DIR | --connect HOST:PORT]
                                              MCP server on stdio (headless, or bridged to the app)
  gitcraft-cli [<command>] -h | --help        this help
  gitcraft-cli --version

--repo defaults to the current folder. GITCRAFT_GIT names the git program (default: git on PATH).
";

const SUBCOMMANDS: &[&str] = &["status", "log", "run", "commands", "parity", "mcp"];

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn all_values(args: &[String], name: &str) -> Vec<String> {
    args.windows(2).filter(|w| w.first().is_some_and(|a| a == name)).filter_map(|w| w.get(1).cloned()).collect()
}

fn open(args: &[String]) -> Result<Session, String> {
    let dir = arg_value(args, "--repo").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let mut s = Session::default();
    s.open(&dir).map_err(|e| e.to_string())?;
    Ok(s)
}

fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

/// `id={json}` or `id`.
fn parse_cmd(spec: &str) -> Result<(String, Value), String> {
    match spec.split_once('=') {
        Some((id, p)) => {
            let v: Value = serde_json::from_str(p).map_err(|e| format!("--cmd {id}: params are not JSON: {e}"))?;
            Ok((id.trim().to_string(), v))
        }
        None => Ok((spec.trim().to_string(), json!({}))),
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some(cmd) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return Ok(());
    };
    let rest = args.get(1..).unwrap_or(&[]);
    if SUBCOMMANDS.contains(&cmd) && rest.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    match cmd {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" => {
            println!("gitcraft-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "status" => {
            let s = open(rest)?;
            print_json(&s.inspect());
            Ok(())
        }
        "log" => {
            let mut s = open(rest)?;
            if let Some(n) = arg_value(rest, "--limit") {
                s.view.commit_limit = n.parse::<usize>().map_err(|_| format!("--limit {n}: not a number"))?.clamp(1, 1_000_000);
            } else {
                s.view.commit_limit = 50;
            }
            let reference = arg_value(rest, "--ref");
            if let Some(r) = &reference {
                let v = s.run("graph.log", &json!({"ref": r, "limit": s.view.commit_limit})).map_err(|e| e.to_string())?;
                for c in v["commits"].as_array().into_iter().flatten() {
                    println!("{} {}", c["short"].as_str().unwrap_or(""), c["summary"].as_str().unwrap_or(""));
                }
                return Ok(());
            }
            s.refresh().map_err(|e| e.to_string())?;
            let mut out = std::io::stdout().lock();
            for line in graph_text::render(&s.data) {
                if writeln!(out, "{line}").is_err() {
                    break;
                }
            }
            Ok(())
        }
        "run" => {
            let mut s = open(rest)?;
            let cmds = all_values(rest, "--cmd");
            if cmds.is_empty() {
                return Err("run: give at least one --cmd 'id={json}'".into());
            }
            for c in cmds {
                let (id, p) = parse_cmd(&c)?;
                let v = s.run(&id, &p).map_err(|e| format!("{id}: {e}"))?;
                print_json(&json!({"command": id, "result": v}));
            }
            Ok(())
        }
        "commands" => {
            let s = Session::default();
            if rest.iter().any(|a| a == "--json") {
                print_json(&s.registry.describe());
            } else {
                for c in s.registry.all() {
                    let key = if c.shortcut.is_empty() { String::new() } else { format!("  [{}]", c.shortcut) };
                    println!("{:<28} {:<34} {}{key}", c.id, c.label, c.location);
                }
            }
            Ok(())
        }
        "parity" => {
            let s = Session::default();
            if rest.iter().any(|a| a == "--markdown") {
                print!("{}", gitcraft_engine::catalog::markdown(&s.registry));
            } else {
                print_json(&gitcraft_engine::catalog::parity(&s.registry));
            }
            Ok(())
        }
        "mcp" => {
            let backend: Box<dyn gitcraft_mcp::Backend> = match arg_value(rest, "--connect") {
                Some(addr) => Box::new(gitcraft_mcp::Remote::connect(&addr).map_err(|e| format!("--connect {addr}: {e}"))?),
                None => {
                    let mut s = Session::default();
                    if let Some(dir) = arg_value(rest, "--repo") {
                        s.open(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
                    } else if let Ok(cwd) = std::env::current_dir() {
                        // Inside a repository: start there; anywhere else: open_repository first.
                        let _ = s.open(&cwd);
                    }
                    Box::new(gitcraft_mcp::Headless::new(s))
                }
            };
            let mut server = gitcraft_mcp::Server::new(backend);
            let stdin = std::io::stdin();
            server.serve(stdin.lock(), std::io::stdout().lock()).map_err(|e| format!("mcp: {e}"))
        }
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
