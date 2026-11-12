# CodeAtlas repository audit — 2026-11-11

Scope: `main`, PRs #1, #2, #3, closed PR #4, CI, and the code on each branch.
This is a snapshot taken before integration. During the audit nothing was merged, closed, rebased, pushed or rewritten.

Follow-up: the owner approved the plan in section 5. PRs #1, #3 and #2 were then merged in that order (`main` at `414fdf0`, all six CI jobs green), actions were pinned to commit SHAs (S1), and `feat/milestone-1.1` was deleted. Issues C1–C10 and S2–S7 remain open.

Every statement below is tagged:

- **[V]** verified in this audit (I ran it or read it).
- **[A]** assumption or not verified.

## 1. Actual repository state

- [V] Local checkout: branch `claude/project-thread-arl648` at `9a241a0`, clean, in sync with origin.
- [V] `main` is at `5c3c448` (Milestone 0 only). Nothing from Milestone 1.1 is on `main`.
- [V] Remote branches:

| Branch | Head | PR | Base |
| --- | --- | --- | --- |
| `claude/project-thread-arl648` | `9a241a0` | #1 open, draft | `main` |
| `claude/project-thread-x61mxp` | `7a11884` | #2 open, draft | `main` |
| `claude/project-thread-ff5f30` | `4c851ba` | #3 open, draft | PR #1's branch |
| `feat/milestone-1.1` | `9167524` | #4 closed | `main` |

- [V] All three open PRs are **drafts**, have **zero reviews**, and GitHub reports each as mergeable against its own base.
- [V] No secrets found in any branch (pattern scan for keys, tokens, private keys). Commit author emails are GitHub no-reply or Anthropic addresses.
- [V] `Cargo.lock` contains no HTTP or TLS crate. The tool has no network code.

## 2. CI status (latest commit of each PR)

| PR | Commit | Jobs | Result |
| --- | --- | --- | --- |
| #1 | `9a241a0` | check on Linux, Windows, macOS | all pass [V] |
| #2 | `7a11884` | check ×3, msrv 1.90, docs, cargo-deny | all pass [V] |
| #3 | `4c851ba` | check ×3 | all pass [V] |

- [V] PR #2's first commit (`867cfc2`) failed the `cargo-deny` job. The second commit fixed it.
- [V] PR #1 and PR #3 have never been checked by MSRV, docs or cargo-deny in CI, because those jobs only exist in PR #2.

## 3. PR-by-PR assessment

### PR #1 — Milestone 1.1 (17 files, +2676 / −220)

What it contains [V]: new `atlas-index` crate (SQLite index, link resolution, queries, HTML report), JavaScript/Rust/Python adapters, clap CLI (`scan`, `symbols`, `graph`, `explain`, `report`), fixtures, ADR 0001.

- [V] The CSS heading fix is present: `html.rs` line 23 has `h2 code,h3 code{font-size:inherit}h3 .dim,h3 .tag{…}` (commit `9a241a0`).
- [V] 18 tests pass locally on Windows. Clippy is clean. `cargo fmt --check` could not run locally (rustfmt is not installed for the GNU toolchain); CI ran it and it passed.

What is solid [V]:

- A scan is one SQLite transaction. An interrupted scan leaves the old index intact.
- Deleted and renamed files are handled. I renamed a file and rescanned: `1 reparsed, 1 removed`, `PRAGMA integrity_check` = ok, no foreign-key violations, no orphaned call rows.
- Incremental scan reparses only changed files, then relinks everything (tested).
- All SQL is parameterised. No string-built queries from user input.
- HTML report has no scripts and no external resources. All project text goes through an escape function. Links are in-page anchors only. A test checks a `<script>` payload is escaped.
- Paths are stored relative with `/` separators. Symlinks are not followed. Results are sorted, so output does not depend on the OS.
- The parser walks the tree iteratively, so deeply nested input cannot overflow the stack (tested with 20,000 levels).
- Call links are never labelled stronger than `Inferred`. Limits are printed with every explanation.

Correctness issues [V unless marked]:

