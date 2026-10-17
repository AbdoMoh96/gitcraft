//! Workspace tooling: `cargo xtask <command>`.
//!
//! Pure Rust (std + serde_json). External tools (`cargo`) are invoked through
//! `std::process::Command`.

mod assets;
mod layers;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  ci        fmt --check, clippy -D warnings, test, assets, layers (stops at the first failure)
  layers    enforce the crate dependency layering (docs/architecture.md)
  assets    check that every image, icon, font or sound file is attributed in ATTRIBUTION.md
  parity    recompute the feature-parity checklist in docs/parity-checklist.md
  stats     count tests and lines per crate
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("layers") => cmd_layers(),
        Some("assets") => assets::run(&root()),
        Some("parity") => parity(),
        Some("stats") => stats(),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root (parent of the xtask crate).
pub fn root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().map(Path::to_path_buf).unwrap_or_else(|| here.to_path_buf())
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    eprintln!("$ {what}");
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

fn ci() -> Result<(), String> {
    let mut fmt = cargo();
    fmt.args(["fmt", "--all", "--", "--check"]);
    run(fmt, "cargo fmt --all -- --check")?;
    let mut clippy = cargo();
    clippy.args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
    run(clippy, "cargo clippy --workspace --all-targets -- -D warnings")?;
    let mut test = cargo();
    test.args(["test", "--workspace"]);
    run(test, "cargo test --workspace")?;
    assets::run(&root())?;
    cmd_layers()?;
    eprintln!("ci: all green");
    Ok(())
}

fn metadata() -> Result<serde_json::Value, String> {
    let out = cargo().args(["metadata", "--format-version", "1", "--no-deps"]).output().map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: bad JSON: {e}"))
}

fn cmd_layers() -> Result<(), String> {
    let crates = layers::from_metadata(&metadata()?)?;
    let violations = layers::check(&crates);
    for c in &crates {
        let deps: Vec<String> = c.deps.iter().filter(|d| d.workspace).map(|d| layers::short_name(&d.name).to_string()).collect();
        println!("{:<24} {:<10} {}", c.name, layers::describe(layers::classify(&c.name)), deps.join(", "));
    }
    if violations.is_empty() {
        println!("layers: OK ({} crates)", crates.len());
        Ok(())
    } else {
        for v in &violations {
            println!("  - {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

/// Writes docs/parity-checklist.md from `gitcraft-cli parity --markdown`.
fn parity() -> Result<(), String> {
    let out = cargo().args(["run", "-q", "-p", "gitcraft-cli", "--", "parity", "--markdown"]).output().map_err(|e| format!("gitcraft-cli: {e}"))?;
    if !out.status.success() {
        return Err(format!("gitcraft-cli parity failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let path = root().join("docs/parity-checklist.md");
    std::fs::write(&path, &out.stdout).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

fn stats() -> Result<(), String> {
    let mut total = (0usize, 0usize);
    for dir in ["crates", "apps"] {
        let Ok(entries) = std::fs::read_dir(root().join(dir)) else { continue };
        let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        names.sort();
        for krate in names {
            let (mut lines, mut tests) = (0usize, 0usize);
            walk(&krate, &mut |p| {
                if p.extension().is_some_and(|e| e == "rs")
                    && let Ok(s) = std::fs::read_to_string(p)
                {
                    lines += s.lines().count();
                    tests += s.matches("#[test]").count();
                }
            });
            println!("{:<28} {:>7} lines {:>5} tests", krate.strip_prefix(root()).unwrap_or(&krate).display(), lines, tests);
            total.0 += lines;
            total.1 += tests;
        }
    }
    println!("{:<28} {:>7} lines {:>5} tests", "total", total.0, total.1);
    Ok(())
}

pub fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name.starts_with('.') || name.starts_with("target-") || name == "plan" {
            continue;
        }
        if p.is_dir() {
            walk(&p, f);
        } else {
            f(&p);
        }
    }
}
