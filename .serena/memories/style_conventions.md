# Style & Conventions

- Rust edition 2024
- No explicit formatting config — uses default `cargo fmt` (rustfmt defaults)
- Tests colocated in modules (`#[cfg(test)] mod tests`)
- Integration tests in `tests/integration.rs` using `decision_test!` macro
- Zero clippy warnings policy
- Zero cargo doc warnings policy (including --document-private-items)
- Rustdoc link pitfall: module-level `//!` docs can't use `[`Foo`]` for items in the same
  module when re-exported through lib.rs — use explicit `(crate::mod::Foo)` link targets
  or plain backticks
- Release profile: strip=true, lto=true
- Types in dedicated files (types.rs), parsing logic separate from evaluation
- Config uses embedded defaults + user overlay merge pattern
