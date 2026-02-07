use crate::commands::CommandSpec;
use crate::config::CargoConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct CargoSpec {
    safe_subcommands: Vec<String>,
}

impl CargoSpec {
    pub fn from_config(config: &CargoConfig) -> Self {
        Self {
            safe_subcommands: config.safe_subcommands.clone(),
        }
    }

    /// Extract the cargo subcommand (first non-flag word after "cargo").
    fn subcommand<'a>(ctx: &'a CommandContext) -> Option<&'a str> {
        ctx.words
            .iter()
            .skip(1)
            .find(|w| !w.starts_with('-'))
            .map(|s| s.as_str())
    }
}

impl CommandSpec for CargoSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub_str = Self::subcommand(ctx).unwrap_or("?");

        if self.safe_subcommands.iter().any(|s| s == sub_str) {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("cargo {sub_str} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("cargo {sub_str}"),
            };
        }

        // --version / -V at any position
        if ctx.has_any_flag(&["--version", "-V"]) {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "cargo --version".into(),
            };
        }

        RuleMatch {
            decision: Decision::Ask,
            reason: format!("cargo {sub_str} requires confirmation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn spec() -> CargoSpec {
        CargoSpec::from_config(&Config::default_config().cargo)
    }

    fn eval(cmd: &str) -> Decision {
        let s = spec();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn allow_build() {
        assert_eq!(eval("cargo build --release"), Decision::Allow);
    }

    #[test]
    fn allow_test() {
        assert_eq!(eval("cargo test"), Decision::Allow);
    }

    #[test]
    fn allow_clippy() {
        assert_eq!(eval("cargo clippy"), Decision::Allow);
    }

    #[test]
    fn allow_version() {
        assert_eq!(eval("cargo --version"), Decision::Allow);
    }

    #[test]
    fn allow_version_short() {
        assert_eq!(eval("cargo -V"), Decision::Allow);
    }

    #[test]
    fn ask_install() {
        assert_eq!(eval("cargo install ripgrep"), Decision::Ask);
    }

    #[test]
    fn ask_publish() {
        assert_eq!(eval("cargo publish"), Decision::Ask);
    }

    #[test]
    fn redir_build() {
        assert_eq!(eval("cargo build --release > /tmp/log"), Decision::Ask);
    }
}
