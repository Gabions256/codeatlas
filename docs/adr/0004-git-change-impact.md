# ADR 0004: Git change impact through the git program

Status: accepted (Milestone 1.4)

## Context

"Trust every change" is part of the product promise, and code from AI agents usually arrives as a set of uncommitted edits. Users need to see which declarations an edit touched and which callers might be affected, without the tool claiming the change is safe.

## Decision

1. `atlas changes --base <ref> --depth N` compares the working tree with one commit.
2. Git is read by running the installed `git` program, not through a Rust git library.
3. The code is a module of `atlas-index` (`changes.rs`), not a new `atlas-git` crate.
4. Changed symbols are found by parsing both versions of each changed file and comparing symbols keyed by qualified name, kind and occurrence. A different signature or parameter list is `SignatureChanged`; any other text difference is `BodyChanged`. These are `Syntax` evidence.
5. Affected callers come from the index, rescanned first. Changed symbols use the linked call graph (`Inferred`). Removed symbols use call sites with the same name and a matching call form and language.
6. Git safety: arguments are passed as a list, never through a shell. `--base` may not start with `-`, and is passed after `--end-of-options`. Only `rev-parse`, `diff`, `ls-files` and `cat-file` are run, with `core.fsmonitor=false`, `--no-ext-diff`, `--no-textconv`, `GIT_OPTIONAL_LOCKS=0` and `GIT_TERMINAL_PROMPT=0`.

## Why

- The `git` program: no new dependency, and it reads every repository format and setting git itself supports. The `gix` crate would remove the need for git to be installed, but adds a large dependency tree to audit. Developers who want change impact have git.
- A module, not a crate: there is one consumer (the CLI) and the code needs the index's private call-graph walk. ADR 0001's rule applies: split when a second consumer appears.
- Comparing parsed symbols instead of diff hunks: the result is per declaration, the unit the rest of CodeAtlas reports on, and a moved function is not reported as changed.

## Consequences

- `atlas changes` fails with a clear error when git is missing or the project is not in a repository.
- It writes only the index under `.codeatlas/`, as `scan` does.
- Renaming a symbol shows as removed plus added. A method change also marks its class as body changed.

## When to revisit

- Comparing two commits: add `--head <ref>` and read both sides with `cat-file`.
- If git cannot be assumed (desktop app bundles), reconsider `gix`.
- Rename detection inside a file: match removed and added symbols with near-identical bodies.
