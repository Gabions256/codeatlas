//! `Index::changes` against throwaway git repositories.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tracemark_domain::Evidence;
use tracemark_index::{
    render_markdown, Change, ChangeReport, Index, Recommendation, Review, Severity,
};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn write(root: &Path, path: &str, content: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

/// Fresh repository with `files` committed.
fn repo(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("changes-{name}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    for (path, content) in files {
        write(&root, path, content);
    }
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    root
}

/// `(path, status, [(qualified name, change)])`.
type FileSummary<'a> = (&'a str, &'a str, Vec<(&'a str, Change)>);

fn summary(report: &ChangeReport) -> Vec<FileSummary<'_>> {
    report
        .files
        .iter()
        .map(|f| {
            let symbols = f
                .symbols
                .iter()
                .map(|s| (s.qualified_name.as_str(), s.change))
                .collect();
            (f.path.as_str(), f.status, symbols)
        })
        .collect()
}

#[test]
fn body_signature_added_and_removed_symbols() {
    let root = repo(
        "kinds",
        &[
            (
                "lib.ts",
                "export function body() { return 1; }\n\
                 export function sig(a: number) { return a; }\n\
                 export function same() {}\n\
                 export function gone() {}\n",
            ),
            (
                "app.ts",
                "import { body, sig, gone } from './lib';\n\
                 function main() { body(); sig(1); gone(); }\n\
                 function outer() { main(); }\n",
            ),
        ],
    );
    write(
        &root,
        "lib.ts",
        "export function body() { return 2; }\n\
         export function sig(a: number, b = 0) { return a + b; }\n\
         export function same() {}\n\
         export function fresh() {}\n",
    );
    let mut index = Index::open(&root).unwrap();
    let report = index.changes("HEAD", 2).unwrap();
    assert_eq!(
        summary(&report),
        [(
            "lib.ts",
            "modified",
            vec![
                ("body", Change::BodyChanged),
                ("sig", Change::SignatureChanged),
                ("fresh", Change::Added),
                ("gone", Change::Removed),
            ]
        )]
    );
    let symbols = &report.files[0].symbols;
    assert!(symbols.iter().all(|s| s.evidence == Evidence::Syntax));

    // Two hops: main calls body, outer calls main.
    let body: Vec<_> = symbols[0]
        .may_affect
        .iter()
        .map(|e| (e.from.as_str(), e.depth, e.evidence))
        .collect();
    assert_eq!(
        body,
        [
            ("main", 1, Evidence::Inferred),
            ("outer", 2, Evidence::Inferred)
        ]
    );
    assert!(symbols[2].may_affect.is_empty());
    // `gone()` is still called, but no longer links to anything.
    let gone = &symbols[3].may_affect;
    assert_eq!(gone.len(), 1);
    assert_eq!((gone[0].path.as_str(), gone[0].line), ("app.ts", 2));
    assert_eq!(gone[0].evidence, Evidence::Syntax);
    assert_eq!(report.base, "HEAD");
    assert_eq!(report.base_commit.len(), 40);
    assert!(!report.limitations.is_empty());
}

#[test]
fn renamed_untracked_deleted_and_unsupported_files() {
    let root = repo(
        "files",
        &[
            ("old.py", "def kept():\n    return 1\n"),
            ("drop.rs", "fn dropped() {}\n"),
            ("notes.md", "hello\n"),
        ],
    );
    git(&root, &["mv", "old.py", "new.py"]);
    fs::remove_file(root.join("drop.rs")).unwrap();
    write(&root, "notes.md", "changed\n");
    write(&root, "extra.ts", "export function extra() {}\n");

    let mut index = Index::open(&root).unwrap();
    let report = index.changes("HEAD", 1).unwrap();
    assert_eq!(
        summary(&report),
        [
            ("drop.rs", "deleted", vec![("dropped", Change::Removed)]),
            ("extra.ts", "untracked", vec![("extra", Change::Added)]),
            // Same content: matched by name, so nothing changed inside.
            ("new.py", "renamed", vec![]),
            ("notes.md", "modified", vec![]),
        ]
    );
    assert_eq!(report.files[2].old_path.as_deref(), Some("old.py"));
    assert!(report.files[2].analysed);
    assert!(!report.files[3].analysed);
    assert_eq!(report.files[3].note.as_deref(), Some("no language adapter"));
}

