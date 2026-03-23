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
use crate::parse;
use crate::parse::Operator;

/// Check whether a command segment is likely to succeed unconditionally.
///
/// Used during compound-command evaluation to decide whether environment
/// variables set by prior segments can be assumed available for later segments.
/// Only returns true for commands with deterministic, side-effect-free success:
/// assignments, exports, `true`, and `echo`/`printf` (output-only).
///
/// This is intentionally conservative — returning false for an unknown command
/// just means we won't accumulate its env vars, which is the safe default.
fn is_likely_successful(segment: &str) -> bool {
    // Subshell substitutions make success unpredictable — the substituted
    // command could fail, changing the segment's exit code.
    if segment.contains("__SUBST__") {
        return false;
    }
    let words = parse::tokenize(segment);
    if words.is_empty() {
        return false;
    }
    // Bare VAR=VALUE assignment (single token with `=`)
    if words.len() == 1 && words[0].contains('=') {
        return parse_assignment(&words[0]).is_some();
    }
    let base = parse::base_command(segment);
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

/// Try to parse a single `KEY=VALUE` token, returning (key, value) if valid.
fn parse_assignment(token: &str) -> Option<(String, String)> {
    let eq_pos = token.find('=')?;
    let key = &token[..eq_pos];
    let val = &token[eq_pos + 1..];
    if is_var_name(key) {
        Some((key.to_string(), val.to_string()))
    } else {
        None
    }
}

/// Extract environment variable assignments from an `export` or bare assignment segment.
///
/// Handles:
/// - `export FOO=bar BAZ=qux` → [("FOO", "bar"), ("BAZ", "qux")]
/// - `export FOO=bar` → [("FOO", "bar")]
/// - `FOO=bar` (bare assignment, no command) → [("FOO", "bar")]
/// - `export FOO` (no assignment) → []
/// - `export -p` / `export -n FOO` → []
fn extract_segment_env(segment: &str) -> Vec<(String, String)> {
    let words = parse::tokenize(segment);
    if words.is_empty() {
        return Vec::new();
    }

    // Bare assignment: single token like "FOO=bar" (no command follows)
    if words.len() == 1 {
        return parse_assignment(&words[0]).into_iter().collect();
    }

    // export command: extract KEY=VALUE pairs from arguments
    if words[0] == "export" {
        return words[1..]
            .iter()
            .filter(|w| !w.starts_with('-')) // skip flags
            .filter_map(|w| parse_assignment(w))
            .collect();
    }

    Vec::new()
}

/// Extract variable names from an `unset` command.
///
/// Handles:
/// - `unset FOO` → ["FOO"]
/// - `unset FOO BAR` → ["FOO", "BAR"]
/// - `unset -v FOO` → ["FOO"] (default behavior, unset variables)
/// - `unset -f FOO` → [] (unsets functions, not variables)
fn extract_unset_vars(segment: &str) -> Vec<String> {
    let words = parse::tokenize(segment);
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
        } else if !word.starts_with('-') && !unsetting_functions && is_var_name(word) {
            result.push(word.clone());
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
    /// When true, DENY decisions are escalated to ASK.
    escalate_deny: bool,
}

impl CommandRegistry {
    /// Build the registry from configuration.
    pub fn from_config(config: &Config) -> Self {
        use crate::commands::{
            simple::SimpleCommandSpec,
            tools::{cargo::CargoSpec, gh::GhSpec, git::GitSpec, kubectl::KubectlSpec},
        };

        let mut specs: HashMap<String, Box<dyn CommandSpec>> = HashMap::new();

        // Deny commands (registered first, complex specs override if needed)
        for name in &config.commands.deny {
            specs.insert(
                name.clone(),
                Box::new(SimpleCommandSpec::new(Decision::Deny)),
            );
        }

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

        // Complex command specs (override any simple entry for the same name)
        specs.insert("git".into(), Box::new(GitSpec::from_config(&config.git)));
        specs.insert(
            "cargo".into(),
            Box::new(CargoSpec::from_config(&config.cargo)),
        );
        specs.insert(
            "kubectl".into(),
            Box::new(KubectlSpec::from_config(&config.kubectl)),
        );
        specs.insert("gh".into(), Box::new(GhSpec::from_config(&config.gh)));

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

        Self {
            specs,
            wrappers,
            escalate_deny: config.settings.escalate_deny,
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

    /// Check if a command is a wrapper; return its floor decision if so.
    fn wrapper_floor(&self, name: &str) -> Option<Decision> {
        self.wrappers.get(name).copied()
    }

    /// Extract the wrapped command from a wrapper invocation.
    ///
    /// Skips the wrapper name and its flags, then returns the remaining
    /// words joined as a command string. For `env`, also skips KEY=VALUE pairs.
    fn extract_wrapped_command(ctx: &CommandContext) -> String {
        let iter = ctx.words.iter().skip(1); // skip wrapper name

        if ctx.base_command == "env" {
            // env: skip flags AND KEY=VALUE pairs before the subcommand
            let mut rest: Vec<&str> = Vec::new();
            let mut found_cmd = false;
            for word in iter {
                if found_cmd {
                    rest.push(word);
                } else if word.starts_with('-') {
                    continue; // skip flags
                } else if word.contains('=') {
                    continue; // skip KEY=VALUE
                } else {
                    found_cmd = true;
                    rest.push(word);
                }
            }
            rest.join(" ")
        } else {
            // General case: skip flags (start with -), then collect the rest.
            // Non-flag words before the actual command (like "10" in `nice -n 10 ls`)
            // are flag values. We include them but base_command() in the recursive
            // evaluate_single call will extract the first word, so we need to
            // skip non-command words. We do this by skipping words that are purely
            // numeric (common flag values like priority, timeout seconds, etc.).
            let non_flags: Vec<&str> = iter
                .skip_while(|w| w.starts_with('-'))
                .map(|s| s.as_str())
                .collect();
            // Skip leading numeric-only words (flag values like "10", "30")
            let cmd_start = non_flags
                .iter()
                .position(|w| !w.chars().all(|c| c.is_ascii_digit() || c == '.'))
                .unwrap_or(non_flags.len());
            non_flags[cmd_start..].join(" ")
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

    /// Evaluate a single (non-compound) command against the registry.
    pub fn evaluate_single(&self, command: &str) -> RuleMatch {
        self.evaluate_single_with_env(command, &HashMap::new())
    }

    /// Evaluate a single command with accumulated environment from prior segments.
    fn evaluate_single_with_env(
        &self,
        command: &str,
        accumulated_env: &HashMap<String, String>,
    ) -> RuleMatch {
        let cmd = command.trim();
        if cmd.is_empty() {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "empty".into(),
            };
        }

        // Bare variable assignments (e.g. "FOO=bar") are always safe.
        let words = parse::tokenize(cmd);
        if words.len() == 1 && parse_assignment(&words[0]).is_some() {
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("variable assignment: {}", words[0]),
            };
        }

        let mut ctx = CommandContext::from_command(cmd);
        ctx.accumulated_env = accumulated_env.clone();

        // Wrapper commands: execute their arguments as a subcommand.
        // Extract the wrapped command, evaluate it, return max(floor, inner).
        if let Some(floor) = self.wrapper_floor(&ctx.base_command) {
            let wrapped_cmd = Self::extract_wrapped_command(&ctx);
            let mut strictest = floor;
            let mut reason = if !wrapped_cmd.is_empty() {
                // env -i / env - clears the environment for the wrapped command.
                let inner_env = if ctx.base_command == "env" && ctx.has_any_flag(&["-i", "-"]) {
                    HashMap::new()
                } else {
                    accumulated_env.clone()
                };
                let inner = self.evaluate_single_with_env(&wrapped_cmd, &inner_env);
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

    /// Evaluate a full command string, handling compound expressions and substitutions.
    pub fn evaluate(&self, command: &str) -> RuleMatch {
        let (pipeline, substitutions) = parse::parse_with_substitutions(command);

        // Simple case: no substitutions, not compound, and the segment text matches
        // the original command → evaluate directly.  When the parser extracts a
        // sub-range (e.g. a loop body), the segment text differs from the original
        // and we must fall through to compound evaluation so the inner command is
        // evaluated against actual rules instead of the enclosing keyword.
        if pipeline.segments.len() <= 1 && substitutions.is_empty() {
            let is_passthrough = match pipeline.segments.first() {
                Some(seg) => seg.command.trim() == command.trim(),
                None => true,
            };
            if is_passthrough {
                return self.evaluate_single(command);
            }
        }

        let mut strictest = Decision::Allow;
        let mut reasons = Vec::new();

        // Recursively evaluate substitution contents
        for inner in &substitutions {
            let result = self.evaluate(inner);
            let label: String = inner.trim().chars().take(60).collect();
            reasons.push(format!(
                "  subst[$({label})] -> {}: {}",
                result.decision.label(),
                result.reason
            ));
            if result.decision > strictest {
                strictest = result.decision;
            }
        }

        // Evaluate each part of the (possibly compound) outer command,
        // accumulating environment variables from export/assignment segments.
        let mut accumulated_env: HashMap<String, String> = HashMap::new();
        // Whether the current segment is known to execute (for env accumulation).
        // The first segment always executes.
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
                        segment_executes = segment_executes
                            && is_likely_successful(&pipeline.segments[i - 1].command);
                    }
                    // Or / Pipe / PipeErr: can't guarantee execution or env propagation.
                    // Clear accumulated env: after || the prior segment succeeded
                    // (so this one is skipped) or failed (so its env isn't set).
                    // After | the left side runs in a subshell.
                    Operator::Or | Operator::Pipe | Operator::PipeErr => {
                        segment_executes = false;
                        accumulated_env.clear();
                    }
                }
            }

            let mut result = self.evaluate_single_with_env(&segment.command, &accumulated_env);

            // Accumulate env vars from this segment if it's known to execute.
            // Also remove any vars that are explicitly unset.
            if segment_executes {
                for (key, val) in extract_segment_env(&segment.command) {
                    accumulated_env.insert(key, val);
                }
                for var in extract_unset_vars(&segment.command) {
                    accumulated_env.remove(&var);
                }
            }

            // Propagate redirection from wrapping constructs (e.g. a for loop
            // with output redirection: `for ... done > file`).  The inner
            // command text won't contain the redirect, so evaluate_single
            // can't see it — escalate here.
            if result.decision == Decision::Allow
                && let Some(ref r) = segment.redirection
            {
                result.decision = Decision::Ask;
                result.reason =
                    format!("{} (escalated: wrapping {})", result.reason, r.description);
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

        // Build summary header
        let mut desc = Vec::new();
        if !pipeline.operators.is_empty() {
            let mut unique_ops: Vec<&str> = pipeline.operators.iter().map(|o| o.as_str()).collect();
            unique_ops.sort();
            unique_ops.dedup();
            desc.push(unique_ops.join(", "));
        }
        if !substitutions.is_empty() {
            desc.push(format!("{} substitution(s)", substitutions.len()));
        }
        let header = if desc.is_empty() {
            "compound command".into()
        } else {
            format!("compound command ({})", desc.join("; "))
        };

        RuleMatch {
            decision: strictest,
            reason: format!("{}:\n{}", header, reasons.join("\n")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clear `GIT_CONFIG_GLOBAL` from the process environment so the
    /// env-gate fallback in `env_satisfies` doesn't interfere.  Requires nextest.
    fn clear_git_env() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process env and requires nextest (cargo nextest run)"
        );
        unsafe { std::env::remove_var("GIT_CONFIG_GLOBAL") };
    }

    // ── is_likely_successful ──

    #[test]
    fn likely_success_export() {
        assert!(is_likely_successful("export FOO=bar"));
    }

    #[test]
    fn likely_success_export_multiple() {
        assert!(is_likely_successful("export A=1 B=2"));
    }

    #[test]
    fn likely_success_bare_assignment() {
        assert!(is_likely_successful("FOO=bar"));
    }

    #[test]
    fn likely_success_true() {
        assert!(is_likely_successful("true"));
    }

    #[test]
    fn likely_success_echo() {
        assert!(is_likely_successful("echo hello"));
    }

    #[test]
    fn likely_success_printf() {
        assert!(is_likely_successful("printf '%s\\n' hello"));
    }

    #[test]
    fn likely_success_export_with_subshell_is_not_likely() {
        // export FOO=$(cmd) — the substitution could fail
        assert!(!is_likely_successful("export FOO=__SUBST__"));
    }

    #[test]
    fn likely_success_echo_with_subshell_is_not_likely() {
        assert!(!is_likely_successful("echo __SUBST__"));
    }

    #[test]
    fn likely_success_bare_assignment_with_subshell_is_not_likely() {
        assert!(!is_likely_successful("FOO=__SUBST__"));
    }

    #[test]
    fn likely_success_unknown_command() {
        assert!(!is_likely_successful("some_command --flag"));
    }

    #[test]
    fn likely_success_git() {
        assert!(!is_likely_successful("git push"));
    }

    #[test]
    fn likely_success_rm() {
        assert!(!is_likely_successful("rm -rf /"));
    }

    // ── extract_segment_env ──

    #[test]
    fn extract_env_export_single() {
        let vars = extract_segment_env("export FOO=bar");
        assert_eq!(vars, vec![("FOO".into(), "bar".into())]);
    }

    #[test]
    fn extract_env_export_multiple() {
        let vars = extract_segment_env("export A=1 B=2");
        assert_eq!(
            vars,
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn extract_env_export_with_path() {
        let vars = extract_segment_env("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai");
        assert_eq!(
            vars,
            vec![("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into())]
        );
    }

    #[test]
    fn extract_env_bare_assignment() {
        let vars = extract_segment_env("FOO=bar");
        assert_eq!(vars, vec![("FOO".into(), "bar".into())]);
    }

    #[test]
    fn extract_env_export_no_value() {
        // `export FOO` (no =) should not extract anything
        let vars = extract_segment_env("export FOO");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_env_export_flags() {
        let vars = extract_segment_env("export -p");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_env_non_export() {
        let vars = extract_segment_env("git push");
        assert!(vars.is_empty());
    }

    // ── Compound command env accumulation (end-to-end via registry) ──

    /// Build a registry with git config_env gating enabled.
    fn registry_with_git_env_gate() -> CommandRegistry {
        let mut config = crate::config::Config::default_config();
        config.git.allowed_with_config = vec!["push".into(), "commit".into(), "add".into()];
        config
            .git
            .config_env
            .insert("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into());
        CommandRegistry::from_config(&config)
    }

    #[test]
    fn export_semicolon_git_push_allows() {
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main");
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn export_and_git_push_allows() {
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main");
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn multiple_exports_and_git_push_allows() {
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export PATH=/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn export_or_git_push_does_not_allow() {
        clear_git_env();
        // || means git push runs only if export failed → env not set
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai || git push origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn export_pipe_git_push_does_not_allow() {
        clear_git_env();
        // | means subshell boundary → env doesn't propagate
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai | git push origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn unknown_cmd_breaks_and_chain() {
        // unknown_cmd is not is_likely_successful, so && chain breaks
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && unknown_cmd && git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn semicolon_after_unknown_cmd_resumes_accumulation() {
        // ; resets segment_executes to true, so export after ; is accumulated
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "unknown_cmd ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
        // Note: still ASK because unknown_cmd itself is ASK (unrecognized),
        // and strictest-wins. Let's verify the git push part specifically.
    }

    #[test]
    fn semicolon_resumes_accumulation_all_known() {
        // echo is allowed AND likely_successful. After ;, export accumulates.
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "echo starting ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn bare_assignment_semicolon_git_push_allows() {
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main");
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn bare_assignment_and_git_push_allows() {
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main");
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn wrong_export_value_still_asks() {
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.wrong && git push origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn export_overridden_by_later_export() {
        let reg = registry_with_git_env_gate();
        // First export sets wrong value, second corrects it
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=wrong ; export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn or_after_export_clears_accumulated_env() {
        clear_git_env();
        // export A=1 && echo ok || export B=2 && git push
        // The || clears accumulated env (conservative: can't determine which
        // path was taken). git push doesn't see GIT_CONFIG_GLOBAL.
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo ok || export OTHER=x && git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn echo_and_export_and_git_push_allows() {
        // echo is likely_successful, export is likely_successful, chain holds
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "echo 'Pushing...' && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn realistic_claude_pattern() {
        // The actual pattern Claude generates
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export PATH=/home/user/.cargo/bin:/usr/bin && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && echo 'Pushing...' && git push -u origin feature-branch",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn force_push_still_asks_with_export() {
        // Force push flags should escalate even with correct env
        let reg = registry_with_git_env_gate();
        let result = reg
            .evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push --force origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn subshell_in_export_breaks_and_chain() {
        // export FOO=$(cmd) && git push — subshell makes export's success unpredictable,
        // so the && chain can't guarantee the next segment executes.
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=$(cat ~/.gitconfig.ai.path) && git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn subshell_in_echo_breaks_and_chain() {
        // echo $(cmd) && export FOO=bar && git push — echo with subshell is not
        // likely successful, breaking the chain for subsequent accumulation.
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "echo $(some_status_cmd) && export GIT_CONFIG_GLOBAL=~/.gitconfig.ai && git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    // ── unset ──

    #[test]
    fn unset_removes_accumulated_var() {
        clear_git_env();
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset GIT_CONFIG_GLOBAL ; git push origin main",
        );
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn unset_only_removes_named_var() {
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset OTHER_VAR ; git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    #[test]
    fn unset_f_does_not_remove_var() {
        // unset -f removes functions, not variables
        let reg = registry_with_git_env_gate();
        let result = reg.evaluate(
            "export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; unset -f GIT_CONFIG_GLOBAL ; git push origin main",
        );
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }

    // ── extract_unset_vars ──

    #[test]
    fn extract_unset_single() {
        assert_eq!(extract_unset_vars("unset FOO"), vec!["FOO"]);
    }

    #[test]
    fn extract_unset_multiple() {
        assert_eq!(extract_unset_vars("unset FOO BAR"), vec!["FOO", "BAR"]);
    }

    #[test]
    fn extract_unset_with_v_flag() {
        assert_eq!(extract_unset_vars("unset -v FOO"), vec!["FOO"]);
    }

    #[test]
    fn extract_unset_with_f_flag() {
        let result = extract_unset_vars("unset -f my_func");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_unset_mixed_flags() {
        // -f disables var unset, -v re-enables it
        assert_eq!(
            extract_unset_vars("unset -f my_func -v MY_VAR"),
            vec!["MY_VAR"]
        );
    }

    #[test]
    fn extract_unset_not_unset_cmd() {
        assert!(extract_unset_vars("export FOO=bar").is_empty());
    }

    // ── env -i wrapper ──

    #[test]
    fn env_i_clears_accumulated_env_for_wrapped_cmd() {
        clear_git_env();
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env -i git push origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn env_dash_clears_accumulated_env_for_wrapped_cmd() {
        clear_git_env();
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env - git push origin main");
        assert_eq!(result.decision, Decision::Ask, "reason: {}", result.reason);
    }

    #[test]
    fn env_without_i_passes_accumulated_env() {
        let reg = registry_with_git_env_gate();
        let result =
            reg.evaluate("export GIT_CONFIG_GLOBAL=~/.gitconfig.ai ; env git push origin main");
        assert_eq!(
            result.decision,
            Decision::Allow,
            "reason: {}",
            result.reason
        );
    }
}
