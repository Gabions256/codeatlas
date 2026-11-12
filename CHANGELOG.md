# Changelog

## 0.1.0

First public release. A local, offline command-line tool: no AI, no network access, no telemetry.

### Commands

- `atlas scan`: index a project into `<project>/.codeatlas/index.db`. Rescans reparse only changed files. Honours `.gitignore` and `.codeatlasignore` inside the project.
- `atlas symbols list`: functions, methods, classes, structs, interfaces, traits, modules, enums and type aliases, with signatures and parameters.
- `atlas graph dependencies` / `dependents`: call relationships, followed `--depth` hops.
- `atlas explain function`: structure of one function from indexed evidence. Not a description of behaviour.
- `atlas report --format html|json`: a self-contained report with no scripts and no external resources.
- `atlas changes --base <commit>`: symbols added, removed or changed since a git commit, and the callers that may be affected.
- `--json` on every command, `--project <path>` to query from elsewhere.

### Languages

TypeScript, TSX, JavaScript, JSX, Rust and Python. Syntax only (Tree-sitter): nothing is type-checked or run.

### Evidence

Every symbol, import and call is reported with its source position and an evidence class: `Syntax` (written in the source), `StaticAnalysis` (an import matched to a file by path) or `Inferred` (a call matched to a symbol by name). Nothing is reported as observed at runtime.

### Privacy

- Source code never leaves the machine.
- String default values in signatures are stored and shown as `"<redacted>"`.
- The index folder ignores itself in git.

### Known limits

- Calls are linked by name. `x.f()` is linked only when `x` is `this` or `self`.
- Rust calls inside macros are not seen. Rust and Python imports are not resolved. Python `K()` is not linked to class `K`.
- Symbol ids change when a symbol or its file is renamed.
- A folder or file that cannot be read is skipped, and the scan says the index is incomplete.
- `atlas changes` needs `git`, compares the working tree with one commit, and shows a renamed symbol as removed plus added.
- Binaries are not code-signed.
