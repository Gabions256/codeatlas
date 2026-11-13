# Rules for AI coding agents

Agents are contributors to Tracemark, not unquestioned authorities.

1. Never claim an implementation is complete without running relevant tests.
2. Never invent functions, interfaces or dependencies.
3. Never introduce unnecessary dependencies.
4. Never allow one crate to bypass the defined architectural boundaries.
5. Every important analytical result must have source evidence.
6. Every new feature must have tests.
7. Every architectural change must include an Architecture Decision Record.
8. Do not replace working implementations with mock data.
9. Do not silently swallow errors.
10. Always report what was implemented, tested, not tested and still pending.

Checks before any change is called done:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
