# Style & Conventions

- Rust edition 2024
- No explicit formatting config — uses default `cargo fmt` (rustfmt defaults)
- Tests colocated in modules (`#[cfg(test)] mod tests`)
- Integration tests in `src/main.rs` (not in `tests/` dir)
- Zero clippy warnings policy
- Release profile: strip=true, lto=true
- Types in dedicated files (types.rs), parsing logic separate from evaluation
- Config uses embedded defaults + user overlay merge pattern