| # | Severity | Issue | Evidence |
| --- | --- | --- | --- |
| C1 | High | **Wrong "not exported" label.** `function foo() {}` + `export { foo };` is reported `exported: false`. Same for CommonJS `module.exports = { used }`. The tool states something false. | Probe: `foo exported= False`, `used exported= False`. Cause: `atlas-parser/src/lib.rs:432-446` only looks for `export` directly on the declaration. |
| C2 | High | **Member calls link to unrelated methods.** `console.log(1)` is linked as `Inferred` to a project method `K.log`; `[].push(2)` to `K.push`. The label is honest ("name match") but the link is wrong and will mislead in real projects. | Probe output: `main -> K.log [Inferred] … (only symbol with this name in the same file)`. Cause: `atlas-index/src/lib.rs:822-865` ignores the receiver. |
| C3 | Medium | **Rust calls inside macros are invisible.** `println!("{}", helper())` and `assert!(check())` produce zero call sites. Not listed in README or in the printed limitations. | Probe: 0 call sites for `m.rs`. |
| C4 | Medium | **`new K()` is not recorded** as a call in JS/TS. Not documented. | Probe: no edge for `new K()`. |
| C5 | Medium | **Rust `exported` over-claims.** It means "has a visibility keyword". `pub(crate)` counts as exported; a `pub fn` in a private module counts as exported; trait-impl methods count as not exported. | Read `lib.rs:447-450`. |
| C6 | Low | `graph` stops silently at 5000 edges. The output does not say it was cut. | `atlas-index/src/lib.rs:446`. |
| C7 | Low | Resolved imports carry no evidence label. They are guesses by file-path convention (no `tsconfig` paths, no `package.json` exports) but are shown as plain facts. | `html.rs:142-151`, `ImportRow`. |
| C8 | Low | One unreadable sub-directory aborts the whole scan (I/O errors are fatal). Unreadable files only warn. | `lib.rs:617-622`. |
| C9 | Low | A 64-bit hash collision between two symbol ids would abort a scan with a database error. Extremely unlikely; documented as a shortcut in code. [A: not reproduced] | `symbols.id TEXT PRIMARY KEY`. |
| C10 | Low | Every scan rewrites every call row, even if nothing changed. Fine now; will be slow on large repositories. [A: not measured] | `resolve()`. |

Architecture [V]: the plan lists 13 crates; the code has 4 (`atlas-domain`, `atlas-parser`, `atlas-index`, `atlas-cli`). ADR 0001 records this as a deliberate choice and says when to split. The dependency direction is correct (domain ← parser ← index ← cli). Two gaps against the engineering principles:

- There is no evidence type in `atlas-domain`. Call status is a free-text string in the database. Symbols and imports have no evidence class at all. The six required classes (syntax, static analysis, inferred, runtime, test, AI) do not exist in code.
- HTML rendering lives inside the engine crate. Acceptable today (ADR 0001), worth moving when a second renderer appears.

### PR #2 — CI hardening (8 files, +166 / −2)

- [V] Adds caching, least-privilege `permissions: contents: read`, PR concurrency cancel, MSRV job, docs job, cargo-deny job, and a tag-triggered release workflow.
- [V] The MSRV claim is true: `tree-sitter 0.27` and `tree-sitter-language` both declare `rust-version = "1.90"`.
- [V] Uses `pull_request`, not `pull_request_target`. No secrets are referenced. Forked PRs cannot reach a write token.
- [V] Shell steps read the tag from an environment variable, not from inline `${{ }}` text, so a hostile tag name cannot inject commands.

Concerns:

| # | Severity | Concern |
| --- | --- | --- |
| S1 | Medium | Actions are pinned to moving tags (`actions/checkout@v4`, `Swatinem/rust-cache@v2`, `EmbarkStudios/cargo-deny-action@v2`). Whoever controls those tags controls the CI run. Pin to commit SHAs. Matters most for the release workflow, which has a write token. |
| S2 | Medium | The release workflow has **never run** [V: stated in the PR, no tag exists]. Treat it as untested code. |
| S3 | Low | `release.yml` creates the draft release **before** tests run. A failed build leaves a half-filled draft to delete by hand. |
| S4 | Low | Checksums are produced on the same runner as the binary. They detect corruption, not tampering. No build attestation. |
| S5 | Low | `contents: write` is set for the whole release workflow rather than per job. |
| S6 | Low | CI uses unpinned `stable`. A new Rust release with new clippy lints can turn `main` red with no code change. This is a maintenance cost, not a bug. |
| S7 | Low | Linux binary is linked against the runner's glibc; it will not run on older distributions. No Intel macOS build. |

No compatibility problem found with PR #1: [V] I merged #1 + #3 + #2 in a scratch clone and the combined tree passes clippy, 19 tests and `cargo doc -D warnings`.

### PR #3 — `.gitignore` support (7 files, +278 / −42)

- [V] **Stacking is correct but stale.** Its base is PR #1's branch, but it forked from `cc245e0`. PR #1 has two newer commits (`430fabc`, `9a241a0`) that PR #3 does not contain. They merge cleanly (simulated).
- [V] Design matches ADR 0002: reads `.gitignore` and `.codeatlasignore` inside the project only; fixed skip list still wins; symlinks not followed; bad rules become warnings.
- [V] One new integration test covers nested ignore files, negation, the skip-list override, rescan after rule change, and a malformed rule.
- [V] Adds the `ignore` crate (+99 lock lines). It is the ripgrep rule engine; reasonable instead of hand-writing gitignore matching.
- [A] Its licences have **not** been checked by cargo-deny, and it has **not** been built on Rust 1.90. Both only happen once PR #2's jobs run against it.
- Behaviour change to know about: files git ignores are no longer indexed. Generated code a project depends on (for example a git-ignored API client) disappears from the index. This is the intended trade-off; `.codeatlasignore` negation (`!path`) brings a file back.

### PR #4 (closed) and duplicate work

- [V] `feat/milestone-1.1` is the same work as PR #1. The two trees differ in exactly two lines (a `Cargo.toml` version field and a rustdoc fix), both better in PR #1. Closing #4 was correct.
- [V] The branch `feat/milestone-1.1` still exists locally and on GitHub. It should be deleted after PR #1 merges.
- [V] No other duplicate or conflicting work. The only textual conflict is PR #1 vs PR #2 in `Cargo.toml` `[workspace.dependencies]`; resolution is "keep both sides".

