# CodeAtlas

Open-source code intelligence for understanding, exploring and verifying software built by humans and AI agents.

Status: Milestone 1.3. A local engine that indexes a project, links calls and imports, and answers questions from that index. No AI, no network, no desktop app yet.

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
| TypeScript | ts, tsx, mts, cts | yes | `export`, `export { a }`, `export default a` | relative specifiers to project files |
| JavaScript | js, jsx, mjs, cjs | yes | same; unknown in files with no `import` or `export` | relative specifiers to project files |
| Rust | rs | yes | a `pub` modifier is present (not reachability) | none |
| Python | py | yes | none | none |

## What the output does and does not claim

- A symbol means a declaration exists at that source range. It does not mean the code runs or works.
- Every call site and import is reported with its file, line and column. That position is the evidence.
- A call is `Inferred` when exactly one project function or method has that name (same file first, then the whole project). It is a name match. It was not type-checked or observed.
- A call is `Unresolved` when no name or several names match. The note says which. Library calls, built-ins and dynamic calls land here.
- `f()` is matched to functions, `new X()` to classes, and Rust `a::f()` to functions or methods.
- `this.f()` and `self.f()` are matched to methods. Any other `x.f()` is never linked, because the type of `x` is unknown: `console.log()` must not link to a project method called `log`. The note gives the number of methods with that name.
- Every symbol, import and call carries an evidence class in JSON (`evidence`) and in the HTML report:
  - `Syntax`: written in the source at that position. All symbols, unresolved imports, unresolved calls.
  - `StaticAnalysis`: an import matched to a project file by path convention. `tsconfig` paths and `package.json` exports are not read.
  - `Inferred`: a call matched to a symbol by name.
  - `RuntimeObserved`, `TestResult` and `AiSuggestion` exist as classes but nothing produces them yet.
- "Exported" in a script file means an `export` names the symbol. A file with no `import` or `export` may be CommonJS, so its symbols are reported as unknown, not as "not exported".
- For Rust the flag only means a `pub` modifier is present, including `pub(crate)`. It does not mean the item is reachable from outside the crate.
- `graph` returns at most 5000 edges. When more exist, the output says so (`"truncated": true` in JSON).
- `explain` describes structure: where a function is, its signature, what it calls, what calls it. It does not say what the code does.
- Symbol ids are a hash of file path, qualified name and kind. They survive edits and moves within a file. They change on rename or file move.

## Not done yet

- Rust calls written inside macros (`println!("{}", f())`, `assert!(g())`) are not seen at all: macro arguments are not parsed as code.
- Python `K()` is not linked to class `K`. Generic Rust calls `f::<T>()` and computed callees are not recorded.
- No Rust or Python import resolution. No `require()`, CommonJS exports, re-exports or path aliases.
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

Minimum supported Rust version (MSRV): 1.90, set by `rust-version` in `Cargo.toml`. CI checks it alongside stable on Linux, Windows and macOS, builds the docs, and runs `cargo deny check` (advisories, licences, bans, sources; config in `deny.toml`).

## Release

Bump `version` under `[workspace.package]` in `Cargo.toml`, merge, then push a matching tag (`git tag v0.2.0 && git push origin v0.2.0`). The release workflow creates a draft GitHub release with `atlas` binaries for Linux x86_64, Windows x86_64 and macOS arm64, plus SHA-256 checksums. Review and publish the draft by hand.

Licence: Apache-2.0.
