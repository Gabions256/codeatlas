# ADR 0002: Honour ignore files with the `ignore` crate

Status: accepted (Milestone 1.2)

## Context

Discovery walked every directory except a fixed skip list. Generated code, vendored code and anything else a project keeps out of git was indexed, inflating symbol counts and creating false name-match links.

## Decision

1. Discovery uses the `ignore` crate's walker, the same rule engine ripgrep uses, rather than a hand-written gitignore matcher.
2. It reads `.gitignore` files and `.codeatlasignore` files (same syntax) inside the project root only. It does not read ignore files above the root, the global git excludes file, `.git/info/exclude` or `.ignore` files. The index of a tree then depends only on that tree, not on the machine.
3. The fixed skip list stays and wins over ignore files, so `!node_modules/` cannot pull a dependency tree into the index.
4. Symlinks are still never followed.
5. A malformed ignore rule is not applied and is reported as a scan warning. I/O errors during the walk stay fatal, as before.

## Why

- Gitignore semantics (anchoring, negation, nested files, directory-only patterns) are easy to get subtly wrong. The `ignore` crate is widely used and maintained, and its licence (MIT or Unlicense) is compatible.
- Reading machine-level excludes would make two clones of the same commit index differently.

## When to revisit

- If users need `.git/info/exclude` or parent ignore files, add an explicit opt-in flag rather than changing the default.
