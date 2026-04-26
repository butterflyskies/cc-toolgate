//! Subcommand-aware GitHub CLI (gh) evaluation.
//!
//! gh uses two-word subcommands (`pr list`, `issue create`), so the spec
//! extracts both the two-word form and the one-word fallback, then tries
//! them in order against the matcher. The `allow` field supports future
//! gh subcommands that are safe but neither read-only nor env-gated.

use super::super::CommandSpec;
use crate::config::GhConfig;
use crate::eval::matcher::SubcommandMatcher;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Subcommand-aware gh CLI evaluator.
///
/// Evaluation order:
/// 1. Try two-word subcommand (`pr list`) through the matcher pipeline
/// 2. If matcher returns ASK on the two-word form, try one-word fallback (`pr`)
///    — only upgrades to ALLOW; ASK/Deny stays at two-word result
/// 3. Pipeline: read_only → allow → allowed_with_config → mutating → fallthrough
///
/// Note: the two-word-then-one-word probe order ensures that `pr list` in
/// `read_only` matches before `pr` alone could hit `mutating`. The one-word
/// fallback only fires when the two-word form isn't recognized, so it can
/// never override an explicit two-word classification.
pub struct GhSpec {
    matcher: SubcommandMatcher,
}

impl GhSpec {
    /// Build a gh spec from configuration.
    pub fn from_config(config: &GhConfig) -> Self {
        Self {
            matcher: SubcommandMatcher::new(
                config.read_only.clone(),
                config.allow.clone(),
                config.mutating.clone(),
                config.allowed_with_config.clone(),
                config.config_env.clone(),
            ),
        }
    }

    /// Global gh flags that consume the next word as their argument.
    const GLOBAL_ARG_FLAGS: &[&str] = &["--hostname", "-R", "--repo"];

    /// Extract subcommands, skipping global flags.
    /// Returns `(two_word, one_word)` like `("pr list", "pr")`.
    fn subcommands(ctx: &CommandContext) -> (String, String) {
        let mut iter = ctx.words.iter();
        for word in iter.by_ref() {
            if word == "gh" {
                break;
            }
        }
        let sub_one = loop {
            let word = match iter.next() {
                Some(w) => w,
                None => return (String::new(), "?".into()),
            };
            if Self::GLOBAL_ARG_FLAGS.contains(&word.as_str()) {
                iter.next();
                continue;
            }
            if word.starts_with('-') {
                continue;
            }
            break word.clone();
        };
        let sub_two = iter
            .find(|w| !w.starts_with('-'))
            .map(|w| format!("{sub_one} {w}"))
            .unwrap_or_default();
        (sub_two, sub_one)
    }
}

impl CommandSpec for GhSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        // `gh --version` has no subcommand word; the extractor returns "--version"
        // as sub_one, which never matches any config list. Handle it here — same
        // pattern as cargo's --version/-V pre-check.
        if ctx.has_flag("--version") {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "gh --version".into(),
                matched: true,
            };
        }

        let (sub_two, sub_one) = Self::subcommands(ctx);

        // Try two-word first, fall back to one-word only if unrecognized.
        if !sub_two.is_empty() {
            let two_result = self.matcher.evaluate(ctx, "gh", &sub_two);
            if two_result.decision == Decision::Allow {
                return two_result;
            }
            if !two_result.matched {
                let one_result = self.matcher.evaluate(ctx, "gh", &sub_one);
                if one_result.decision == Decision::Allow {
                    return one_result;
                }
            }
            return two_result;
        }

        self.matcher.evaluate(ctx, "gh", &sub_one)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GhConfig};
    use crate::eval::CommandContext;
    use std::collections::HashMap;

    fn spec() -> GhSpec {
        GhSpec::from_config(&Config::default_config().gh)
    }

    fn eval(cmd: &str) -> Decision {
        let s = spec();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn allow_pr_list() {
        assert_eq!(eval("gh pr list"), Decision::Allow);
    }

    #[test]
    fn allow_pr_view() {
        assert_eq!(eval("gh pr view 123"), Decision::Allow);
    }

    #[test]
    fn allow_status() {
        assert_eq!(eval("gh status"), Decision::Allow);
    }

    #[test]
    fn allow_api() {
        assert_eq!(eval("gh api repos/owner/repo/pulls"), Decision::Allow);
    }

    #[test]
    fn ask_pr_create() {
        assert_eq!(eval("gh pr create --title 'Fix'"), Decision::Ask);
    }

    #[test]
    fn ask_pr_merge() {
        assert_eq!(eval("gh pr merge 123"), Decision::Ask);
    }

    #[test]
    fn ask_repo_delete() {
        assert_eq!(eval("gh repo delete my-repo --yes"), Decision::Ask);
    }

    #[test]
    fn allow_version() {
        assert_eq!(eval("gh --version"), Decision::Allow);
    }

    #[test]
    fn redir_pr_list() {
        assert_eq!(eval("gh pr list > /tmp/prs.txt"), Decision::Ask);
    }

    // ── Env-gated commands ──

    fn spec_with_env_gate() -> GhSpec {
        GhSpec::from_config(&GhConfig {
            read_only: vec!["pr list".into(), "pr view".into(), "status".into()],
            allow: vec![],
            mutating: vec!["repo delete".into()],
            allowed_with_config: vec!["pr create".into(), "pr merge".into()],
            config_env: HashMap::from([("GH_CONFIG_DIR".into(), "~/.config/gh-ai".into())]),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_pr_create_with_matching_value() {
        assert_eq!(
            eval_with_env_gate("GH_CONFIG_DIR=~/.config/gh-ai gh pr create --title 'Fix'"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_pr_create_with_wrong_value() {
        assert_eq!(
            eval_with_env_gate("GH_CONFIG_DIR=~/.config/gh gh pr create --title 'Fix'"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_pr_create_no_config() {
        assert_eq!(
            eval_with_env_gate("gh pr create --title 'Fix'"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_pr_merge_with_config() {
        assert_eq!(
            eval_with_env_gate("GH_CONFIG_DIR=~/.config/gh-ai gh pr merge 123"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_pr_list_still_readonly() {
        // read_only commands don't need the env var
        assert_eq!(eval_with_env_gate("gh pr list"), Decision::Allow);
    }

    #[test]
    fn env_gate_repo_delete_still_asks() {
        // mutating commands not in allowed_with_config always ask
        assert_eq!(
            eval_with_env_gate("GH_CONFIG_DIR=~/.config/gh-ai gh repo delete my-repo"),
            Decision::Ask
        );
    }
}
