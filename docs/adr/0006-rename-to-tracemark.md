# ADR 0006: Rename CodeAtlas to Tracemark

Status: accepted (0.2.0)

## Context

After 0.1.1, a check of package registries found that the name was not free for this product:

- On crates.io, `codeatlas` belongs to another Rust "code intelligence and knowledge graph" CLI. `codeatlas-cli` and `codeatlas-domain` belong to a third project, an MCP server also called CodeAtlas. `atlas-cli` belongs to an unrelated project.
- The command `atlas` clashed with the Homebrew `atlas` formula (a database schema tool).

Two other code-analysis tools with the same name would confuse users and block publishing to crates.io.

## Decision

1. The product is named **Tracemark**, and the command is `tracemark`. The name says what the tool does: it traces connections and marks each claim with its evidence class.
2. The crates, binary, index folder (`.tracemark/`), ignore file (`.tracemarkignore`), release archives and repository are renamed to match.
3. There is no compatibility layer for the old names. 0.1.x was public for one day, and the index is a rebuildable cache (ADR 0001). The changelog lists every change a user must make.
4. Decision records, the audit and changelog entries written before the rename keep the old names, because they describe what was true at the time.

## Checked before choosing (2026-11-13)

- `tracemark`, `tracemark-cli`, `tracemark-core` and `tracemark-engine` were free on crates.io. `tracemark` was free on npm, PyPI and Homebrew core.
- GitHub repositories named tracemark were small annotation and watermark tools. None was a code-analysis tool.
- `tracemark.dev` and `tracemark.io` had no DNS records. `.com` and `.app` are registered.
- Trademark registries were not searched.

## Consequences

- Users of 0.1.x must rename `.codeatlasignore` and may delete `.codeatlas/`.
- GitHub redirects the old repository address. Old release download links keep working through that redirect.