#[test]
fn compares_with_an_older_commit_and_leaves_the_repository_untouched() {
    let root = repo("base", &[("a.ts", "function a() {}\n")]);
    write(&root, "a.ts", "function a() { return 1; }\n");
    git(&root, &["commit", "-q", "-am", "second"]);
    write(&root, "a.ts", "function a() { return 2; }\n");
    let status_before = git(&root, &["status", "--porcelain"]);

    let mut index = Index::open(&root).unwrap();
    let report = index.changes("HEAD~1", 1).unwrap();
    assert_eq!(
        summary(&report),
        [("a.ts", "modified", vec![("a", Change::BodyChanged)])]
    );
    // The index folder ignores itself, so git sees no new file.
    assert_eq!(git(&root, &["status", "--porcelain"]), status_before);

    // Nothing left to report once the change is committed.
    git(&root, &["commit", "-q", "-am", "third"]);
    assert!(index.changes("HEAD", 1).unwrap().files.is_empty());
}

#[test]
fn rejects_bad_bases() {
    let root = repo("bad", &[("a.ts", "function a() {}\n")]);
    let mut index = Index::open(&root).unwrap();
    let mut message = |base: &str| index.changes(base, 1).unwrap_err().to_string();
    assert!(message("--output=x").contains("invalid --base"));
    assert!(message("").contains("invalid --base"));
    assert!(message("no-such-branch").contains("is not a commit"));
}

/// A change to a redacted string default is still a signature change, and
/// neither value reaches the report.
#[test]
fn changed_redacted_default_is_a_signature_change() {
    let root = repo(
        "redacted-default",
        &[("pay.py", "def pay(key=\"sk-old-111\"):\n    return 1\n")],
    );
    write(
        &root,
        "pay.py",
        "def pay(key=\"sk-new-222\"):\n    return 1\n",
    );
    let mut index = Index::open(&root).unwrap();
    let report = index.changes("HEAD", 1).unwrap();
    assert_eq!(
        summary(&report),
        [(
            "pay.py",
            "modified",
            vec![("pay", Change::SignatureChanged)]
        )]
    );
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("sk-old-111") && !json.contains("sk-new-222"));
}

/// `(rule, severity, symbol)` per finding.
fn rules(review: &Review) -> Vec<(&str, Severity, Option<&str>)> {
    review
        .findings
        .iter()
        .map(|f| (f.rule, f.severity, f.symbol.as_deref()))
        .collect()
}

#[test]
fn removed_function_still_called_requests_changes() {
    let root = repo(
        "review-removed",
        &[
            (
                "lib.ts",
                "export function gone() {}\nexport function kept() {}\n",
            ),
            (
                "app.ts",
                "import { gone } from './lib';\nfunction main() { gone(); }\n",
            ),
        ],
    );
    write(&root, "lib.ts", "export function kept() {}\n");
    let review = Index::open(&root).unwrap().review("HEAD", 2).unwrap();
    assert_eq!(review.recommendation, Recommendation::RequestChanges);
    assert_eq!(
        rules(&review),
        [("removed-still-called", Severity::Blocker, Some("gone"))]
    );
    let site = &review.findings[0].sites[0];
    assert_eq!((site.path.as_str(), site.line), ("app.ts", 2));
    assert_eq!(site.evidence, Evidence::Syntax);
}

