use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// A command spec that unconditionally denies execution.
pub struct DenyCommandSpec {
    names: &'static [&'static str],
}

impl CommandSpec for DenyCommandSpec {
    fn names(&self) -> &[&str] {
        self.names
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        RuleMatch {
            decision: Decision::Deny,
            reason: format!("blocked command: {}", ctx.base_command),
        }
    }
}

// Each denied command is listed explicitly
static SHRED: DenyCommandSpec = DenyCommandSpec {
    names: &["shred"],
};
static DD: DenyCommandSpec = DenyCommandSpec { names: &["dd"] };
static MKFS: DenyCommandSpec = DenyCommandSpec { names: &["mkfs"] };
static FDISK: DenyCommandSpec = DenyCommandSpec {
    names: &["fdisk"],
};
static PARTED: DenyCommandSpec = DenyCommandSpec {
    names: &["parted"],
};
static SHUTDOWN: DenyCommandSpec = DenyCommandSpec {
    names: &["shutdown"],
};
static REBOOT: DenyCommandSpec = DenyCommandSpec {
    names: &["reboot"],
};
static HALT: DenyCommandSpec = DenyCommandSpec {
    names: &["halt"],
};
static POWEROFF: DenyCommandSpec = DenyCommandSpec {
    names: &["poweroff"],
};
static EVAL: DenyCommandSpec = DenyCommandSpec {
    names: &["eval"],
};

pub static DENY_SPECS: &[&dyn CommandSpec] = &[
    &SHRED, &DD, &MKFS, &FDISK, &PARTED, &SHUTDOWN, &REBOOT, &HALT, &POWEROFF, &EVAL,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        for spec in DENY_SPECS {
            if spec.names().contains(&ctx.base_command.as_str()) {
                return spec.evaluate(&ctx).decision;
            }
        }
        Decision::Allow // not in deny list
    }

    #[test]
    fn deny_shred() {
        assert_eq!(eval("shred /dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_dd() {
        assert_eq!(eval("dd if=/dev/zero of=/dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_eval() {
        assert_eq!(eval("eval 'rm -rf /'"), Decision::Deny);
    }

    #[test]
    fn deny_shutdown() {
        assert_eq!(eval("shutdown -h now"), Decision::Deny);
    }

    #[test]
    fn deny_reboot() {
        assert_eq!(eval("reboot"), Decision::Deny);
    }

    #[test]
    fn deny_halt() {
        assert_eq!(eval("halt"), Decision::Deny);
    }

    #[test]
    fn deny_poweroff() {
        assert_eq!(eval("poweroff"), Decision::Deny);
    }

    #[test]
    fn not_denied() {
        assert_eq!(eval("ls -la"), Decision::Allow);
    }
}
