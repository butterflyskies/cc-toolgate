pub mod cargo;
pub mod deny;
pub mod gh;
pub mod git;
pub mod kubectl;
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