#[test]
fn signature_change_flags_only_callers_outside_the_change() {
    let root = repo(
        "review-signature",
        &[
            ("lib.ts", "export function pay(a: number) { return a; }\n"),
            ("far.ts", "function far() { pay(1); }\n"),
            ("near.ts", "function near() { pay(1); }\n"),
        ],
    );
    write(
        &root,
        "lib.ts",
        "export function pay(a: number, b: number) { return a + b; }\n",
    );
    // near.ts is updated in the same change, far.ts is not.
    write(&root, "near.ts", "function near() { pay(1, 2); }\n");
    let review = Index::open(&root).unwrap().review("HEAD", 1).unwrap();
    assert_eq!(review.recommendation, Recommendation::ReviewCarefully);
    let finding = review
        .findings
        .iter()
        .find(|f| f.rule == "signature-callers-untouched")
        .unwrap();
    assert_eq!(finding.severity, Severity::High);
    let paths: Vec<_> = finding.sites.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["far.ts"]);
    assert_eq!(finding.evidence, Evidence::Inferred);
}

#[test]
fn new_syntax_errors_are_high() {
    let root = repo("review-syntax", &[("a.ts", "export function a() {}\n")]);
    write(
        &root,
        "a.ts",
        "export function a() {}\nfunction broken( {\n",
    );
    let review = Index::open(&root).unwrap().review("HEAD", 1).unwrap();
    assert!(rules(&review).contains(&("syntax-errors-added", Severity::High, None)));
    assert_eq!(review.recommendation, Recommendation::ReviewCarefully);
}

#[test]
fn wide_impact_is_medium_and_tests_are_recognised() {
    let mut callers = String::from("import { core } from './core';\n");
    for i in 0..10 {
        callers.push_str(&format!("export function c{i}() {{ return core(); }}\n"));
    }
    let root = repo(
        "review-wide",
        &[
            ("core.ts", "export function core() { return 1; }\n"),
            ("callers.ts", &callers),
            (
                "core.test.ts",
                "import { core } from './core';\ntest('core', () => core());\n",
            ),
        ],
    );
    write(&root, "core.ts", "export function core() { return 2; }\n");
    let review = Index::open(&root).unwrap().review("HEAD", 1).unwrap();
    assert_eq!(review.recommendation, Recommendation::ApproveAfterChecks);
    // `core` is called from core.test.ts, so no `no-test-calls` finding.
    assert_eq!(
        rules(&review),
        [("wide-impact", Severity::Medium, Some("core"))]
    );
}

#[test]
fn untested_change_alone_flags_no_risk_but_says_so() {
    let root = repo("review-clean", &[("a.py", "def total(x):\n    return x\n")]);
    write(&root, "a.py", "def total(x):\n    return x + 1\n");
    write(&root, "notes.md", "changed\n");
    // A helper inside a test file is test code: not flagged.
    write(&root, "a_test.py", "def helper():\n    return 1\n");
    let review = Index::open(&root).unwrap().review("HEAD", 1).unwrap();
    assert_eq!(review.recommendation, Recommendation::NoRuleFlagged);
    assert!(review.reason.contains("not proof"));
    assert_eq!(
        rules(&review),
        [
            ("no-test-calls", Severity::Info, Some("total")),
            ("not-analysed", Severity::Info, None),
        ]
    );
    assert!(review.limitations[0].contains("did not run, build or test"));
}

#[test]
fn markdown_cannot_be_hijacked_by_project_names() {
    let root = repo(
        "review-markdown",
        &[(
            "a.ts",
            "export function gone() {}\nfunction main() { gone(); }\n",
        )],
    );
    write(&root, "a.ts", "function main() { gone(); }\n");
    write(&root, "`@evil`.md", "hi\n");
    let review = Index::open(&root).unwrap().review("HEAD", 1).unwrap();
    let md = render_markdown(&review);
    assert!(md.starts_with("### Tracemark review: Request changes"));
    // The file name is inside a code span: no live mention, no broken table.
    assert!(md.contains("| `'@evil'.md` |"), "{md}");
    assert!(!md.contains("`@evil`"));
    assert!(md.contains("did not run, build or test"));
}
