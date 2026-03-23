# ADR-0001: MSRV policy — match dependency requirements

## Status
Accepted

## Context
Declaring a Minimum Supported Rust Version (MSRV) signals to downstream users which
toolchain they need. cc-toolgate uses edition 2024 (stabilized in 1.85), but transitive
dependencies may require a newer toolchain. Empirical check of the dependency tree shows
`time 0.3.47` (via `simplelog`) requires Rust 1.88.

## Decision
Set MSRV to match the highest requirement among our dependencies (currently 1.88). When
a dependency bumps its MSRV, we follow rather than holding back or pinning older versions.

The MSRV is declared in `Cargo.toml` (`rust-version`) and will be enforced by a CI job
that runs `cargo check` on the declared toolchain.

## Consequences
- Anyone installing via `cargo install cc-toolgate` needs rustc 1.88+
- MSRV bumps are driven by `cargo update`, not by our own code changes
- CI catches MSRV regressions before they reach main
