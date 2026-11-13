# ADR 0008: MCP connector inside the CLI binary

Status: accepted (0.4.0)

## Context

AI coding agents edit code they have not fully read, and guess at structure they cannot see. The Model Context Protocol (MCP) is the common way agents such as Claude Code, Claude Desktop and Cursor call local tools. The planned architecture listed a separate `atlas-mcp` crate for this.

## Decision

1. `tracemark mcp` is a subcommand of the existing binary, implemented in `tracemark-cli/src/mcp.rs`. It is an interface like the CLI: it calls `tracemark_index` and does no analysis itself.
2. The transport is JSON-RPC 2.0 over stdio, one message per line, written with `serde_json`. No MCP library is added.
3. The project root is fixed by `--project` at start. No tool takes a path, so an agent cannot use the server to read other folders.
4. Every query tool rescans changed files first. A rescan of an unchanged project takes about 0.4 s for 1,500 files (CHANGELOG 0.1.1), and it keeps answers in step with the agent's edits.
5. Results are the same JSON as `--json` output, evidence classes included. The server's `instructions` tell the agent how to read the classes.
6. A failure inside a tool returns a result with `isError: true`, so the agent can read and react to it. Unknown tools or methods return JSON-RPC errors.
7. Results over 500 items are cut, with `truncated: true`.

## Why

- One binary keeps install and release simple. Every channel (Homebrew, winget, release archives, the GitHub Action) already ships it.
- Tools, resources and prompts are the whole of MCP's server side, and only tools are needed here. That is about 250 lines without a library, and avoids a dependency tree to audit.

## Consequences

- No resources, prompts or progress notifications. Agents call tools only.
- Long scans block the server, because requests are handled one at a time.

## When to revisit

- Move to a `tracemark-mcp` crate if the desktop app or another host needs to embed the server.
- Add resources (for example the HTML report) if agents ask for them.
