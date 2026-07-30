//! KB-backed command evaluation using `agent_command_knowledge::classify()`.
//!
//! Replaces per-tool specs (GitSpec, CargoSpec, GhSpec, KubectlSpec) in the
//! registry with a single unified evaluator that delegates subcommand resolution
//! and flag analysis to the knowledge base, then applies cc-toolgate policy:
//! env-gating, redirection escalation, version flag detection, and escalation
//! flag detection.

use agent_command_knowledge::{
    CommandInfo, Effect, KnowledgeBase, classify, default_knowledge_base,
};
use agent_shell_parser::parse::Word;

use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

use std::collections::HashMap;

/// A unified command spec backed by the knowledge base.
///
/// Handles all KB-known commands (git, cargo, gh, kubectl, etc.) through a
/// single evaluation path: classify via KB, then apply cc-toolgate policy.
#[derive(Clone)]
pub struct KnowledgeSpec {
    kb: &'static KnowledgeBase,
    /// Per-command env-gate config: command name -> (allowed_with_config subcommands, config_env).
    /// When a subcommand appears in `allowed_with_config` and all `config_env` entries match,
    /// the command is allowed even if the KB classifies it as Mutating.
    env_gates: HashMap<String, EnvGateConfig>,
    /// Per-command force-push flags that override env-gating.
    /// e.g. git push --force always asks even with correct env.
    force_flags: HashMap<String, Vec<String>>,
    /// Per-command unconditional allow-overrides from config.
    /// Subcommands in these lists are allowed regardless of KB classification.
    /// Populated from: git.read_only, git.allow, cargo.safe_subcommands,
    /// kubectl.read_only, gh.read_only.
    allow_overrides: HashMap<String, Vec<String>>,
}

/// Env-gate config for a specific command (mirrors cc-toolgate's config model).
#[derive(Clone)]
struct EnvGateConfig {
    /// Subcommands that can be unlocked by env vars.
    allowed_with_config: Vec<String>,
    /// Required env var name→value pairs. All must match (AND).
    config_env: HashMap<String, String>,
}

impl KnowledgeSpec {
    /// Build a KnowledgeSpec from cc-toolgate configuration.
    ///
    /// Extracts env-gate config and unconditional allow-overrides from the
    /// per-tool sections to layer on top of the KB's base classification.
    pub fn from_config(config: &crate::config::Config) -> Self {
        let kb = default_knowledge_base();
        let mut env_gates = HashMap::new();
        let mut force_flags = HashMap::new();
        let mut allow_overrides: HashMap<String, Vec<String>> = HashMap::new();

        // Git env-gating
        if !config.git.config_env.is_empty() {
            env_gates.insert(
                "git".to_string(),
                EnvGateConfig {
                    allowed_with_config: config.git.allowed_with_config.clone(),
                    config_env: config.git.config_env.clone(),
                },
            );
        }
        if !config.git.force_push_flags.is_empty() {
            force_flags.insert("git".to_string(), config.git.force_push_flags.clone());
        }

        // Git unconditional allow-overrides: read_only + allow
        {
            let mut git_overrides = config.git.read_only.clone();
            for item in &config.git.allow {
                if !git_overrides.contains(item) {
                    git_overrides.push(item.clone());
                }
            }
            if !git_overrides.is_empty() {
                allow_overrides.insert("git".to_string(), git_overrides);
            }
        }

        // Cargo env-gating
        if !config.cargo.config_env.is_empty() {
            env_gates.insert(
                "cargo".to_string(),
                EnvGateConfig {
                    allowed_with_config: config.cargo.allowed_with_config.clone(),
                    config_env: config.cargo.config_env.clone(),
                },
            );
        }

        // Cargo unconditional allow-overrides: safe_subcommands
        if !config.cargo.safe_subcommands.is_empty() {
            allow_overrides.insert("cargo".to_string(), config.cargo.safe_subcommands.clone());
        }

        // kubectl env-gating
        if !config.kubectl.config_env.is_empty() {
            env_gates.insert(
                "kubectl".to_string(),
                EnvGateConfig {
                    allowed_with_config: config.kubectl.allowed_with_config.clone(),
                    config_env: config.kubectl.config_env.clone(),
                },
            );
        }

        // kubectl unconditional allow-overrides: read_only
        if !config.kubectl.read_only.is_empty() {
            allow_overrides.insert("kubectl".to_string(), config.kubectl.read_only.clone());
        }

        // gh env-gating
        if !config.gh.config_env.is_empty() {
            env_gates.insert(
                "gh".to_string(),
                EnvGateConfig {
                    allowed_with_config: config.gh.allowed_with_config.clone(),
                    config_env: config.gh.config_env.clone(),
                },
            );
        }

        // gh unconditional allow-overrides: read_only
        if !config.gh.read_only.is_empty() {
            allow_overrides.insert("gh".to_string(), config.gh.read_only.clone());
        }

        Self {
            kb,
            env_gates,
            force_flags,
            allow_overrides,
        }
    }

