use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gitcraft-cli"))
}

#[test]
fn help_version_commands_parity() {
    let o = cli().arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("USAGE"));
    let o = cli().arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("gitcraft-cli "));
    let o = cli().arg("commands").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("commit.create"));
    let o = cli().args(["parity"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["implemented"].as_u64().unwrap() > 50);
    let o = cli().arg("nope").output().unwrap();
    assert!(!o.status.success());
}

#[test]
fn status_log_and_run_on_a_repository() {
    let (_d, repo) = gitcraft_git::testing::sample("cli").unwrap();
    let dir = repo.root().to_string_lossy().into_owned();
    // The CLI uses the user's git config; give it the test's isolated one.
    let env: Vec<(String, String)> = repo.git().env.clone();
    let run = |args: &[&str]| {
        let mut c = cli();
        c.args(args).envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        c.output().unwrap()
    };
    let o = run(&["status", "--repo", &dir]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["branch"], serde_json::json!("main"));
    let o = run(&["log", "--repo", &dir]);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("Add a library") && text.contains("HEAD → main"), "{text}");
    assert!(text.contains('●'));
    let o = run(&["run", "--repo", &dir, "--cmd", r#"branch.create={"name":"cli-branch"}"#, "--cmd", "repo.status"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(repo.resolve("refs/heads/cli-branch").is_some());
    let o = run(&["run", "--repo", &dir, "--cmd", "stage.all"]);
    assert!(!o.status.success(), "nothing to stage is an error");
    let o = run(&["status", "--repo", "/definitely/not/here"]);
    assert!(!o.status.success());
}
