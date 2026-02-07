use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// A data-driven command spec for flat allow/ask commands.
///
/// For allow commands: returns Allow unless output redirection is detected (→ Ask).
/// For ask commands: always returns Ask.
pub struct SimpleCommandSpec {
    decision: Decision,
}

impl SimpleCommandSpec {
    pub fn new(decision: Decision) -> Self {
        Self { decision }
    }
}

impl CommandSpec for SimpleCommandSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        match self.decision {
            Decision::Allow => {
                // Check for --version on any allowed command
                if ctx.words.len() <= 3 && ctx.has_flag("--version") {
                    return RuleMatch {
                        decision: Decision::Allow,
                        reason: format!("{} --version", ctx.base_command),
                    };
                }
                // Redirection escalates ALLOW → ASK
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("{} with {}", ctx.base_command, r.description),
                    };
                }
                RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("allowed: {}", ctx.base_command),
                }
            }
            Decision::Ask => RuleMatch {
                decision: Decision::Ask,
                reason: format!("{} requires confirmation", ctx.base_command),
            },
            Decision::Deny => RuleMatch {
                decision: Decision::Deny,
                reason: format!("blocked command: {}", ctx.base_command),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_simple() {
        let spec = SimpleCommandSpec::new(Decision::Allow);
        let ctx = CommandContext::from_command("ls -la");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Allow);
    }

    #[test]
    fn allow_with_redir() {
        let spec = SimpleCommandSpec::new(Decision::Allow);
        let ctx = CommandContext::from_command("ls > file.txt");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Ask);
    }

    #[test]
    fn ask_simple() {
        let spec = SimpleCommandSpec::new(Decision::Ask);
        let ctx = CommandContext::from_command("rm -rf /tmp");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Ask);
    }
}
