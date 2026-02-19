# cc-toolgate Project Overview

**Purpose:** PreToolUse hook for Claude Code that gates Bash commands with compound-command-aware validation.
**Language:** Rust (edition 2024)
**Repo:** butterflyskies/cc-toolgate
**Binary:** target/release/cc-toolgate (~2.5MB)

## Architecture
```
src/
  main.rs          Entry point + CLI flags + 219 integration tests
  lib.rs           Re-exports, top-level evaluate() orchestrator
  config.rs        TOML config loading, ConfigOverlay merge system
  parse/
    mod.rs         Re-exports
    shell.rs       tree-sitter-bash AST: compound splitting, substitution extraction, redirection detection
    tokenize.rs    shlex-based word splitting, base_command(), env_vars()
    types.rs       ParsedPipeline, ShellSegment, Operator, Redirection
  eval/
    mod.rs         CommandRegistry, worst-wins aggregation
    context.rs     CommandContext struct
    decision.rs    Decision enum, RuleMatch
  commands/        CommandSpec implementations (simple, deny, git, cargo, kubectl, gh)
  logging.rs       File appender
config.default.toml  Embedded default config
```

## Key Types (src/parse/types.rs)
- `ParsedPipeline { segments: Vec<ShellSegment>, operators: Vec<Operator> }`
- `ShellSegment { command: String, redirection: Option<Redirection> }`
- `Redirection { description: String }`
- `Operator` enum: Pipe, PipeErr, And, Or, Semi

## Dependencies
serde, serde_json, toml, shlex, log, simplelog, tree-sitter, tree-sitter-bash

## Tests
336 total: 117 lib (colocated) + 219 integration (in main.rs)
