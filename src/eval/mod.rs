//! Evaluation engine: builds a command registry from config and evaluates commands.
//!
//! The [`CommandRegistry`](crate::eval::CommandRegistry) is the central evaluation structure. It maps command
//! names to [`CommandSpec`](crate::commands::CommandSpec) implementations and
//! handles compound command decomposition, substitution evaluation, wrapper
//! command unwrapping, and decision aggregation.

/// Per-segment evaluation context (base command, args, env vars, redirections).
pub mod context;
/// Decision enum and rule match types.
pub mod decision;

pub use context::CommandContext;
pub use decision::{Decision, RuleMatch};

use std::collections::HashMap;

use crate::commands::CommandSpec;
use crate::config::Config;
use agent_shell_parser::parse;
use agent_shell_parser::parse::{
    CommandConfig, Operator, ParsedPipeline, ResolvedCommand, ShellSegment, WrapperSpec,
};

/// Check whether a command segment is likely to succeed unconditionally.
///
/// Used during compound-command evaluation to decide whether environment
/// variables set by prior segments can be assumed available for later segments.
/// Only returns true for commands with deterministic, side-effect-free success:
/// assignments, exports, `true`, and `echo`/`printf` (output-only).
///
/// This is intentionally conservative — returning false for an unknown command
/// just means we won't accumulate its env vars, which is the safe default.
fn is_likely_successful(segment: &ShellSegment) -> bool {
    // Subshell substitutions make success unpredictable — the substituted
    // command could fail, changing the segment's exit code.
    if !segment.substitutions.is_empty() {
        return false;
    }
    let words = &segment.words;
    if words.is_empty() {
        return false;
    }
    // Bare VAR=VALUE assignment (single token with `=`)
    if words.len() == 1 && words[0].as_assignment().is_some() {
        return true;
    }
    // Use the first non-env-var word as the base command
    let base = CommandContext::base_command_from_words(words);
    match base.as_str() {
        // export/unset with assignments is near-infallible
        "export" | "unset" => true,
        // Builtins/commands that always succeed
        "true" => true,
        // Output-only commands that succeed unless stdout is broken
        "echo" | "printf" => true,
        _ => false,
    }
}

