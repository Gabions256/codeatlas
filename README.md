# CodeAtlas

Open-source code intelligence for understanding, exploring and verifying software built by humans and AI agents.

Status: Milestone 1.2. A local engine that indexes a project, links calls and imports, and answers questions from that index. No AI, no network, no desktop app yet.

## Use

    atlas scan ./my-project
    cd my-project
    atlas symbols list
    atlas graph dependencies
    atlas graph dependents handleCheckout --depth 2
    atlas explain function handleCheckout
    atlas report --format html > report.html

From a checkout, replace `atlas` with `cargo run -p atlas-cli --`. Add `--json` to any command for machine-readable output, and `--project <path>` to query a project from elsewhere.

`scan` honours `.gitignore` files and a `.codeatlasignore` file (same syntax) anywhere inside the project. Ignore files above the project root, global git excludes and `.git/info/exclude` are not read, so the same tree indexes the same way on every machine. `.git`, `.codeatlas`, `node_modules`, `target`, `dist`, `build`, `__pycache__`, `.venv` and `venv` are always skipped, even if an ignore file re-includes them. A malformed ignore rule is reported as a warning and not applied.

`scan` writes its index to `<project>/.codeatlas/index.db`. That folder ignores itself in git. A rescan reparses only files whose content changed. Nothing else in the project is touched.

## What each language adapter can establish

All adapters are syntax-only (Tree-sitter). None type-checks or runs code.

| Language | Extensions | Symbols, signatures, parameters, call sites | Export detection | Import resolution |
| --- | --- | --- | --- | --- |
| TypeScript | ts, tsx, mts, cts | yes | `export` | relative specifiers to project files |
| JavaScript | js, jsx, mjs, cjs | yes | `export` | relative specifiers to project files |
| Rust | rs | yes | `pub` | none |
| Python | py | yes | none | none |

## What the output does and does not claim

- A symbol means a declaration exists at that source range. It does not mean the code runs or works.
- Every call site and import is reported with its file, line and column. That position is the evidence.
- A call is `Inferred` when exactly one project function or method has that name (same file first, then the whole project). It is a name match. It was not type-checked or observed.
- A call is `Unresolved` when no name or several names match. The note says which. Library calls, built-ins and dynamic calls land here.
- `f()` is matched to functions, `x.f()` to methods, and Rust `a::f()` to either.
- `explain` describes structure: where a function is, its signature, what it calls, what calls it. It does not say what the code does.
- Symbol ids are a hash of file path, qualified name and kind. They survive edits and moves within a file. They change on rename or file move.

## Not done yet

- No Rust or Python import resolution. No `require()`, re-exports or path aliases.
- No type references, inheritance, or data flow.
- No documentation comments.
- No secret redaction: signatures are copied verbatim, including default values.
- No AI explanations, MCP server, desktop app, Git change analysis or impact analysis.

## Crates

- `atlas-domain`: shared types. No internal dependencies.
- `atlas-parser`: Tree-sitter integration and language adapters.
- `atlas-index`: the shared engine. Discovery, incremental SQLite index, link resolution, queries, reports.
- `atlas-cli`: the `atlas` binary. Argument parsing and printing only.

Design decisions are in [docs/adr](docs/adr).

## Develop

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

A C compiler is required: the Tree-sitter grammars and SQLite are built from source. On Windows with a long checkout path, set `CARGO_TARGET_DIR` to a short path.

Licence: Apache-2.0.