## 4. Security and privacy summary

- [V] No network access, no telemetry, no AI provider. Source never leaves the machine.
- [V] The tool writes only inside `<project>/.codeatlas/`. It does not modify user source.
- [V] Signatures are stored and printed verbatim, including default values. A default like `token = "abc"` ends up in the index and in the HTML report. The README says so. Anyone sharing a report shares those strings.
- [V] The HTML report embeds the project path exactly as typed on the command line.
- [V] PR descriptions on the public repository contain the owner's local checkout path and claude.ai session links. Low risk; edit if unwanted.
- See S1–S5 for CI.

## 5. Merge dependencies and recommended order

Dependencies [V]: #3 needs #1. #2 is independent but conflicts with #1 in one file. After both #1 and #2 are on `main`, `crates/atlas-index/Cargo.toml` needs `rust-version.workspace = true`.

Recommended plan (each step needs owner approval):

1. **PR #1 → `main`.** Mark ready, merge with a merge commit. Known issues C1–C7 are tracked in the next milestone, not fixed in this PR.
2. **PR #3 → `main`.** First merge PR #1's branch into PR #3's branch (no rebase, no force-push) so it contains `430fabc` and `9a241a0`. Retarget to `main`, wait for CI, merge.
3. **PR #2 → `main`, last.** Merge `main` into its branch, resolve `Cargo.toml` by keeping both sides, add `rust-version.workspace = true` to `atlas-index`. Its CI is then the first run of MSRV, docs and cargo-deny against the full dependency set (`rusqlite`, `ignore`). Fix anything it finds, then merge. Pin actions to SHAs (S1) in the same PR.
4. **Delete `feat/milestone-1.1`** (local and remote).
5. Do not push a release tag until the release workflow has been tried once on a throwaway pre-release tag.

Why #2 last: one conflict resolution instead of two, and the strict checks run once against everything. The cost: `main` holds dependencies cargo-deny has not checked for a short time. If that is unacceptable, merge #2 first and resolve the conflict in #1 instead.

## 6. Tests actually executed in this audit

| What | Where | Result |
| --- | --- | --- |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PR #1 branch, Windows GNU toolchain | pass |
| `cargo test --workspace --locked` | PR #1 branch | 18 passed, 0 failed |
| Same clippy + tests | scratch merge of #1 + #3 + #2 | pass, 19 passed |
| `cargo doc --workspace --no-deps --locked` with `-D warnings` | scratch merge | pass |
| Manual probes with the built `atlas` binary: export forms, member calls, `new`, Rust macros, CommonJS, file rename, SQLite integrity | throwaway project | results in section 3 |
| `git merge-tree` conflict simulation for #1+#2 and #1+#3 | local | one conflict (`Cargo.toml`), #1+#3 clean |

Not executed: `cargo fmt --check` (rustfmt missing locally), cargo-deny (not installed), MSRV 1.90 build (toolchain not installed), anything on Linux or macOS (CI only), the release workflow.

## 7. Tests still missing

- Export detection: `export { a }`, `export default a`, re-exports, CommonJS.
- Member calls on built-ins (`console.log`, `array.push`) must not link to project methods.
- Rust calls inside macros; `new X()`; generic calls `f::<T>()`.
- File rename and file move as automated tests (only probed by hand).
- Schema version change drops and rebuilds the index.
- `graph` truncation at 5000 edges.
- Cycles in `graph --depth N`.
- CLI end-to-end: exit codes, `--json` shape, missing index message.
- Non-UTF-8 file names, CRLF files, BOM, very large files.
- Two scans at the same time on one index.
- HTML: validity, keyboard navigation, contrast, screen-reader structure.
- Unreadable directory (see C8).

## 8. Recommended next milestone

**Milestone 1.3 — Evidence model and label correctness.** No new features. Make what exists true.

1. Add an evidence class to `atlas-domain` and attach it to every symbol, import and call in JSON and HTML. Symbols: syntax. Resolved imports: static analysis. Call links: inferred or unresolved.
2. Fix C1: recognise `export { a }` and `export default a`. When the syntax cannot tell (CommonJS), report "unknown", never "not exported".
3. Fix C5: rename the Rust field's meaning to "declared `pub`" in output text.
4. Fix C2: **owner decision needed.** Option A (recommended): link `x.f()` only when the receiver is `this` or `self`; otherwise `Unresolved` with the candidate count. Option B: keep linking but add a `low` confidence level.
5. Fix C4 (record `new X()`), document C3 in README and in printed limitations.
6. Fix C6: say when output was truncated.
7. Add the tests from section 7 that cover items 1–6, plus rename and schema-version tests.

Done when: every item has a failing-then-passing test, CI is green on three platforms plus MSRV/docs/deny, README matches behaviour.

After 1.3, the strongest candidate is Git change-impact analysis (`atlas-git`), because "Trust every change" is the product promise with no code behind it yet.
