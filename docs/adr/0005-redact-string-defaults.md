# ADR 0005: Redact string default values in the parser

Status: accepted (Milestone 1.5)

## Context

Signatures and parameters were copied from the source verbatim. A default
value such as `api_key="sk-live-..."` ended up in `.codeatlas/index.db`, in
JSON output, in `explain` and in the HTML report, which people share.

## Decision

The parser replaces every string literal inside a parameter list with
`"<redacted>"` before the symbol leaves the adapter. Literals inside type
annotations (`m: "on" | "off"`) are kept, because they are types, not
values. Numbers, booleans, `None`/`null` and identifiers are kept.

When the parameter list has a syntax error, the tree cannot be trusted to
find the string, so everything from the first quote to the end of the list
is redacted.

The index schema went to version 3, so indexes written by older versions
are rebuilt. The database runs with `secure_delete` so removed rows are
overwritten, not left in free pages.

## Why

- Redacting in the parser means no consumer (CLI, HTML, a future MCP
  server) can forget to do it.
- String literals are where keys, tokens and passwords are written.
  Keeping numbers keeps signatures like `tip=0` useful.
- A heuristic ("looks like a secret") was rejected: it would miss some
  secrets, and the tool must not claim a safety it does not have.

## Limits

- A secret in a numeric default, or built from identifiers, is shown.
- Function bodies are not stored, so secrets there are not in the index.
  Call names and import specifiers are stored verbatim.

## Revisit when

- The index starts storing bodies, constants or comments.
- Users need the real default value; then add an explicit opt-in flag.

## Interaction with `atlas changes`

Two versions whose string default differs have the same redacted
signature. So that this still shows as "signature changed" and not as
"body changed", each symbol carries a hash of its unredacted declaration
text. The hash is kept in memory only: it is never serialized or written
to the index.
