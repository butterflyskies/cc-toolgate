# Suggested Commands

**IMPORTANT:** Builds must run inside distrobox (immutable Bazzite host needs C compiler for deps).

## Build
```bash
distrobox enter raptor -- cargo build
distrobox enter raptor -- cargo build --release
```

## Test
```bash
distrobox enter raptor -- cargo test
distrobox enter raptor -- cargo test -- --test-threads=1  # if tests conflict
```

## Lint
```bash
distrobox enter raptor -- cargo clippy
```

## Format
```bash
distrobox enter raptor -- cargo fmt
```

## Check (no build)
```bash
distrobox enter raptor -- cargo check
```

## Run
```bash
echo '{"tool_name":"Bash","tool_input":{"command":"ls"}}' | distrobox enter raptor -- cargo run
```

## Git
```bash
GIT_CONFIG_GLOBAL=~/.gitconfig.ai git <command>
```

## GitHub CLI
```bash
GH_CONFIG_DIR=~/.config/gh-butterflysky-ai gh <command>
```
