# ADR 0009: Desktop app with Tauri and a plain HTML interface

Status: accepted (0.5.0)

## Context

The planned architecture named Tauri + React + TypeScript for the desktop interface. Users asked for an app they can install and click through, not only a command line.

## Decision

1. The app is a Tauri 2 shell in `desktop/`, calling `tracemark_index` directly. Like the CLI and the MCP server, it does no analysis of its own.
2. The interface is plain HTML, CSS and JavaScript in `desktop/ui/`, without React or an npm build. This departs from the original plan.
3. `desktop/` is a separate Cargo workspace with its own lockfile and `deny.toml`. The core crates and their CI never build Tauri or need the system GUI libraries.
4. Security:
   - The window may call only the app's four commands and open a folder picker (`capabilities/default.json`).
   - The content-security policy allows only the app's own files and its IPC channel.
   - All text from a project is inserted with `textContent`, never as HTML.
5. Release builds produce an NSIS installer (Windows), a dmg (macOS on Apple Silicon), and an AppImage and deb (Linux). They are attached to the release with SHA-256 files and build provenance. The release fails if the app version differs from the workspace version.
6. Icons are generated in CI from `icons/source.png` by `cargo tauri icon`. Only the source image is committed.

## Why

- Four screens do not need a framework. Skipping npm removes a second dependency tree to audit and a second build tool. React can be added if the interface grows.
- A separate workspace keeps the core small. Tauri brings about 450 crates, and its Linux build needs WebKitGTK headers.

## Consequences

- One unmaintained crate (`proc-macro-error`, RUSTSEC-2024-0370) is accepted in the desktop tree. It is a build-time helper pulled in by Tauri's GTK 3 bindings on Linux.
- The installers are not code-signed, so Windows SmartScreen and macOS Gatekeeper warn on first run.
- No Intel macOS build of the app yet. The command-line tool covers Intel Macs.

## When to revisit

- Add React (or another framework) when the interface needs shared components or complex state.
- Sign the installers once certificates exist.
- Add a drawn call graph if users ask for one; today the graph is followed by clicking names.
