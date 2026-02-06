use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Cargo subcommands that are safe (build / check / informational).
const CARGO_SAFE: &[&str] = &[
    "build",
    "check",
    "test",
    "bench",
    "run",
    "clippy",
    "fmt",
    "doc",
    "clean",
    "update",
    "fetch",
    "tree",
    "metadata",
    "version",
    "verify-project",
    "search",
    "generate-lockfile",
];

pub struct CargoSpec;

impl CargoSpec {
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
    fn names(&self) -> &[&str] {
        &["cargo"]
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub_str = Self::subcommand(ctx).unwrap_or("?");

        if CARGO_SAFE.contains(&sub_str) {
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

pub static CARGO_SPEC: CargoSpec = CargoSpec;

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        CARGO_SPEC.evaluate(&ctx).decision
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
