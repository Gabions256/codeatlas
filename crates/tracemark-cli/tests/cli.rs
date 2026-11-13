//! End-to-end checks of the `tracemark` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn changes_outside_a_git_repository_fails_cleanly() {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = tmp.join("cli-no-git");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.ts"), "function a() {}\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_tracemark"))
        .args(["changes", "--project"])
        .arg(&root)
        // The temp dir sits inside this repository's checkout. Stop git
        // from finding that repository above the project.
        .env("GIT_CEILING_DIRECTORIES", tmp)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(
        stderr.contains("is not a commit in a git repository"),
        "{stderr}"
    );
}

#[test]
fn query_without_an_index_says_to_scan() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-no-index");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tracemark"))
        .args(["symbols", "list", "--project"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("run `tracemark scan` first"));
}

#[test]
fn review_fail_on_sets_the_exit_code() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-review");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@localhost",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    fs::write(
        root.join("a.ts"),
        "export function gone() {}\nfunction main() { gone(); }\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    fs::write(root.join("a.ts"), "function main() { gone(); }\n").unwrap();

    let run = |fail_on: &str| {
        Command::new(env!("CARGO_BIN_EXE_tracemark"))
            .args(["review", "--fail-on", fail_on, "--project"])
            .arg(&root)
            .output()
            .unwrap()
    };
    let blocked = run("blocker");
    assert_eq!(blocked.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&blocked.stdout);
    assert!(
        stdout.starts_with("Recommendation: Request changes"),
        "{stdout}"
    );
    assert!(stdout.contains("removed-still-called"));
    assert_eq!(run("never").status.code(), Some(0));
}

/// Sends each line to `tracemark mcp`, closes stdin, returns the replies.
fn mcp_session(root: &Path, lines: &[&str]) -> Vec<serde_json::Value> {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_tracemark"))
        .args(["mcp", "--project"])
        .arg(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("every stdout line is JSON-RPC"))
        .collect()
}

/// Parses the JSON a tool returned in its text content.
fn tool_json(reply: &serde_json::Value) -> serde_json::Value {
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn mcp_server_speaks_json_rpc_and_answers_with_evidence() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-mcp");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@localhost",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    fs::write(
        root.join("a.ts"),
        "export function helper() {}\nexport function gone() {}\nfunction main() { helper(); gone(); }\n",
    )
    .unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    // The agent's edit: `gone` deleted while still called.
    fs::write(
        root.join("a.ts"),
        "export function helper() {}\nfunction main() { helper(); gone(); }\n",
    )
    .unwrap();

    let replies = mcp_session(
        &root,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"tracemark_graph","arguments":{"symbol":"helper"}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"tracemark_review","arguments":{}}}"#,
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"tracemark_explain","arguments":{"name":"nope"}}}"#,
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}"#,
            r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#,
            "{not json",
            r#"{"jsonrpc":"2.0","id":8,"method":"ping"}"#,
        ],
    );
    // One reply per request with an id, plus the parse error. None for the notification.
    let ids: Vec<_> = replies.iter().map(|r| r["id"].clone()).collect();
    assert_eq!(
        ids,
        serde_json::json!([1, 2, 3, 4, 5, 6, 7, null, 8])
            .as_array()
            .unwrap()
            .clone()
    );

    let init = &replies[0]["result"];
    assert_eq!(init["protocolVersion"], "2025-03-26");
    assert_eq!(init["serverInfo"]["name"], "tracemark");
    assert!(init["instructions"].as_str().unwrap().contains("Inferred"));

    let names: Vec<_> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "tracemark_scan",
            "tracemark_symbols",
            "tracemark_graph",
            "tracemark_explain",
            "tracemark_changes",
            "tracemark_review"
        ]
    );

    let graph = tool_json(&replies[2]);
    assert_eq!(graph["edges"][0]["from"], "main");
    assert_eq!(graph["edges"][0]["evidence"], "Inferred");
    assert_eq!(graph["truncated"], false);

    let review = tool_json(&replies[3]);
    assert_eq!(review["recommendation"], "RequestChanges");
    assert_eq!(review["findings"][0]["rule"], "removed-still-called");

    // A failing tool is a result the agent can read, not a protocol error.
    assert_eq!(replies[4]["result"]["isError"], true);
    assert!(replies[4]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("no function or method named `nope`"));
    assert_eq!(replies[5]["error"]["code"], -32602);
    assert_eq!(replies[6]["error"]["code"], -32601);
    assert_eq!(replies[7]["error"]["code"], -32700);
    assert_eq!(replies[8]["result"], serde_json::json!({}));
}
