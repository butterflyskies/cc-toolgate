//! Shell command parsing: tree-sitter-bash AST, shlex tokenizer, and pipeline types.
//!
//! This module provides three layers:
//!
//! - `shell` — tree-sitter-bash AST walker that decomposes commands into segments,
//!   operators, substitutions, and redirections.
//! - `tokenize` — shlex-based word splitting, base command extraction, and env var parsing.
//! - `types` — data types shared between the parser and evaluator.

/// tree-sitter-bash AST walker for compound command splitting.
pub mod shell;
/// shlex-based tokenization: word splitting, base command extraction, env var parsing.
pub mod tokenize;
/// Shared types: [`ParsedPipeline`], [`ShellSegment`], [`Operator`], [`Redirection`].
pub mod types;

pub use shell::{has_output_redirection, parse_with_substitutions};
pub use tokenize::{base_command, env_vars, tokenize};
pub use types::{Operator, ParsedPipeline, Redirection, ShellSegment};
