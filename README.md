# Tracemark

Open-source code intelligence for understanding, exploring and verifying software built by humans and AI agents.

Status: Milestone 1.5, first public release (0.1.0). A local engine that indexes a project, links calls and imports, and answers questions from that index. No AI, no network, no desktop app yet.

## Install

Prebuilt binaries for Linux (x86_64), Windows (x86_64) and macOS (Apple Silicon and Intel) are attached to each [GitHub release](https://github.com/Gabions256/tracemark/releases). Download the archive, unpack it and put `tracemark` on your `PATH`.

    # macOS and Linux, with Homebrew
    brew install gabions256/tap/tracemark

    # Any platform with Rust: downloads the release binary
    cargo binstall --git https://github.com/Gabions256/tracemark tracemark-cli

    # Any platform with Rust: builds from source (needs a C compiler)
    cargo install --git https://github.com/Gabions256/tracemark tracemark-cli

Each archive has a `.sha256` file next to it. Check it before running the binary, for example `sha256sum -c tracemark-v0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256`.

Each archive also has signed build provenance. With the GitHub CLI, `gh attestation verify <archive> -R Gabions256/tracemark` confirms it was built by this repository's release workflow.

The Linux binary is statically linked (musl), so it runs on any x86_64 distribution.

The binaries are not code-signed. Windows SmartScreen and macOS Gatekeeper may warn the first time you run `tracemark`. On macOS, `xattr -d com.apple.quarantine ./tracemark` removes the warning for a binary you downloaded and trust.


## Use

    tracemark scan ./my-project
    cd my-project
    tracemark symbols list
    tracemark graph dependencies
    tracemark graph dependents handleCheckout --depth 2
    tracemark explain function handleCheckout
    tracemark report --format html > report.html
    tracemark changes --base HEAD --depth 2
    tracemark review --base main
    tracemark mcp --project .          # for AI coding agents, see below

From a checkout, replace `tracemark` with `cargo run -p tracemark-cli --`. Add `--json` to any command for machine-readable output, and `--project <path>` to query a project from elsewhere.

`scan` honours `.gitignore` files and a `.tracemarkignore` file (same syntax) anywhere inside the project. Ignore files above the project root, global git excludes and `.git/info/exclude` are not read, so the same tree indexes the same way on every machine. `.git`, `.tracemark`, `node_modules`, `target`, `dist`, `build`, `__pycache__`, `.venv` and `venv` are always skipped, even if an ignore file re-includes them. A malformed ignore rule is reported as a warning and not applied.

`scan` writes its index to `<project>/.tracemark/index.db`. That folder ignores itself in git. A rescan reparses only files whose content changed. Nothing else in the project is touched.

A folder or file that cannot be read (for example, no permission) is skipped with a warning, and the summary says the index is incomplete (`skipped` in JSON). An unreadable project root is an error.

String default values in signatures are replaced with `"<redacted>"`, before anything is stored, so an API key written as `def f(key="sk-...")` does not reach the index or the reports. Literal types (`m: "on" | "off"`) and other defaults (`tip=0`, `None`) are shown as written. A secret in a numeric default is not hidden. Call names and import paths are stored as written.

## What changed, and what it may affect

`tracemark changes` compares the working tree with a git commit (`--base`, default `HEAD`). It needs `git` installed and the project inside a git repository. It rescans the index first.

For each changed file it reports the symbols that were added, removed, had their signature changed, or only their body changed. These come from comparing the source text of the two versions: evidence `Syntax`. New untracked files count as changed; git-ignored files do not.

For a changed symbol, `may affect` lists the callers linked to it, followed `--depth` hops: evidence `Inferred`, because the links are name matches. For a removed symbol, it lists the call sites that still use its name. Files no adapter handles are listed as "changed, not analysed".

It does not say whether a change works, is correct, or is tested. Other limits:

- Callers written as `x.f()` (other than `this`/`self`) are not found, as for `graph`.
- A symbol renamed inside a file shows as one removed and one added.
- A changed method also shows its class as body changed.
- Only the working tree against one commit. Two commits cannot be compared with each other yet.

git is run read-only (`rev-parse`, `diff`, `ls-files`, `cat-file`), never through a shell. A `--base` starting with `-` is rejected. The fsmonitor setting is switched off for these calls, so repository settings cannot start another program, and git's own index file is not rewritten.

## Review a change before you approve it

`tracemark review --base <commit>` runs `changes`, then applies fixed rules and gives a recommendation. It is built for code from AI agents, and for any change too large to read line by line.

| Rule | Severity | Flags |
| --- | --- | --- |
| `removed-still-called` | blocker | A removed function, method or class is still called, and the call now links to nothing. |
| `syntax-errors-added` | high | A changed file has more parser errors than before. |
| `signature-callers-untouched` | high | A signature changed and some callers are in files the change did not touch. |
| `wide-impact` | medium | A changed function has 10 or more callers within `--depth`. |
| `no-test-calls` | info | A new or changed function outside test code is not called by name from any test. |
| `not-analysed` | info | A changed file Tracemark cannot read. |

The recommendation follows from the most severe finding:

| Most severe finding | Recommendation |
| --- | --- |
| blocker | Request changes |
| high | Review carefully before approving |
| medium | Approve after checking the listed items |
| info, or none | No rule flagged a risk |

Every finding lists the call sites behind it, each with its evidence class.

It is structure only. Tracemark does not run, build or test the code, and no AI is involved. "No rule flagged a risk" is not a statement that the change is correct, and the output says so every time.

- `--markdown` prints a pull-request comment, `--json` the full data.
- `--fail-on blocker|high|medium` exits with code 3 when such a finding exists, for CI.

### On every pull request

Add `.github/workflows/tracemark.yml` to any repository:

    on: pull_request
    permissions:
      contents: read
      pull-requests: write
    jobs:
      review:
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4
            with:
              fetch-depth: 2
          - uses: Gabions256/tracemark@v0.4.0
            with:
              fail-on: blocker

The action downloads the release binary and checks its SHA-256. It posts one comment and updates that comment on each push, and writes the review to the job summary. Pull requests from forks get a read-only token, so they get the job summary but no comment. Linux, macOS and Windows runners work.

## Use from AI coding agents (MCP)

`tracemark mcp --project <path>` runs Tracemark as an [MCP](https://modelcontextprotocol.io) server on stdin and stdout. AI coding agents can then ask it for facts about the project instead of guessing, and check their own edits before calling them done.

Claude Code:

    claude mcp add tracemark -- tracemark mcp --project /path/to/project

Claude Desktop, Cursor and other clients that read an `mcpServers` config:

    {
      "mcpServers": {
        "tracemark": { "command": "tracemark", "args": ["mcp", "--project", "/path/to/project"] }
      }
    }

| Tool | Answers |
| --- | --- |
| `tracemark_symbols` | Where symbols are, with signatures. |
| `tracemark_graph` | What a symbol calls, or what calls it, with file:line:column. |
| `tracemark_explain` | The structure of one function and the limits of that evidence. |
| `tracemark_changes` | What changed since a commit, and what it may affect. |
| `tracemark_review` | Rule-based findings and a recommendation for the change. |
| `tracemark_scan` | Re-index and report counts and warnings. |

- **Fresh answers:** every query tool rescans changed files first, so answers include the agent's own latest edits.
- **Evidence labels:** results carry the same evidence classes as the CLI. The server's instructions tell the agent to treat `Inferred` links as leads to verify.
- **Scope and privacy:** the server answers only for the project given at start, and it cannot be pointed at other folders. It makes no network calls, and nothing it sees leaves the machine (the agent itself may send its results to its own AI provider).
- **Large results:** results over 500 items are cut, and say so (`truncated`).

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
- Python `K()` is not linked to class `K`: it stays `Unresolved`. Generic Rust calls `f::<T>()` and computed callees are not recorded.
- No Rust or Python import resolution. No `require()`, CommonJS exports, re-exports or path aliases.
- No type references, inheritance, or data flow.
- No documentation comments.
- No winget or crates.io package yet ([packaging](packaging/README.md) says why). Binaries are not code-signed, and there is no Linux ARM or Windows ARM build.
- No AI explanations, MCP server or desktop app.
- No test coverage: nothing says whether changed code is tested.

## Crates

- `tracemark-domain`: shared types. No internal dependencies.
- `tracemark-parser`: Tree-sitter integration and language adapters.
- `tracemark-index`: the shared engine. Discovery, incremental SQLite index, link resolution, queries, reports.
- `tracemark-cli`: the `tracemark` binary. Argument parsing and printing only.

Design decisions are in [docs/adr](docs/adr).

## Develop

    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked

A C compiler is required: the Tree-sitter grammars and SQLite are built from source. On Windows with a long checkout path, set `CARGO_TARGET_DIR` to a short path.

Minimum supported Rust version (MSRV): 1.90, set by `rust-version` in `Cargo.toml`. CI checks it alongside stable on Linux, Windows and macOS, builds the docs, and runs `cargo deny check` (advisories, licences, bans, sources; config in `deny.toml`).

## Release

1. Bump `version` under `[workspace.package]` in `Cargo.toml` and the three internal versions under `[workspace.dependencies]`, and add a `CHANGELOG.md` entry. Merge.
2. Push a matching tag: `git tag v0.2.0 && git push origin v0.2.0`. A version with a suffix (`0.2.0-rc.1`) becomes a pre-release.
3. The release workflow tests, builds and smoke-tests `tracemark` for Linux x86_64 (static), Windows x86_64, macOS arm64 and macOS x86_64. Only when all four pass does it create a **draft** release with the archives and SHA-256 files.
4. Review the draft on GitHub and publish it by hand.

Pull requests that change `release.yml`, `Cargo.toml` or `Cargo.lock` run the same build as a dry run, without publishing.

Licence: Apache-2.0.