    /// Check if any word matches the version flag(s) for a command.
    fn is_version_invocation(&self, ctx: &CommandContext) -> bool {
        if let Some(cmd_knowledge) = self.kb.commands.get(ctx.base_command.as_str())
            && let Some(ref version_flag) = cmd_knowledge.properties.version_flag
            && ctx.has_flag(version_flag)
        {
            // For git: --version must be the only meaningful arg
            // (existing behavior: ctx.words.len() <= 3)
            if ctx.base_command == "git" {
                return ctx.words.len() <= 3;
            }
            return true;
        }
        // Cargo also accepts -V as a version flag
        if ctx.base_command == "cargo" && ctx.has_flag("-V") {
            return true;
        }
        false
    }

    /// Extract the subcommand string for reason messages.
    /// Uses the KB classification result's subcommand, or falls back to "?".
    fn subcommand_display<'a>(&self, info: &'a CommandInfo, ctx: &'a CommandContext) -> &'a str {
        if let Some(ref sub) = info.subcommand {
            // Full subcommand string (may be multi-word, e.g. "pr create")
            sub.as_str()
        } else {
            // Fallback: find first non-flag word after the command
            let base_word = Word::from(ctx.base_command.as_str());
            let suffix = format!("/{}", ctx.base_command);
            let cmd_idx = ctx
                .words
                .iter()
                .position(|w| *w == base_word || w.ends_with(suffix.as_str()))
                .map(|i| i + 1)
                .unwrap_or(0);
            ctx.words.get(cmd_idx).map(|w| w.as_str()).unwrap_or("?")
        }
    }

    /// Format config_env keys for reason strings.
    fn env_keys_display(config_env: &HashMap<String, String>) -> String {
        let mut keys: Vec<&str> = config_env.keys().map(|k| k.as_str()).collect();
        keys.sort();
        keys.join(", ")
    }
}

impl CommandSpec for KnowledgeSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        // Classify via KB
        let base_word = Word::from(ctx.base_command.as_str());
        let words: Vec<Word> = ctx.words.iter().map(|w| Word::from(w.as_str())).collect();
        let info = classify(&base_word, &words, self.kb);

        let cmd = &ctx.base_command;
        let sub_display = self.subcommand_display(&info, ctx);

        // Force-push flags: always Ask regardless of env-gating.
        // Must be checked BEFORE env-gating can unlock Allow.
        if let Some(force_flags) = self.force_flags.get(cmd.as_str())
            && info.subcommand.as_deref() == Some("push")
        {
            let flag_strs: Vec<&str> = force_flags.iter().map(|s| s.as_str()).collect();
            if ctx.has_any_flag(&flag_strs) {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("{cmd} force-push requires confirmation"),
                };
            }
        }

        // Config allow-overrides: if the subcommand is in the per-tool config
        // lists (git.read_only, git.allow, cargo.safe_subcommands, kubectl.read_only,
        // gh.read_only), override the KB classification to Allow.
        if let Some(overrides) = self.allow_overrides.get(cmd.as_str()) {
            let sub_str = info.subcommand.as_deref().unwrap_or("");
            if overrides.iter().any(|s| s == sub_str) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("{cmd} {sub_display} with {r}"),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("{cmd} {sub_display} (config override)"),
                };
            }
        }

        // Map KB effect to base decision.
        // cc-toolgate policy: ReadOnly -> Allow, everything else -> Ask.
        // (Deny is only for top-level deny-list commands via SimpleCommandSpec.)
        let base_decision = match info.effect {
            Effect::ReadOnly => Decision::Allow,
            Effect::Mutating | Effect::Destructive | Effect::Unknown => Decision::Ask,
        };

        // ReadOnly subcommands: Allow with redirection escalation
        if base_decision == Decision::Allow {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("{cmd} {sub_display} with {r}"),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only {cmd} {sub_display}"),
            };
        }

        // Non-readonly: check env-gating
        if let Some(gate_config) = self.env_gates.get(cmd.as_str()) {
            // Check if this subcommand is in allowed_with_config
            let sub_str = info.subcommand.as_deref().unwrap_or("");
            let is_gated = gate_config.allowed_with_config.iter().any(|s| s == sub_str);

            if is_gated
                && !gate_config.config_env.is_empty()
                && ctx.env_satisfies(&gate_config.config_env)
            {
                // Env gate satisfied: Allow with redirection escalation
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("{cmd} {sub_display} with {r}"),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!(
                        "{cmd} {sub_display} with {}",
                        Self::env_keys_display(&gate_config.config_env)
                    ),
                };
            }
        }

        // Version flag check: allows the command regardless
        if self.is_version_invocation(ctx) {
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("{cmd} --version"),
            };
        }

        // Default: Ask
        RuleMatch {
            decision: Decision::Ask,
            reason: format!("{cmd} {sub_display} requires confirmation"),
        }
    }
}
