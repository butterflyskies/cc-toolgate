# Task Completion Checklist

After completing any code change:

1. `distrobox enter raptor -- cargo fmt` — format code
2. `distrobox enter raptor -- cargo clippy` — zero warnings
3. `distrobox enter raptor -- cargo test` — all tests pass
4. `distrobox enter raptor -- cargo build --release` — release binary builds
5. Verify binary size hasn't grown unexpectedly
