# ADR 0003: Evidence classes on every reported row

Status: accepted (Milestone 1.3)

## Context

Call links had a free-text status (`Inferred` or `Unresolved`). Symbols and imports had no label at all. A resolved import looked as certain as a declaration, although it is only a file-path match. The project rule is that every claim states how it was established.

An audit also found claims that were wrong rather than unlabelled: `export { a }` reported as "not exported", and `console.log()` linked to a project method named `log`.

## Decision

1. `atlas-domain` defines `Evidence`: `Syntax`, `StaticAnalysis`, `Inferred`, `RuntimeObserved`, `TestResult`, `AiSuggestion`. The last three are defined so the vocabulary is fixed, but nothing produces them yet.
2. Every symbol, import and call edge in the JSON and HTML output has an `evidence` field. It names the class of the strongest claim in that row:
   - symbol: `Syntax`;
   - import: `StaticAnalysis` when matched to a project file, otherwise `Syntax`;
   - call: `Inferred` when linked to a symbol, otherwise `Syntax` (only the call site is claimed).
3. The class is derived when a row is read. It is not stored in the database, because today it follows from the row's other columns.
4. A member call `x.f()` is linked only when the receiver is `this` (scripts) or `self` (Rust, Python). Other member calls stay `Unresolved`, with the number of same-named methods in the note.
5. When syntax cannot answer, the answer is unknown (`null`), never "no". Script files with no `import` or `export` report `exported: null`.
6. A graph query that hits the edge limit returns `truncated: true`.
7. The schema version goes to 2. Old indexes are dropped and rebuilt on the next scan, as ADR 0001 allows.

## Why

- One field per row is the smallest change that makes every claim carry its class.
- Option considered for member calls: keep linking by name with a "low confidence" level. Rejected by the project owner: a wrong link that looks like a link misleads more than a missing one, and the note still says how many candidates exist.
- Python's `self` is a naming convention, not a keyword. A method whose first parameter has another name is not recognised. Accepted as a documented limit.

## Consequences

- Fewer call links: `cart.add()` no longer links to `CartService.add`.
- `atlas graph --json` now returns an object (`edges`, `truncated`) instead of an array.

## When to revisit

- Store evidence in the database once a row can have more than one source, for example a call seen by static analysis and also observed at runtime.
- Link `x.f()` again when the type of `x` can be established (type inference or a language server).
