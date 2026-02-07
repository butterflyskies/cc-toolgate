pub mod context;
pub mod decision;

pub use context::CommandContext;
pub use decision::{BaseDisposition, Decision, FlagDisposition, RuleMatch};

use std::collections::HashMap;

use crate::commands::CommandSpec;
use crate::parse;

/// Registry of all command specs, keyed by command name.
pub struct CommandRegistry {
    specs: HashMap<&'static str, &'static dyn CommandSpec>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
        }
    }

    pub fn register(&mut self, spec: &'static dyn CommandSpec) {
        for name in spec.names() {
            self.specs.insert(name, spec);
        }
    }

    /// Look up a spec by exact command name.
    pub fn get(&self, name: &str) -> Option<&&'static dyn CommandSpec> {
        self.specs.get(name)
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
            return spec.evaluate(&ctx);
        }

        // Dotted command fallback for deny list (e.g. mkfs.ext4 → mkfs)
        if let Some(prefix) = ctx.base_command.split('.').next()
            && prefix != ctx.base_command
            && let Some(spec) = self.get(prefix)
        {
            return spec.evaluate(&ctx);
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

/// Build the default registry with all command specs.
pub fn default_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();

    // Register all command specs
    for spec in crate::commands::deny::DENY_SPECS.iter() {
        registry.register(*spec);
    }
    registry.register(&crate::commands::git::GIT_SPEC);
    registry.register(&crate::commands::cargo::CARGO_SPEC);
    registry.register(&crate::commands::kubectl::KUBECTL_SPEC);
    registry.register(&crate::commands::gh::GH_SPEC);
    for spec in crate::commands::simple::ALLOW_SPECS.iter() {
        registry.register(*spec);
    }
    for spec in crate::commands::simple::ASK_SPECS.iter() {
        registry.register(*spec);
    }

    registry
}
