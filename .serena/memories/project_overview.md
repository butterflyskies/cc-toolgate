# cc-toolgate Project Overview

**Purpose:** PreToolUse hook for Claude Code that gates Bash commands with compound-command-aware validation.
**Language:** Rust (edition 2024)
**Repo:** butterflyskies/cc-toolgate
**Version:** 0.6.0
**Binary:** `~/.cargo/bin/cc-toolgate` (installed via `cargo install --path .`)

## Architecture
```
src/
  main.rs           Entry point + CLI flags
  lib.rs            Re-exports, top-level evaluate() orchestrator
  config.rs         TOML config loading, ConfigOverlay merge system
  parse/
    mod.rs          Re-exports
    shell.rs        tree-sitter-bash AST: compound splitting, substitution extraction, redirection detection
    tokenize.rs     shlex-based word splitting, base_command(), env_vars()
    types.rs        ParsedPipeline, ShellSegment, Operator, Redirection
  eval/
    mod.rs          CommandRegistry, strictest-wins aggregation
    context.rs      CommandContext struct, env_satisfies() with shellexpand
    decision.rs     Decision enum, RuleMatch
  commands/
    mod.rs          CommandSpec trait
    simple.rs       SimpleCommandSpec (flat allow/ask/deny)
    tools/          Subcommand-aware evaluators for specific CLI tools
      git.rs        GitSpec — subcommand-aware git evaluation
      cargo.rs      CargoSpec — subcommand-aware cargo evaluation
      kubectl.rs    KubectlSpec — subcommand-aware kubectl evaluation
      gh.rs         GhSpec — subcommand-aware gh evaluation
  logging.rs        File appender (~/.local/share/cc-toolgate/decisions.log)
tests/
  integration.rs    Integration tests (decision_test! macro + complex tests)
config.default.toml Embedded default config
.config/
  nextest.toml      cargo-nextest configuration
.github/workflows/
  claude-code-review.yml  Auto PR review
  claude.yml              @claude mention handler
```

## Key Types (src/eval/mod.rs — free functions)
- `is_likely_successful(segment)` — heuristic for && chain propagation
- `extract_segment_env(segment)` — KEY=VALUE from export/bare assignment
- `extract_unset_vars(segment)` — variable names from unset commands
- `parse_assignment(token)` / `is_var_name(s)` — shared helpers

## Key Types (src/parse/types.rs)
- `ParsedPipeline { segments: Vec<ShellSegment>, operators: Vec<Operator> }`
- `ShellSegment { command: String, redirection: Option<Redirection> }`
- `Redirection { description: String }`
- `Operator` enum: Pipe, PipeErr, And, Or, Semi

## Dependencies
serde, serde_json, toml, shlex, log, simplelog, tree-sitter, tree-sitter-bash, shellexpand

## Tests
418+ total: unit (colocated) + integration (tests/integration.rs)
- All modules have thorough rustdoc — zero `cargo doc --document-private-items` warnings
- Tests that mutate process env use `require_nextest()` guard — must run via `cargo nextest run`
