pub mod commands;
pub mod eval;
pub mod logging;
pub mod parse;

use eval::RuleMatch;

/// Build the default registry and evaluate a command string.
///
/// This is the main entry point for the evaluation pipeline.
pub fn evaluate(command: &str) -> RuleMatch {
    let registry = eval::default_registry();
    registry.evaluate(command)
}
