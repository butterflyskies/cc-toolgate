//! cc-toolgate: a PreToolUse hook for Claude Code that gates Bash commands.
//!
//! This crate evaluates shell commands against configurable rules and returns
//! one of three decisions: [`eval::Decision::Allow`], [`eval::Decision::Ask`],
//! or [`eval::Decision::Deny`]. Commands are parsed into an AST using
//! tree-sitter-bash, split into segments, and each segment is evaluated
//! against a [`CommandRegistry`](crate::eval::CommandRegistry) built from configuration.
//!
//! # Architecture
//!
//! - **[`parse`]** — Shell parsing: tree-sitter-bash AST walker, shlex tokenizer, type definitions.
//! - **[`eval`]** — Evaluation engine: command registry, decision types, per-segment context.
//! - **[`commands`]** — Command specs: per-tool evaluation logic (git, cargo, kubectl, gh, etc.).
//! - **[`config`]** — Configuration loading: embedded defaults + user overlay merge.
//! - **[`logging`]** — Decision logging to `~/.local/share/cc-toolgate/decisions.log`.

/// Command spec trait and per-tool implementations.
pub mod commands;
/// Configuration types, loading, and overlay merge logic.
pub mod config;
/// Evaluation engine: registry, decision aggregation, command context.
pub mod eval;
/// File-based decision logging.
pub mod logging;
/// Shell command parsing: tree-sitter AST, shlex tokenizer, pipeline types.
pub mod parse;

use eval::RuleMatch;

/// Build the registry from default config and evaluate a command string.
///
/// This is the main entry point for tests and simple usage.
/// For CLI usage with --escalate-deny or user config, build the registry directly.
pub fn evaluate(command: &str) -> RuleMatch {
    let config = config::Config::default_config();
    let registry = eval::CommandRegistry::from_config(&config);
    registry.evaluate(command)
}
