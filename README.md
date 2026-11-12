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

Minimum supported Rust version (MSRV): 1.90, set by `rust-version` in `Cargo.toml`. CI checks it alongside stable on Linux, Windows and macOS, builds the docs, and runs `cargo deny check` (advisories, licences, bans, sources; config in `deny.toml`).

## Release

Bump `version` under `[workspace.package]` in `Cargo.toml`, merge, then push a matching tag (`git tag v0.2.0 && git push origin v0.2.0`). The release workflow creates a draft GitHub release with `atlas` binaries for Linux x86_64, Windows x86_64 and macOS arm64, plus SHA-256 checksums. Review and publish the draft by hand.

Licence: Apache-2.0.
