pub mod context;
pub mod decision;

pub use context::CommandContext;
pub use decision::{BaseDisposition, Decision, FlagDisposition, RuleMatch};

use std::collections::HashMap;

use crate::commands::CommandSpec;
use crate::config::Config;
use crate::parse;

/// Registry of all command specs, keyed by command name.
pub struct CommandRegistry {
    specs: HashMap<String, Box<dyn CommandSpec>>,
    escalate_deny: bool,
}

impl CommandRegistry {
    /// Build the registry from configuration.
    pub fn from_config(config: &Config) -> Self {
        use crate::commands::{
            cargo::CargoSpec,
            deny::DenyCommandSpec,
            gh::GhSpec,
            git::GitSpec,
            kubectl::KubectlSpec,
            simple::SimpleCommandSpec,
        };

        let mut specs: HashMap<String, Box<dyn CommandSpec>> = HashMap::new();

        // Deny commands (registered first, complex specs override if needed)
        for name in &config.commands.deny {
            specs.insert(name.clone(), Box::new(DenyCommandSpec));
        }

        // Allow commands
        for name in &config.commands.allow {
            specs.insert(name.clone(), Box::new(SimpleCommandSpec::new(Decision::Allow)));
        }

        // Ask commands
        for name in &config.commands.ask {
            specs.insert(name.clone(), Box::new(SimpleCommandSpec::new(Decision::Ask)));
        }

        // Complex command specs (override any simple entry for the same name)
        specs.insert("git".into(), Box::new(GitSpec::from_config(&config.git)));
        specs.insert("cargo".into(), Box::new(CargoSpec::from_config(&config.cargo)));
        specs.insert("kubectl".into(), Box::new(KubectlSpec::from_config(&config.kubectl)));
        specs.insert("gh".into(), Box::new(GhSpec::from_config(&config.gh)));

        Self {
            specs,
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
        let cmd = command.trim();
        if cmd.is_empty() {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "empty".into(),
            };
        }

        let ctx = CommandContext::from_command(cmd);

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

        // Simple case: no substitutions and not compound → evaluate directly
        if pipeline.segments.len() <= 1 && substitutions.is_empty() {
            return self.evaluate_single(command);
        }

        let mut worst = Decision::Allow;
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
            if result.decision > worst {
                worst = result.decision;
            }
        }

        // Evaluate each part of the (possibly compound) outer command
        for segment in &pipeline.segments {
            let result = self.evaluate_single(&segment.command);
            let label: String = segment.command.trim().chars().take(60).collect();
            reasons.push(format!(
                "  [{label}] -> {}: {}",
                result.decision.label(),
                result.reason
            ));
            if result.decision > worst {
                worst = result.decision;
            }
        }

        // Build summary header
        let mut desc = Vec::new();
        if !pipeline.operators.is_empty() {
            let mut unique_ops: Vec<&str> = pipeline
                .operators
                .iter()
                .map(|o| o.as_str())
                .collect();
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
            decision: worst,
            reason: format!("{}:\n{}", header, reasons.join("\n")),
        }
    }
}
