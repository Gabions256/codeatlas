use std::fs;
use std::path::{Path, PathBuf};

use atlas_domain::Evidence;
use atlas_index::{render_html, Direction, Edge, Index};

/// Fresh project directory under the cargo test tmpdir.
fn project(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for (path, content) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }
    root
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Scratch copy of a fixture, so tests never write an index into the repo.
fn fixture(language: &str) -> PathBuf {
    let root = project(&format!("fixture-{language}"), &[]);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(language)
        .join("sample");
    copy_dir(&source, &root);
    root
}

fn links(edges: &[Edge]) -> Vec<(&str, &str, &str)> {
    edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str(), e.status.as_str()))
        .collect()
}

#[test]
fn scans_sample_repository() {
    let root = fixture("typescript");
    let mut index = Index::open(&root).unwrap();
    let summary = index.scan().unwrap();
    assert_eq!(summary.files, 4);
    assert_eq!(summary.reparsed, 4);
    // README.md has no adapter; node_modules is never entered.
    assert_eq!(summary.unsupported, 1);
    assert!(summary.warnings.is_empty());

    let report = index.report().unwrap();
    let paths: Vec<_> = report.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "src/broken.ts",
            "src/cart.ts",
            "src/checkout.ts",
            "src/types.ts"
        ]
    );
    assert_eq!(report.files[0].parse_status, "ParsedWithErrors");
    assert!(!report.files[0].diagnostics.is_empty());

    let cart = &report.files[1];
    assert_eq!(cart.parse_status, "Parsed");
    assert_eq!(cart.imports[0].source, "./types");
    assert_eq!(
        cart.imports[0].resolved_path.as_deref(),
        Some("src/types.ts")
    );
    let symbols: Vec<_> = cart
        .symbols
        .iter()
        .map(|s| (s.qualified_name.as_str(), s.kind.as_str(), s.start_line))
        .collect();
    assert_eq!(
        symbols,
        [
            ("Cart", "Interface", 3),
            ("Total", "TypeAlias", 5),
            ("Status", "Enum", 7),
            ("handleCheckout", "Function", 9),
            ("applyDiscount", "Function", 13),
            ("CartService", "Class", 15),
            ("CartService.add", "Method", 16),
        ]
    );

    let dependents = index
        .graph(Some("handleCheckout"), Direction::Dependents, 1)
        .unwrap()
        .edges;
    assert_eq!(
        links(&dependents),
        [("payNow", "handleCheckout", "Inferred")]
    );
    // The evidence for the link is the call site.
    assert_eq!(
        (dependents[0].path.as_str(), dependents[0].line),
        ("src/checkout.ts", 4)
    );

    let dependencies = index
        .graph(Some("payNow"), Direction::Dependencies, 1)
        .unwrap()
        .edges;
    assert_eq!(
        links(&dependencies),
        [
            ("payNow", "handleCheckout", "Inferred"),
            ("payNow", "log", "Unresolved"),
            ("payNow", "applyDiscount", "Inferred"),
        ]
    );

    // The report serializes.
    serde_json::to_string(&report).unwrap();
}

#[test]
fn rescans_only_changed_files_and_relinks_the_rest() {
    let root = project(
        "incremental",
        &[
            ("a.ts", b"export function helper() {}\n"),
            (
                "b.ts",
                b"import { helper } from './a';\nfunction main() { helper(); }\n",
            ),
        ],
    );
    let mut index = Index::open(&root).unwrap();
    let first = index.scan().unwrap();
    assert_eq!((first.reparsed, first.unchanged, first.removed), (2, 0, 0));
    assert_eq!(first.calls_inferred, 1);
    assert_eq!(first.imports_resolved, 1);
    let helper_id = index.find("helper").unwrap()[0].id.clone();

    let second = index.scan().unwrap();
    assert_eq!((second.reparsed, second.unchanged), (0, 2));

    // Editing a body reparses that file only and keeps the symbol id.
    fs::write(
        root.join("a.ts"),
        "export function helper() { return 1; }\n",
    )
    .unwrap();
    let third = index.scan().unwrap();
    assert_eq!((third.reparsed, third.unchanged), (1, 1));
    assert_eq!(index.find("helper").unwrap()[0].id, helper_id);
    assert_eq!(third.calls_inferred, 1);

    // Deleting a.ts unlinks the call in b.ts, which is not reparsed.
    fs::remove_file(root.join("a.ts")).unwrap();
    let fourth = index.scan().unwrap();
    assert_eq!(
        (fourth.reparsed, fourth.unchanged, fourth.removed),
        (0, 1, 1)
    );
    assert_eq!((fourth.calls_inferred, fourth.imports_resolved), (0, 0));
    assert!(index.find("helper").unwrap().is_empty());

    // The index persists across opens.
    drop(index);
    let reopened = Index::open(&root).unwrap();
    assert_eq!(reopened.find("main").unwrap().len(), 1);
}

