//! Subcommand-aware evaluators for specific CLI tools.
//!
//! Each module implements `CommandSpec` with tool-specific
//! logic: subcommand extraction, read-only vs mutating classification,
//! env-gated auto-allow, and redirection escalation.

/// Subcommand-aware cargo evaluation (build → allow, install → ask, etc.).
pub mod cargo;
/// Subcommand-aware GitHub CLI evaluation (pr list → allow, pr create → ask, etc.).
pub mod gh;
/// Subcommand-aware git evaluation with env-gating and force-push detection.
pub mod git;
/// Subcommand-aware kubectl evaluation (get → allow, apply → ask, etc.).
pub mod kubectl;
