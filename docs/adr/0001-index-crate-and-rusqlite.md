# ADR 0001: One index crate, rusqlite instead of SQLx

Status: accepted (Milestone 1.1)

## Context

The specification lists separate `atlas-index`, `atlas-storage`, `atlas-graph`, `atlas-analysis` and `atlas-application` crates, and SQLx for database access.

At this milestone there is one consumer (the CLI), one storage backend, and a few hundred lines of index, resolution and query code.

## Decision

1. Discovery, incremental indexing, SQLite persistence, relationship resolution, graph queries and report generation live in one crate, `atlas-index`. It is the shared engine: interfaces call `atlas_index::Index` and implement no analysis themselves.
2. Database access uses `rusqlite` with bundled SQLite, not SQLx.
3. The index is a rebuildable cache. A schema version change drops and recreates it. There are no migrations yet.
4. Evidence is stored inline: every symbol, import, call and diagnostic row carries its file and source position, and every call carries a status and a note. There is no separate evidence table yet.

## Why

- SQLx needs an async runtime. Nothing else in the engine is async, and indexing is CPU and disk bound. `rusqlite` is synchronous and needs no runtime.
- Crate boundaries with a single consumer on each side add files, not safety. The spec's dependency rule that matters today is kept: `atlas-domain` depends on nothing internal, `atlas-parser` only on the domain, and no interface reimplements analysis.

## When to revisit

- Split `atlas-storage` out when a second storage consumer or backend appears.
- Split `atlas-graph` out when graph queries need their own model (paths, impact analysis).
- Add migrations and an evidence table when the index holds data a rescan cannot recreate: explanations, runtime observations, analysis history.
