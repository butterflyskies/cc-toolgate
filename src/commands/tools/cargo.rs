//! Subcommand-aware cargo evaluation.
//!
//! Distinguishes safe subcommands (build, test, clippy) from mutating ones
//! (install, publish). Supports env-gated auto-allow and `--version`/`-V` detection.

use super::super::CommandSpec;
use crate::config::CargoConfig;
use crate::eval::matcher::SubcommandMatcher;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Subcommand-aware cargo evaluator.
///
/// Evaluation order:
/// 1. `--version` / `-V` at any position → ALLOW
///    (checked before subcommand extraction: `cargo -V` has no subcommand word)
/// 2. Safe subcommands (`safe_subcommands`) → ALLOW (redirection escalates to ASK)
/// 3. Read-only subcommands → ALLOW (redirection escalates to ASK)
/// 4. Env-gated subcommands → ALLOW if all `config_env` entries match, else ASK
/// 5. Mutating subcommands → ASK
/// 6. Everything else → ASK
///
/// Note: `--version`/`-V` is checked first because `cargo --version` produces no
/// subcommand word and would otherwise fall through to "requires confirmation".
/// The `has_any_flag` check is kept here (not in config) because it's a flag
/// pattern, not a subcommand, and doesn't fit the SubcommandMatcher model.
pub struct CargoSpec {
    matcher: SubcommandMatcher,
}

impl CargoSpec {
    /// Build a cargo spec from configuration.
    ///
    /// `safe_subcommands` and `read_only` are both unconditionally-allowed lists;
    /// they are combined into the matcher's `read_only` slot.
    pub fn from_config(config: &CargoConfig) -> Self {
        // Merge safe_subcommands + read_only into a single read_only list.
        let mut read_only = config.safe_subcommands.clone();
        for s in &config.read_only {
            if !read_only.contains(s) {
                read_only.push(s.clone());
            }
        }
        Self {
            matcher: SubcommandMatcher::new(
                read_only,
                vec![], // cargo has no unconditional "allow" tier between read_only and gated
                config.mutating.clone(),
                config.allowed_with_config.clone(),
                config.config_env.clone(),
            ),
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
        // Check --version/-V before extracting a subcommand: `cargo --version` and
        // `cargo -V` have no subcommand word and would otherwise fall through.
        // TODO(review): consider adding "--version" and "-V" as read_only subcommands
        // in config instead, but that requires the matcher to understand flag-as-subcommand.
        if ctx.has_any_flag(&["--version", "-V"]) {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "cargo --version".into(),
                matched: true,
            };
        }

        let sub = Self::subcommand(ctx).unwrap_or("?");
        self.matcher.evaluate(ctx, "cargo", sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CargoConfig, Config};
    use crate::eval::CommandContext;
    use std::collections::HashMap;

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
            read_only: vec![],
            mutating: vec![],
            allowed_with_config: vec!["install".into(), "publish".into()],
            config_env: HashMap::from([("CARGO_INSTALL_ROOT".into(), "/tmp/bin".into())]),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_install_with_matching_value() {
        assert_eq!(
            eval_with_env_gate("CARGO_INSTALL_ROOT=/tmp/bin cargo install ripgrep"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_install_with_wrong_value() {
        assert_eq!(
            eval_with_env_gate("CARGO_INSTALL_ROOT=/usr/local cargo install ripgrep"),
            Decision::Ask
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
