//! Decision types for command evaluation.

/// The gating decision for a command.
///
/// Variants are ordered by severity: `Allow < Ask < Deny`.
/// When evaluating compound commands, the strictest decision across
/// all segments wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Decision {
    /// Command runs silently without user confirmation.
    Allow,
    /// Claude Code prompts the user for confirmation before running.
    Ask,
    /// Command is blocked outright and cannot be executed.
    Deny,
}

impl Decision {
    /// Lowercase string for JSON output (`"allow"`, `"ask"`, `"deny"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }

    /// Uppercase label for human-readable log output (`"ALLOW"`, `"ASK"`, `"DENY"`).
    pub fn label(self) -> &'static str {
        match self {
            Decision::Allow => "ALLOW",
            Decision::Ask => "ASK",
            Decision::Deny => "DENY",
        }
    }
}

/// The result of evaluating a command: a decision and a human-readable reason.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    /// The gating decision.
    pub decision: Decision,
    /// Human-readable explanation of why this decision was reached.
    pub reason: String,
}