#[test]
fn ambiguous_names_are_not_linked() {
    let root = project(
        "ambiguous",
        &[
            ("a.ts", b"export function run() {}\n"),
            ("b.ts", b"export function run() {}\n"),
            ("c.ts", b"function main() { run(); obj.run(); }\n"),
        ],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let edges = index
        .graph(Some("main"), Direction::Dependencies, 1)
        .unwrap()
        .edges;
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| e.to_id.is_none()));
    assert!(edges[0].note.contains("ambiguous: 2 candidates"));
    // `obj.run()` is a member call; the only `run`s are plain functions.
    assert!(edges[1].note.contains("no project symbol"));
}

#[test]
fn rust_and_python_calls_follow_declared_rules() {
    let root = fixture("rust");
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let edges = index
        .graph(Some("check"), Direction::Dependencies, 2)
        .unwrap()
        .edges;
    assert_eq!(
        links(&edges),
        [
            ("check", "Cart.new", "Inferred"),
            // `cart.size()`: the receiver is a variable, so it is not linked.
            ("check", "size", "Unresolved"),
            ("Cart.new", "check", "Inferred"),
        ]
    );
    assert!(edges[1].note.contains("1 method(s) have this name"));
    // Rust's flag is the modifier, not reachability.
    let explain = |name: &str| index.explain_function(name).unwrap().remove(0).summary;
    assert!(explain("new").contains("(method, has a `pub` modifier)"));
    assert!(explain("size").contains("(method, no `pub` modifier)"));
    assert_eq!(edges[2].depth, 2);

    let root = fixture("python");
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let explained = index.explain_function("total").unwrap();
    assert_eq!(explained.len(), 1);
    let total = &explained[0];
    assert_eq!(links(&total.called_by), [("Shop.pay", "total", "Inferred")]);
    assert_eq!(links(&total.calls), [("total", "len", "Unresolved")]);
    assert!(total.summary.contains("shop.py at lines 14-15"));
    assert!(total.summary.contains("1 parameter(s): cart"));
    assert!(!total.limitations.is_empty());
    // Python declares no import resolution.
    let report = index.report().unwrap();
    assert!(report.files[0]
        .imports
        .iter()
        .all(|i| i.resolved_path.is_none()));
    // Not a function: nothing to explain.
    assert!(index.explain_function("Shop").unwrap().is_empty());
}

#[test]
fn every_row_carries_its_evidence_class() {
    let root = project(
        "evidence",
        &[
            ("a.ts", b"export function helper() {}\n" as &[u8]),
            (
                "b.ts",
                b"import { helper } from './a';\nimport React from 'react';\n\
                  function main() { helper(); missing(); }\n",
            ),
        ],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let report = index.report().unwrap();
    assert!(report
        .files
        .iter()
        .flat_map(|f| &f.symbols)
        .all(|s| s.evidence == Evidence::Syntax));
    let imports: Vec<_> = report.files[1]
        .imports
        .iter()
        .map(|i| (i.source.as_str(), i.evidence))
        .collect();
    assert_eq!(
        imports,
        [
            ("./a", Evidence::StaticAnalysis),
            ("react", Evidence::Syntax)
        ]
    );
    let calls: Vec<_> = report
        .calls
        .iter()
        .map(|e| (e.to.as_str(), e.evidence))
        .collect();
    assert_eq!(
        calls,
        [
            ("helper", Evidence::Inferred),
            ("missing", Evidence::Syntax)
        ]
    );

    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"evidence\":\"StaticAnalysis\""));
    let html = render_html(&report);
    for label in ["syntax", "static analysis", "inferred"] {
        assert!(html.contains(&format!("title=\"evidence class\">{label}</span>")));
    }
}

