//! Command evaluation specs: per-tool logic for deciding allow/ask/deny.
//!
//! This module contains the `CommandSpec` trait and two categories of implementation:
//!
//! - **`simple`** — A data-driven spec for flat command lists (allow/ask/deny with no
//!   subcommand awareness).
//! - **`tools`** — Subcommand-aware evaluators for specific CLI tools (git, cargo, kubectl, gh),
//!   each with config-driven classification, env-gated auto-allow, and redirection escalation.

/// Data-driven spec for flat allow/ask/deny command lists.
pub mod simple;
/// Subcommand-aware evaluators for specific CLI tools.
pub mod tools;

use crate::eval::{CommandContext, RuleMatch};

/// Trait for command evaluation specs.
///
/// Each implementation knows how to evaluate a specific command (or family of commands)
/// and returns a `RuleMatch` with the decision and reason.
pub trait CommandSpec: Send + Sync {
    /// Evaluate the command in the given context and return a decision.
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch;
}
