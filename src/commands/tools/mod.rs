//! Subcommand-aware evaluators for specific CLI tools.
//!
//! The `KnowledgeSpec` in `knowledge.rs` is the unified evaluator that handles
//! all KB-known commands (git, cargo, gh, kubectl, etc.) through a single
//! evaluation path: classify via KB, then apply cc-toolgate policy (env-gating,
//! redirection escalation, version flag detection, force-push detection).

/// KB-backed unified evaluation using `agent_command_knowledge::classify()`.
pub mod knowledge;
