use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// A command spec that unconditionally denies execution.
pub struct DenyCommandSpec;

impl CommandSpec for DenyCommandSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        RuleMatch {
            decision: Decision::Deny,
            reason: format!("blocked command: {}", ctx.base_command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies() {
        let spec = DenyCommandSpec;
        let ctx = CommandContext::from_command("shred /dev/sda");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Deny);
    }
}
