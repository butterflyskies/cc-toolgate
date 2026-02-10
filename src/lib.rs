pub mod commands;
pub mod config;
pub mod eval;
pub mod logging;
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
