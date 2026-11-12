//! End-to-end checks of the `atlas` binary.

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

    let out = Command::new(env!("CARGO_BIN_EXE_atlas"))
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
    let out = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["symbols", "list", "--project"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("run `atlas scan` first"));
}
