# Task Completion Checklist

After completing any code change:

1. `cargo fmt` — format code
2. `cargo clippy` — zero warnings
3. `cargo test` — all tests pass (117 unit + 219 integration)
4. `cargo build --release` — release binary builds
5. Verify binary size hasn't grown unexpectedly