/// Check whether a string is a valid shell variable name.
fn is_var_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Extract environment variable assignments from an `export` or bare assignment segment.
///
/// Accepts pre-tokenized words (from `segment.words`).
///
/// Handles:
/// - `["export", "FOO=bar", "BAZ=qux"]` → [("FOO", "bar"), ("BAZ", "qux")]
/// - `["export", "FOO=bar"]` → [("FOO", "bar")]
/// - `["FOO=bar"]` (bare assignment, no command) → [("FOO", "bar")]
/// - `["export", "FOO"]` (no assignment) → []
/// - `["export", "-p"]` / `["export", "-n", "FOO"]` → []
fn extract_segment_env(words: &[parse::Word]) -> Vec<(String, String)> {
    if words.is_empty() {
        return Vec::new();
    }

    // Bare assignment: single token like "FOO=bar" (no command follows)
    if words.len() == 1 {
        return words[0]
            .as_assignment()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into_iter()
            .collect();
    }

    // export command: extract KEY=VALUE pairs from arguments
    if words[0] == "export" {
        return words[1..]
            .iter()
            .filter(|w| !w.is_flag()) // skip flags
            .filter_map(|w| {
                w.as_assignment()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();
    }

    Vec::new()
}

/// Extract variable names from an `unset` command.
///
/// Accepts pre-tokenized words (from `segment.words`).
///
/// Handles:
/// - `["unset", "FOO"]` → ["FOO"]
/// - `["unset", "FOO", "BAR"]` → ["FOO", "BAR"]
/// - `["unset", "-v", "FOO"]` → ["FOO"] (default behavior, unset variables)
/// - `["unset", "-f", "FOO"]` → [] (unsets functions, not variables)
fn extract_unset_vars(words: &[parse::Word]) -> Vec<&str> {
    if words.is_empty() || words[0] != "unset" {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut unsetting_functions = false;
    for word in &words[1..] {
        if word == "-f" {
            unsetting_functions = true;
        } else if word == "-v" {
            unsetting_functions = false;
        } else if !word.is_flag() && !unsetting_functions && is_var_name(word) {
            result.push(word.as_str());
        }
    }
    result
}

/// Registry of all command specs, keyed by command name.
///
/// Built from [`Config`] via [`from_config`](Self::from_config).
/// Handles single-command evaluation, compound command decomposition,
/// wrapper command unwrapping, substitution evaluation, and decision aggregation.
pub struct CommandRegistry {
    /// Command name → evaluation spec (git, cargo, kubectl, gh, simple, deny).
    specs: HashMap<String, Box<dyn CommandSpec>>,
    /// Wrapper commands (e.g. `xargs`, `sudo`, `env`) → floor decision.
    /// These execute their arguments as subcommands and are handled
    /// separately from regular specs.
    wrappers: HashMap<String, Decision>,
    /// Merged command config for `resolve_command_with`: agent-shell-parser's
    /// default config extended with any cc-toolgate wrappers that aren't
    /// already known to the parser.
    resolve_config: CommandConfig,
    /// When true, DENY decisions are escalated to ASK.
    escalate_deny: bool,
    /// Path to the project overlay file that contributed to this config,
    /// if one was loaded. Used to annotate ASK decisions with provenance.
    project_overlay_path: Option<std::path::PathBuf>,
}

impl CommandRegistry {
    /// Build the registry from configuration.
    pub fn from_config(config: &Config) -> Self {
        use crate::commands::{simple::SimpleCommandSpec, tools::knowledge::KnowledgeSpec};

        let mut specs: HashMap<String, Box<dyn CommandSpec>> = HashMap::new();

        // Allow commands
        for name in &config.commands.allow {
            specs.insert(
                name.clone(),
                Box::new(SimpleCommandSpec::new(Decision::Allow)),
            );
        }

        // Ask commands
        for name in &config.commands.ask {
            specs.insert(
                name.clone(),
                Box::new(SimpleCommandSpec::new(Decision::Ask)),
            );
        }

        // KB-backed spec for subcommand-aware commands.
        // Replaces per-tool specs (GitSpec, CargoSpec, GhSpec, KubectlSpec)
        // with a unified evaluator that delegates to classify().
        // Only registered for commands that have subcommand-aware evaluation
        // in the KB — simple read-only/mutating commands are already covered
        // by the allow/ask/deny lists above.
        let kb_spec = KnowledgeSpec::from_config(config);
        let kb = agent_command_knowledge::default_knowledge_base();
        for cmd_name in kb.commands.keys() {
            // Only register KB spec for commands that have subcommands defined
            // (these benefit from subcommand-aware evaluation).
            let cmd_knowledge = &kb.commands[cmd_name];
            if !cmd_knowledge.subcommands.is_empty() {
                specs.insert(cmd_name.clone(), Box::new(kb_spec.clone()));
            }
        }

        // Deny commands (registered AFTER KB so deny always wins — a future KB
        // version cannot accidentally override a deny-listed command).
        for name in &config.commands.deny {
            specs.insert(
                name.clone(),
                Box::new(SimpleCommandSpec::new(Decision::Deny)),
            );
        }

        // Wrapper commands: these execute their arguments as subcommands.
        // Remove them from the specs map (they're handled separately in evaluate_single).
        let mut wrappers = HashMap::new();
        for name in &config.wrappers.allow_floor {
            specs.remove(name);
            wrappers.insert(name.clone(), Decision::Allow);
        }
        for name in &config.wrappers.ask_floor {
            specs.remove(name);
            wrappers.insert(name.clone(), Decision::Ask);
        }

        // Build a merged CommandConfig for resolve_command_with: start from
        // agent-shell-parser's default config and add any cc-toolgate wrappers
        // that aren't already known to the parser. This lets resolve_command_with
        // handle ALL wrappers — no fallback flag-skipping needed.
        let resolve_config = Self::build_resolve_config(&wrappers);

        Self {
            specs,
            wrappers,
            resolve_config,
            escalate_deny: config.settings.escalate_deny,
            project_overlay_path: config.project_overlay_path.clone(),
        }
    }

    /// Override the escalate_deny setting (e.g. from --escalate-deny CLI flag).
    pub fn set_escalate_deny(&mut self, escalate: bool) {
        self.escalate_deny = escalate;
    }

    /// Look up a spec by exact command name.
    fn get(&self, name: &str) -> Option<&dyn CommandSpec> {
        self.specs.get(name).map(|b| b.as_ref())
    }

    /// Build a merged [`CommandConfig`] for `resolve_command_with`.
    ///
    /// Starts from agent-shell-parser's default config and adds a minimal
    /// [`WrapperSpec`] for any cc-toolgate wrapper that isn't already known
    /// to the parser. This ensures `resolve_command_with` can handle all
    /// wrappers without a fallback code path.
    fn build_resolve_config(wrappers: &HashMap<String, Decision>) -> CommandConfig {
        let mut config = parse::default_command_config().clone();

        for name in wrappers.keys() {
            let already_known = config.wrappers.iter().any(|w| w.name == *name);
            if !already_known {
                // Add a minimal spec: skip leading flags, no value-consuming
                // flags (conservative — may stop early, which is safe since
                // the inner command gets evaluated anyway).
                config.wrappers.push(WrapperSpec {
                    name: name.clone(),
                    short_value_flags: vec![],
                    long_value_flags: vec![],
                    unanalyzable_flags: vec![],
                    skip_env_assignments: false,
                    has_terminator: true,
                    skip_positionals: 0,
                });
            }
        }
        config
    }

    /// Check if a command is a wrapper; return its floor decision if so.
    fn wrapper_floor(&self, name: &str) -> Option<Decision> {
        self.wrappers.get(name).copied()
    }

    /// Extract the wrapped command from a wrapper invocation.
    ///
    /// Uses `resolve_command_with` with the merged config that includes both
    /// agent-shell-parser's built-in wrappers and any cc-toolgate-only wrappers.
    fn extract_wrapped_command(&self, ctx: &CommandContext) -> (String, bool) {
        let resolved = parse::resolve_command_with(&ctx.words, &self.resolve_config);
        match resolved {
            ResolvedCommand::Resolved(ref parsed) if parsed.command != ctx.base_command => {
                // Successfully stripped the wrapper — return the inner command
                (parsed.to_words().join(" "), false)
            }
            ResolvedCommand::Resolved(_) => {
                // resolve_command returned the same command (e.g. wrapper with
                // no inner command, or wrapper not recognized despite config).
                (String::new(), false)
            }
            ResolvedCommand::Unanalyzable(_) => {
                // Unanalyzable (eval, source, shell -c) — signal to caller
                (String::new(), true)
            }
            // Future variants: treat as unanalyzable (fail-closed)
            _ => (String::new(), true),
        }
    }

    /// Apply escalate_deny: DENY → ASK with annotation.
    fn maybe_escalate(&self, mut result: RuleMatch) -> RuleMatch {
        if self.escalate_deny && result.decision == Decision::Deny {
            result.decision = Decision::Ask;
            result.reason = format!("{} (escalated from deny)", result.reason);
        }
        result
    }

    /// Annotate an ASK decision with project overlay provenance, if applicable.
    fn maybe_annotate_project_overlay(&self, mut result: RuleMatch) -> RuleMatch {
        if result.decision == Decision::Ask
            && let Some(ref path) = self.project_overlay_path
        {
            result.reason = format!(
                "{} (project config at {} contributed to this decision)",
                result.reason,
                path.display()
            );
        }
        result
    }

    /// Evaluate a single (non-compound) command against the registry.
    pub fn evaluate_single(&self, command: &str) -> RuleMatch {
        let ctx = CommandContext::from_command(command);
        let result = self.evaluate_ctx(ctx);
        self.maybe_annotate_project_overlay(result)
    }

    /// Evaluate a command context against the registry.
    ///
    /// This is the core evaluation method. All paths — simple commands,
    /// compound segments, and wrapper-extracted inner commands — converge here.
    fn evaluate_ctx(&self, ctx: CommandContext) -> RuleMatch {
        // Bare variable assignments (e.g. "FOO=bar") are always safe.
        // Check before the empty-command guard: a segment like "VAR=$(cmd)"
        // has base_command="" (the token is parsed as an env var with no
        // command), but it's a valid assignment, not an empty command.
        if ctx.words.len() == 1 && ctx.words[0].as_assignment().is_some() {
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("variable assignment: {}", ctx.words[0]),
            };
        }

        if ctx.base_command.is_empty() {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "empty".into(),
            };
        }

        // Wrapper commands: extract inner command, evaluate it, return max(floor, inner).
        if let Some(floor) = self.wrapper_floor(&ctx.base_command) {
            let (wrapped_cmd, is_unanalyzable) = self.extract_wrapped_command(&ctx);
            let mut strictest = floor;
            let mut reason = if is_unanalyzable {
                // Unanalyzable (eval, source, shell -c) → ASK
                strictest = Decision::Ask;
                format!("{} wraps unanalyzable command", ctx.base_command)
            } else if !wrapped_cmd.is_empty() {
                // env -i / env - clears the environment for the wrapped command.
                let inner_env = if ctx.base_command == "env" && ctx.has_any_flag(&["-i", "-"]) {
                    HashMap::new()
                } else {
                    ctx.accumulated_env.clone()
                };
                let mut inner_ctx = CommandContext::from_command(&wrapped_cmd);
                inner_ctx.accumulated_env = inner_env;
                let inner = self.evaluate_ctx(inner_ctx);
                if inner.decision > strictest {
                    strictest = inner.decision;
                }
                format!("{} wraps: {}", ctx.base_command, inner.reason)
            } else {
                format!("{} (no wrapped command)", ctx.base_command)
            };
            // Redirection on the wrapper itself escalates Allow → Ask
            if strictest == Decision::Allow && ctx.redirection.is_some() {
                strictest = Decision::Ask;
                reason = format!("{} with output redirection", reason);
            }
            return self.maybe_escalate(RuleMatch {
                decision: strictest,
                reason,
            });
        }

        // Look up by exact base command name
        if let Some(spec) = self.get(&ctx.base_command) {
            return self.maybe_escalate(spec.evaluate(&ctx));
        }

        // Dotted command fallback for deny list (e.g. mkfs.ext4 → mkfs)
        if let Some(prefix) = ctx.base_command.split('.').next()
            && prefix != ctx.base_command
            && let Some(spec) = self.get(prefix)
        {
            return self.maybe_escalate(spec.evaluate(&ctx));
        }

        // Fallthrough → ask
        RuleMatch {
            decision: Decision::Ask,
            reason: format!("unrecognized command: {}", ctx.base_command),
        }
    }

    /// Recursively evaluate a pipeline tree, collecting substitution results.
    ///
    /// This is the recursive tree walk that replaces the old flat substitution loop.
    /// For each segment, we first evaluate its substitutions, then the segment itself.
    /// Structural substitutions (for-loop values, case subjects) are evaluated first.
    fn evaluate_pipeline(
        &self,
        pipeline: &ParsedPipeline,
        accumulated_env: &mut HashMap<String, String>,
        reasons: &mut Vec<String>,
    ) -> Decision {
        let mut strictest = Decision::Allow;

        // Evaluate structural substitutions first (for-loop values, case subjects)
        for sub in &pipeline.structural_substitutions {
            let sub_decision = self.evaluate_pipeline(&sub.pipeline, &mut HashMap::new(), reasons);
            let label: String = sub
                .pipeline
                .segments
                .iter()
                .map(|s| s.command.as_str())
                .collect::<Vec<_>>()
                .join(" && ");
            let label: String = label.trim().chars().take(60).collect();
            reasons.push(format!(
                "  structural-subst[$({label})] -> {}: (nested)",
                sub_decision.label(),
            ));
            if sub_decision > strictest {
                strictest = sub_decision;
            }
        }

        // Evaluate each segment with its substitutions
        let mut segment_executes = true;

        for (i, segment) in pipeline.segments.iter().enumerate() {
            // Determine if this segment executes based on the preceding operator.
            if i > 0 {
                let op = &pipeline.operators[i - 1];
                match op {
                    // Semicolon: unconditional — segment always executes.
                    Operator::Semi => segment_executes = true,
                    // And: segment executes only if prior executed AND succeeded.
                    Operator::And => {
                        segment_executes =
                            segment_executes && is_likely_successful(&pipeline.segments[i - 1]);
                    }
                    // Or / Pipe / PipeErr / Background: can't guarantee execution or env propagation.
                    Operator::Or | Operator::Pipe | Operator::PipeErr | Operator::Background => {
                        segment_executes = false;
                        accumulated_env.clear();
                    }
                    // Future operator variants: conservative behavior
                    _ => {
                        segment_executes = false;
                        accumulated_env.clear();
                    }
                }
            }

            // Evaluate substitutions within this segment (recursive tree walk).
            // Substitutions don't propagate env to parent — use a fresh env.
            for sub in &segment.substitutions {
                let sub_decision =
                    self.evaluate_pipeline(&sub.pipeline, &mut HashMap::new(), reasons);
                // Build a readable label from the substitution's inner pipeline segments
                let label: String = sub
                    .pipeline
                    .segments
                    .iter()
                    .map(|s| s.command.as_str())
                    .collect::<Vec<_>>()
                    .join(" && ");
                let label: String = label.trim().chars().take(60).collect();
                reasons.push(format!(
                    "  subst[$({label})] -> {}: (nested)",
                    sub_decision.label(),
                ));
                if sub_decision > strictest {
                    strictest = sub_decision;
                }
            }

            // Build a CommandContext from the structured segment — uses the
            // pre-tokenized words from tree-sitter directly.
            let mut ctx = CommandContext::from_segment(segment);
            ctx.accumulated_env = accumulated_env.clone();

            let mut result = self.evaluate_ctx(ctx);

            // Accumulate env vars from this segment if it's known to execute.
            // Use the segment's pre-tokenized words directly (substitutions
            // are already evaluated separately via the recursive tree walk).
            if segment_executes {
                for (key, val) in extract_segment_env(&segment.words) {
                    accumulated_env.insert(key, val);
                }
                for var in extract_unset_vars(&segment.words) {
                    accumulated_env.remove(var);
                }
            }

            // Propagate redirection from wrapping constructs
            if result.decision == Decision::Allow
                && let Some(ref r) = segment.redirection
            {
                result.decision = Decision::Ask;
                result.reason = format!("{} (escalated: wrapping {})", result.reason, r);
            }
            let label: String = segment.command.trim().chars().take(60).collect();
            reasons.push(format!(
                "  [{label}] -> {}: {}",
                result.decision.label(),
                result.reason
            ));
            if result.decision > strictest {
                strictest = result.decision;
            }
        }

        strictest
    }

    /// Evaluate a full command string, handling compound expressions and substitutions.
    pub fn evaluate(&self, command: &str) -> RuleMatch {
        let pipeline = match parse::parse_with_substitutions(command) {
            Ok(p) => p,
            Err(_) => {
                // ParseError → ASK (fail-closed)
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: "parse error (fail-closed)".into(),
                };
            }
        };

        // Check for parse errors in the pipeline tree → ASK (fail-closed)
        if pipeline.has_parse_errors_recursive() {
            // Still evaluate what we can, but escalate to ASK minimum
            let mut strictest = Decision::Ask;
            let mut reasons = vec!["  parse errors detected (fail-closed)".to_string()];
            let mut accumulated_env: HashMap<String, String> = HashMap::new();
            let tree_decision =
                self.evaluate_pipeline(&pipeline, &mut accumulated_env, &mut reasons);
            if tree_decision > strictest {
                strictest = tree_decision;
            }
            return RuleMatch {
                decision: strictest,
                reason: format!(
                    "compound command (parse errors, fail-closed):\n{}",
                    reasons.join("\n")
                ),
            };
        }

        // Simple case: no substitutions, not compound, and the segment text matches
        // the original command → evaluate directly.
        let has_substitutions = pipeline
            .find_segment(&|seg| {
                if !seg.substitutions.is_empty() {
                    Some(())
                } else {
                    None
                }
            })
            .is_some()
            || !pipeline.structural_substitutions.is_empty();

        if pipeline.segments.len() <= 1 && !has_substitutions {
            let is_passthrough = match pipeline.segments.first() {
                Some(seg) => seg.command.trim() == command.trim(),
                None => true,
            };
            if is_passthrough {
                return self.evaluate_single(command);
            }
        }

        let mut reasons = Vec::new();
        let mut accumulated_env: HashMap<String, String> = HashMap::new();
        let strictest = self.evaluate_pipeline(&pipeline, &mut accumulated_env, &mut reasons);

        // Build summary header
        let mut desc = Vec::new();
        if !pipeline.operators.is_empty() {
            let mut unique_ops: Vec<&str> = pipeline.operators.iter().map(|o| o.as_str()).collect();
            unique_ops.sort();
            unique_ops.dedup();
            desc.push(unique_ops.join(", "));
        }
        if has_substitutions {
            let sub_count = pipeline.filter_segments(&|seg| {
                if !seg.substitutions.is_empty() {
                    Some(seg.substitutions.len())
                } else {
                    None
                }
            });
            let total: usize =
                sub_count.iter().sum::<usize>() + pipeline.structural_substitutions.len();
            desc.push(format!("{total} substitution(s)"));
        }
        let header = if desc.is_empty() {
            "compound command".into()
        } else {
            format!("compound command ({})", desc.join("; "))
        };

        self.maybe_annotate_project_overlay(RuleMatch {
            decision: strictest,
            reason: format!("{}:\n{}", header, reasons.join("\n")),
        })
    }
}

#[cfg(test)]
mod tests;