#[test]
fn member_calls_link_only_through_this_or_self() {
    let root = project(
        "receivers",
        &[(
            "a.ts",
            b"class K { log() {} push() {} run() { this.log(); } }\n\
              function main() { console.log(1); [].push(2); new K(); }\n" as &[u8],
        )],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let main = index
        .graph(Some("main"), Direction::Dependencies, 1)
        .unwrap()
        .edges;
    assert_eq!(
        links(&main),
        [
            ("main", "log", "Unresolved"),
            ("main", "push", "Unresolved"),
            ("main", "K", "Inferred"),
        ]
    );
    assert!(main[0].note.contains("receiver is not this/self"));
    let run = index
        .graph(Some("K.run"), Direction::Dependencies, 1)
        .unwrap()
        .edges;
    assert_eq!(links(&run), [("K.run", "K.log", "Inferred")]);
}

#[test]
fn commonjs_symbols_are_not_called_not_exported() {
    let root = project(
        "commonjs",
        &[(
            "x.js",
            b"function used() {}\nmodule.exports = { used };\n" as &[u8],
        )],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let explained = index.explain_function("used").unwrap();
    assert_eq!(explained[0].symbol.exported, None);
    assert!(explained[0]
        .summary
        .contains("`used` (function) is declared"));
}

#[test]
fn graph_says_when_it_was_cut_and_survives_cycles() {
    let mut source = String::from("function f() {}\nfunction main() {\n");
    source.push_str(&"f();\n".repeat(5001));
    source.push_str("}\nfunction a() { b(); }\nfunction b() { a(); }\n");
    let root = project("truncated", &[("big.ts", source.as_bytes())]);
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let cut = index
        .graph(Some("main"), Direction::Dependencies, 1)
        .unwrap();
    assert_eq!(cut.edges.len(), 5000);
    assert!(cut.truncated);

    let cycle = index.graph(Some("a"), Direction::Dependencies, 50).unwrap();
    assert_eq!(
        links(&cycle.edges),
        [("a", "b", "Inferred"), ("b", "a", "Inferred")]
    );
    assert!(!cycle.truncated);
}

#[test]
fn moved_file_gets_new_ids_and_links_are_recomputed() {
    let root = project(
        "moved",
        &[
            ("a.ts", b"export function helper() {}\n" as &[u8]),
            (
                "b.ts",
                b"import { helper } from './a';\nfunction main() { helper(); }\n",
            ),
        ],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let old_id = index.find("helper").unwrap()[0].id.clone();

    fs::create_dir_all(root.join("lib")).unwrap();
    fs::rename(root.join("a.ts"), root.join("lib/a.ts")).unwrap();
    let summary = index.scan().unwrap();
    assert_eq!(
        (summary.reparsed, summary.unchanged, summary.removed),
        (1, 1, 1)
    );
    let helper = &index.find("helper").unwrap()[0];
    assert_eq!(helper.path, "lib/a.ts");
    assert_ne!(helper.id, old_id);
    // `./a` no longer exists; the call still matches the one `helper` by name.
    assert_eq!(summary.imports_resolved, 0);
    let edges = index
        .graph(Some("main"), Direction::Dependencies, 1)
        .unwrap()
        .edges;
    assert_eq!(edges[0].to_id.as_deref(), Some(helper.id.as_str()));
}

#[test]
fn index_from_an_older_schema_is_rebuilt() {
    let root = project("schema", &[("a.ts", b"function a() {}\n" as &[u8])]);
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    drop(index);

    let db = rusqlite::Connection::open(root.join(".codeatlas/index.db")).unwrap();
    db.execute_batch("PRAGMA user_version = 1;").unwrap();
    drop(db);

    let mut index = Index::open(&root).unwrap();
    assert!(index.find("a").unwrap().is_empty());
    let summary = index.scan().unwrap();
    assert_eq!((summary.reparsed, summary.unchanged), (1, 0));
}

#[test]
fn html_report_escapes_project_content() {
    let root = project(
        "html",
        &[(
            "x.ts",
            b"export function x(a = \"<script>alert(1)</script>\") { x(); }\n" as &[u8],
        )],
    );
    let mut index = Index::open(&root).unwrap();
    index.scan().unwrap();
    let html = render_html(&index.report().unwrap());
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script"));
    let id = &index.find("x").unwrap()[0].id;
    assert!(html.contains(&format!("id=\"s-{id}\"")));
    assert!(html.contains(&format!("href=\"#s-{id}\"")));
}

#[test]
fn unreadable_and_unknown_inputs_are_reported_not_fatal() {
    let root = project(
        "bad-input",
        &[
            ("ok.py", b"def ok():\n    pass\n" as &[u8]),
            ("binary.ts", &[0xff, 0xfe, 0x00, 0x41]),
            ("notes.txt", b"hello"),
        ],
    );
    let mut index = Index::open(&root).unwrap();
    let summary = index.scan().unwrap();
    assert_eq!(summary.files, 1);
    assert_eq!(summary.unsupported, 1);
    assert_eq!(summary.warnings, ["binary.ts: not UTF-8"]);
    assert!(index
        .graph(Some("missing"), Direction::Dependencies, 1)
        .is_err());
    assert!(Index::open(&root.join("nope")).is_err());
}

#[test]
fn ignore_files_exclude_paths_and_bad_rules_are_reported() {
    let root = project(
        "ignore-rules",
        &[
            (".gitignore", b"generated/\n*.gen.ts\n" as &[u8]),
            (".codeatlasignore", b"vendor/\n!vendor/keep.ts\n"),
            ("src/main.ts", b"export function main() {}\n"),
            ("src/api.gen.ts", b"export function generatedApi() {}\n"),
            ("generated/out.ts", b"export function out() {}\n"),
            ("vendor/lib.ts", b"export function vendored() {}\n"),
            // Nested .gitignore applies below its own directory only.
            ("pkg/.gitignore", b"local.py\n"),
            ("pkg/local.py", b"def local():\n    pass\n"),
            ("pkg/kept.py", b"def kept():\n    pass\n"),
            ("local.py", b"def top_local():\n    pass\n"),
            // Re-including a fixed skip directory has no effect.
            ("node_modules/dep/index.ts", b"export function dep() {}\n"),
        ],
    );
    fs::write(
        root.join(".gitignore"),
        "generated/\n*.gen.ts\n!node_modules/\n",
    )
    .unwrap();
    let mut index = Index::open(&root).unwrap();
    let summary = index.scan().unwrap();
    assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);

    let mut files: Vec<_> = index
        .symbols(None, None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    files.sort();
    assert_eq!(files, ["kept", "main", "top_local"]);
    // Only the three ignore files themselves count as unsupported; ignored
    // files are not counted at all.
    assert_eq!(summary.unsupported, 3);

    // Un-ignoring a file and rescanning picks it up; ignoring one drops it.
    fs::write(root.join(".codeatlasignore"), "src/main.ts\n").unwrap();
    let summary = index.scan().unwrap();
    assert_eq!(summary.removed, 1);
    let mut names: Vec<_> = index
        .symbols(None, None)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    names.sort();
    assert_eq!(names, ["kept", "top_local", "vendored"]);

    // A malformed rule is skipped with a warning, not silently or fatally.
    fs::write(root.join(".codeatlasignore"), "src/{a\n").unwrap();
    let summary = index.scan().unwrap();
    assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);
    assert!(
        summary.warnings[0].contains(".codeatlasignore"),
        "{:?}",
        summary.warnings
    );
    assert!(!summary.warnings[0].contains(&*root.to_string_lossy()));
}
