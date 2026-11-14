# Changelog

## 0.5.0

### New: desktop app

- Tracemark for Windows, macOS (Apple Silicon) and Linux. The installers are attached to the release as `tracemark-desktop-v0.5.0-*`, each with a SHA-256 file and signed build provenance.
- **Explore:** open a project folder, search symbols, and see each symbol's signature, calls and callers with evidence labels. Click a name to follow the chain.
- **Review a change:** the same rule-based review as `tracemark review`, with the recommendation, findings and call sites.
- The app runs the same engine locally. Its only permission beyond its own commands is the folder picker, and a strict content-security policy applies.
- The installers are not code-signed yet. The README explains the Windows and macOS warnings.

### Docs

- New [quick guide](docs/guide.md) for first-time users.

## 0.4.0

### New: connector for AI coding agents (MCP)

- `tracemark mcp --project <path>` serves the project to AI coding agents over the Model Context Protocol (JSON-RPC on stdin and stdout). It works with Claude Code, Claude Desktop, Cursor and other MCP clients.
- Six tools: `tracemark_symbols`, `tracemark_graph`, `tracemark_explain`, `tracemark_changes`, `tracemark_review`, `tracemark_scan`. Query tools rescan changed files first, so answers include the agent's latest edits.
- Results carry evidence classes. The server's instructions tell agents to treat `Inferred` links as leads to verify.
- The server answers only for the project it was started with, makes no network calls, and cuts results over 500 items with `truncated: true`.
- Supported protocol versions: 2025-06-18, 2025-03-26, 2024-11-05.
- The GitHub Action now downloads 0.4.0 by default.

## 0.3.0

### New: review a change before approving it

- `tracemark review --base <commit>` applies six fixed rules to the change and recommends one of: request changes, review carefully, approve after checks, or no rule flagged a risk. The rules are `removed-still-called`, `syntax-errors-added`, `signature-callers-untouched`, `wide-impact`, `no-test-calls` and `not-analysed`.
- Each finding names its rule, severity, file, line and evidence class, and lists the call sites behind it.
- `--markdown` prints a pull-request comment in which project names cannot add links, mentions or HTML. `--json` prints everything. `--fail-on` exits with code 3 for CI.
- New GitHub Action `Gabions256/tracemark@v0.3.0` posts the review on every pull request and updates the same comment on each push.
- `changes` JSON now includes `syntax_errors` and `syntax_errors_before` per file.

Still structure only. Nothing is run, built or tested, and no AI is involved.

## 0.2.0

CodeAtlas is now **Tracemark**. It traces every connection and marks each result with its evidence. The name changed because other code-intelligence projects already publish as "CodeAtlas", and because the `atlas` command clashed with an unrelated Homebrew tool. Behaviour is unchanged.

### Breaking: what to change

- The command is `tracemark` instead of `atlas`.
- The index lives in `<project>/.tracemark/` instead of `.codeatlas/`. The first `tracemark scan` builds a new index. The old `.codeatlas/` folder can be deleted.
- The project ignore file is `.tracemarkignore` instead of `.codeatlasignore` (same syntax). Rename the file if you have one.
- Crates are `tracemark-domain`, `tracemark-parser`, `tracemark-index` and `tracemark-cli`.
- Release archives are named `tracemark-v<version>-<target>`.
- The repository is `github.com/Gabions256/tracemark`. GitHub redirects the old address.
- Homebrew: `brew install gabions256/tap/tracemark`. An existing `codeatlas` install is migrated by `brew upgrade`.

## 0.1.1

### Fixed

- Two symbols whose 64-bit ids collide no longer abort the scan. The second one is left out of the index and the scan prints a warning naming it.
- `cargo binstall` metadata names the archive suffix exactly (`.tar.gz`, `.zip`).

### Release process

- The Intel macOS binary is now run in the release smoke test, under Rosetta. Before, it was built but never executed.

### Measured

On Windows, a generated TypeScript project of 1,500 files (15,000 functions, 45,000 call sites): full index build 1.9 s, rescan with nothing changed 0.4 s, rescan after one edit 0.45 s, `graph dependents --depth 3` 77 ms, index 10 MB. The very first read of freshly written files took 21 s; that is operating-system and antivirus cost, not the engine.

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
- Binaries are not code-signed. Each archive has a SHA-256 file and signed build provenance (`gh attestation verify`).
