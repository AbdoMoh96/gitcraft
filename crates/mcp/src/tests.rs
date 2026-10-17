use serde_json::{Value, json};

use crate::{Headless, Server};

fn call(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    serde_json::from_str(&s.handle_line(&line).unwrap()).unwrap()
}

fn tool(s: &mut Server, id: u64, name: &str, args: Value) -> (bool, String) {
    let r = call(s, id, "tools/call", json!({"name": name, "arguments": args}));
    let res = &r["result"];
    (res["isError"].as_bool().unwrap_or(true), res["content"][0]["text"].as_str().unwrap_or("").to_string())
}

#[test]
fn lifecycle_and_tools() {
    let (_d, repo) = gitcraft_git::testing::sample("mcp").unwrap();
    let mut session = gitcraft_engine::Session::new(repo.git().clone());
    session.set_repo(repo.clone()).unwrap();
    let mut s = Server::new(Box::new(Headless::new(session)));
    let init = call(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("gitcraft"));
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(s.is_initialized());
    let tools = call(&mut s, 2, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().len() >= 15);
    let (err, text) = tool(&mut s, 3, "status", json!({}));
    assert!(!err, "{text}");
    assert!(text.contains("\"branch\": \"main\""));
    gitcraft_git::testing::write(&repo, "x.txt", "x\n").unwrap();
    let (err, text) = tool(&mut s, 4, "stage", json!({"paths": ["x.txt"]}));
    assert!(!err, "{text}");
    let (err, text) = tool(&mut s, 5, "commit", json!({"summary": "From an agent"}));
    assert!(!err, "{text}");
    let (_, text) = tool(&mut s, 6, "log", json!({"limit": 1, "ref": "HEAD"}));
    assert!(text.contains("From an agent"));
    let (err, _) = tool(&mut s, 7, "screenshot", json!({}));
    assert!(err, "headless has no window");
    let (err, text) = tool(&mut s, 8, "list_commands", json!({"query": "stash"}));
    assert!(!err && text.contains("stash.save"));
    let (err, _) = tool(&mut s, 9, "batch", json!({"commands": [{"command": "repo.status"}, {"command": "nope"}]}));
    assert!(err);
    let r = call(&mut s, 10, "resources/read", json!({"uri": "gitcraft://repository"}));
    assert!(r["result"]["contents"][0]["text"].as_str().unwrap().contains("feature"));
    let r = call(&mut s, 11, "no/such", json!({}));
    assert_eq!(r["error"]["code"], json!(-32601));
    assert!(s.handle_line("{not json").unwrap().contains("-32700"));
}
