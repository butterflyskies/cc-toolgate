//! Subcommand-aware cargo evaluation.
//!
//! Distinguishes safe subcommands (build, test, clippy) from mutating ones
//! (install, publish). Supports env-gated auto-allow and `--version`/`-V` detection.

use super::super::CommandSpec;
use crate::config::CargoConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Subcommand-aware cargo evaluator.
///
/// Evaluation order:
/// 1. Safe subcommands → ALLOW (with redirection escalation)
/// 2. Env-gated subcommands → ALLOW if env var present, else ASK
/// 3. `--version` / `-V` → ALLOW
/// 4. Everything else → ASK
pub struct CargoSpec {
    /// Subcommands that are always safe (e.g. `build`, `test`, `check`).
    safe_subcommands: Vec<String>,
    /// Subcommands allowed only when `config_env_var` is present.
    allowed_with_config: Vec<String>,
    /// Env var name that gates `allowed_with_config` subcommands.
    config_env_var: String,
}

impl CargoSpec {
    /// Build a cargo spec from configuration.
    pub fn from_config(config: &CargoConfig) -> Self {
        Self {
            safe_subcommands: config.safe_subcommands.clone(),
            allowed_with_config: config.allowed_with_config.clone(),
            config_env_var: config.config_env_var.clone(),
        }
    }

    /// Extract the cargo subcommand (first non-flag word after "cargo").
    /// Handles env var prefixes like `CARGO_INSTALL_ROOT=/tmp cargo install`.
    fn subcommand<'a>(ctx: &'a CommandContext) -> Option<&'a str> {
        let mut iter = ctx.words.iter();
        for word in iter.by_ref() {
            if word == "cargo" {
                return iter.find(|w| !w.starts_with('-')).map(|s| s.as_str());
            }
        }
        None
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

        // Env-gated subcommands: allowed only when config_env_var is set and present
        if self.allowed_with_config.iter().any(|s| s == sub_str) {
            if !self.config_env_var.is_empty() && ctx.has_env(&self.config_env_var) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("cargo {sub_str} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("cargo {sub_str} with {}", self.config_env_var),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("cargo {sub_str} requires confirmation"),
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

    // ── Env-gated commands ──

    fn spec_with_env_gate() -> CargoSpec {
        CargoSpec::from_config(&CargoConfig {
            safe_subcommands: vec!["build".into(), "check".into(), "test".into()],
            allowed_with_config: vec!["install".into(), "publish".into()],
            config_env_var: "CARGO_INSTALL_ROOT".into(),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_install_with_config() {
        assert_eq!(
            eval_with_env_gate("CARGO_INSTALL_ROOT=/tmp/bin cargo install ripgrep"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_install_no_config() {
        assert_eq!(eval_with_env_gate("cargo install ripgrep"), Decision::Ask);
    }

    #[test]
    fn env_gate_publish_with_config() {
        assert_eq!(
            eval_with_env_gate("CARGO_INSTALL_ROOT=/tmp/bin cargo publish"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_build_still_safe_no_env() {
        // safe_subcommands don't need the env var
        assert_eq!(eval_with_env_gate("cargo build"), Decision::Allow);
    }
}
