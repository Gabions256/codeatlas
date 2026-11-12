# CodeAtlas

Open-source code intelligence for understanding, exploring and verifying software built by humans and AI agents.

Status: Milestone 0. One language adapter (TypeScript / TSX), syntax-level only.

## Use

    cargo run -p atlas-cli -- scan fixtures/typescript/sample

Prints a JSON report to stdout: per file, the declared symbols (functions, methods, classes, interfaces, enums, type aliases), imports, and parser diagnostics, each with a source range. Warnings go to stderr.

## What the report does and does not claim

- A symbol means a declaration exists at that range. It does not mean the code runs or works.
- Imports are the specifier as written. They are not resolved to files.
- No call relationships, no graph, no explanations yet.
- `.gitignore` is not read. `.git`, `node_modules`, `target`, `dist` and `build` are skipped.

## Crates

- `atlas-domain`: report and symbol types.
- `atlas-parser`: Tree-sitter integration and language adapters.
- `atlas-cli`: the `atlas` binary.

## Develop

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

A C compiler is required to build the Tree-sitter grammars.

Licence: Apache-2.0.
