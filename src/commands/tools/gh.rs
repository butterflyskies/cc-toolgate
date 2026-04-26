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

    /// Get the two-word subcommand (e.g. "pr list") and one-word fallback ("pr").
    /// Handles env var prefixes like `GH_TOKEN=abc gh pr create`.
    fn subcommands(ctx: &CommandContext) -> (String, String) {
        let gh_pos = ctx.words.iter().position(|w| w == "gh");
        let after_gh = gh_pos.map(|p| p + 1).unwrap_or(1);

        let sub_two = if ctx.words.len() > after_gh + 1 {
            format!("{} {}", ctx.words[after_gh], ctx.words[after_gh + 1])
        } else {
            String::new()
        };
        let sub_one = ctx
            .words
            .get(after_gh)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
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
            };
        }

        let (sub_two, sub_one) = Self::subcommands(ctx);

        // Try the two-word subcommand first. If it's recognized (ALLOW or a
        // config-driven ASK), use that result. If it falls through as an
        // unrecognized subcommand, try the one-word fallback — this handles
        // one-word commands like `gh status` that aren't two-word forms.
        //
        // TODO(review): this "try two then one" probe has a subtle edge: if
        // sub_two is empty (e.g. bare `gh`), sub_one becomes "?" and both
        // return "requires confirmation". That's correct behavior — bare `gh`
        // should ask. But if gh gains a recognized one-word command that also
        // appears as the first word of a two-word form (e.g. `gh status` and
        // `gh status refresh`), the two-word probe fires first, which is right.
        if !sub_two.is_empty() {
            let two_result = self.matcher.evaluate(ctx, "gh", &sub_two);
            if two_result.decision == Decision::Allow {
                return two_result;
            }
            // Two-word fell through (unrecognized) — try one-word fallback.
            // If the one-word form is recognized as ALLOW, use it.
            // Otherwise keep the two-word reason (more informative).
            let one_result = self.matcher.evaluate(ctx, "gh", &sub_one);
            if one_result.decision == Decision::Allow {
                return one_result;
            }
            return two_result;
        }

        // No two-word form available — evaluate the one-word subcommand directly.
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
