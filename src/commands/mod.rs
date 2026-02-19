//! Command evaluation specs: per-tool logic for deciding allow/ask/deny.
//!
//! Each command family (git, cargo, kubectl, gh) has its own `CommandSpec`
//! implementation with subcommand-aware evaluation. Simple commands use
//! `SimpleCommandSpec` for flat name → decision mapping.

/// Subcommand-aware cargo evaluation (build → allow, install → ask, etc.).
pub mod cargo;
/// Unconditional deny spec for destructive commands (shred, dd, mkfs, etc.).
pub mod deny;
/// Subcommand-aware GitHub CLI evaluation (pr list → allow, pr create → ask, etc.).
pub mod gh;
/// Subcommand-aware git evaluation with env-gating and force-push detection.
pub mod git;
/// Subcommand-aware kubectl evaluation (get → allow, apply → ask, etc.).
pub mod kubectl;
/// Data-driven spec for flat allow/ask/deny command lists.
pub mod simple;

use crate::eval::{CommandContext, RuleMatch};

/// Trait for command evaluation specs.
///
/// Each implementation knows how to evaluate a specific command (or family of commands)
/// and returns a `RuleMatch` with the decision and reason.
pub trait CommandSpec: Send + Sync {
    /// Evaluate the command in the given context and return a decision.
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch;
}
