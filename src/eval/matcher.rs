//! Data-driven subcommand matcher for tool evaluators.
//!
//! [`SubcommandMatcher`] is a concrete struct that holds the five common
//! subcommand classification lists and implements the standard evaluation
//! pipeline. Tool evaluators (git, cargo, kubectl, gh) construct one from
//! their config and delegate to it, keeping tool-specific logic (subcommand
//! extraction, pre-checks like force-push detection) in the spec itself.
//!
//! ## Evaluation pipeline
//!
//! 1. `read_only` match → ALLOW (redirection escalates to ASK)
//! 2. `allow` match → ALLOW (redirection escalates to ASK)
//! 3. `allowed_with_config` match → ALLOW if `config_env` satisfied, else ASK
//! 4. `mutating` match → ASK
//! 5. Fallthrough → ASK

use crate::eval::{CommandContext, Decision, RuleMatch};
use std::collections::HashMap;

/// Shared subcommand evaluation logic for all tool evaluators.
///
/// Holds the five standard classification lists and applies the evaluation
/// pipeline. Tool specs construct a `SubcommandMatcher` from their config
/// and call [`evaluate`](Self::evaluate) after any tool-specific pre-checks.
pub(crate) struct SubcommandMatcher {
    /// Subcommands that are always allowed without conditions (e.g. `status`, `log`, `get`).
    read_only: Vec<String>,
    /// Subcommands that are unconditionally allowed (e.g. git-town local branch ops).
    allow: Vec<String>,
    /// Subcommands that always require user confirmation (e.g. `push`, `apply`, `delete`).
    mutating: Vec<String>,
    /// Subcommands allowed only when all `config_env` entries match.
    allowed_with_config: Vec<String>,
    /// Required env var name→value pairs that gate `allowed_with_config` subcommands.
    config_env: HashMap<String, String>,
}

impl SubcommandMatcher {
    /// Construct a matcher with the given classification lists.
    pub fn new(
        read_only: Vec<String>,
        allow: Vec<String>,
        mutating: Vec<String>,
        allowed_with_config: Vec<String>,
        config_env: HashMap<String, String>,
    ) -> Self {
        Self {
            read_only,
            allow,
            mutating,
            allowed_with_config,
            config_env,
        }
    }

    /// Format `config_env` keys for reason strings (e.g. `"GIT_CONFIG_GLOBAL"`).
    ///
    /// Keys are sorted for deterministic output.
    fn env_keys_display(&self) -> String {
        let mut keys: Vec<&str> = self.config_env.keys().map(|k| k.as_str()).collect();
        keys.sort();
        keys.join(", ")
    }

    /// Run the standard evaluation pipeline for a subcommand.
    ///
    /// `tool` is the tool name used in reason strings (e.g. `"git"`, `"cargo"`).
    /// `sub` is the extracted subcommand string (e.g. `"push"`, `"pr list"`).
    ///
    /// Evaluation order:
    /// 1. `read_only` match → ALLOW (redirection escalates to ASK)
    /// 2. `allow` match → ALLOW (redirection escalates to ASK)
    /// 3. `allowed_with_config` match → ALLOW if `config_env` satisfied, else ASK
    /// 4. `mutating` match → ASK
    /// 5. Fallthrough → ASK
    pub fn evaluate(&self, ctx: &CommandContext, tool: &str, sub: &str) -> RuleMatch {
        // 1. Read-only: always allowed, redirection escalates.
        if self.read_only.iter().any(|s| s == sub) {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("{tool} {sub} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only {tool} {sub}"),
            };
        }

        // 2. Unconditionally allowed, redirection escalates.
        if self.allow.iter().any(|s| s == sub) {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("{tool} {sub} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("allowed: {tool} {sub}"),
            };
        }

        // 3. Env-gated: allowed only when all config_env entries match.
        if self.allowed_with_config.iter().any(|s| s == sub) {
            if !self.config_env.is_empty() && ctx.env_satisfies(&self.config_env) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("{tool} {sub} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("{tool} {sub} with {}", self.env_keys_display()),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("{tool} {sub} requires confirmation"),
            };
        }

        // 4. Explicitly mutating → ASK.
        if self.mutating.iter().any(|s| s == sub) {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("{tool} {sub} requires confirmation"),
            };
        }

        // 5. Fallthrough → ASK.
        RuleMatch {
            decision: Decision::Ask,
            reason: format!("{tool} {sub} requires confirmation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::CommandContext;
    use std::collections::HashMap;

    /// Panic unless running under nextest (process-per-test isolation).
    fn require_nextest() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process env and requires nextest (cargo nextest run)"
        );
    }

    fn matcher_basic() -> SubcommandMatcher {
        SubcommandMatcher::new(
            vec!["status".into(), "log".into()],
            vec!["town hack".into()],
            vec!["push".into()],
            vec![],
            HashMap::new(),
        )
    }

    fn matcher_with_env_gate() -> SubcommandMatcher {
        SubcommandMatcher::new(
            vec!["status".into()],
            vec![],
            vec!["delete".into()],
            vec!["apply".into()],
            HashMap::from([("KUBECONFIG".into(), "~/.kube/config.ai".into())]),
        )
    }

    // ── read_only ──

    #[test]
    fn read_only_allows() {
        let m = matcher_basic();
        let ctx = CommandContext::from_command("git status");
        let r = m.evaluate(&ctx, "git", "status");
        assert_eq!(r.decision, Decision::Allow);
        assert!(r.reason.contains("read-only"));
    }

    #[test]
    fn read_only_with_redirection_asks() {
        let m = matcher_basic();
        let ctx = CommandContext::from_command("git log > /tmp/log.txt");
        let r = m.evaluate(&ctx, "git", "log");
        assert_eq!(r.decision, Decision::Ask);
    }

    // ── allow ──

    #[test]
    fn allow_list_allows() {
        let m = matcher_basic();
        let ctx = CommandContext::from_command("git town hack feature");
        let r = m.evaluate(&ctx, "git", "town hack");
        assert_eq!(r.decision, Decision::Allow);
        assert!(r.reason.contains("allowed:"));
    }

    #[test]
    fn allow_with_redirection_asks() {
        let m = SubcommandMatcher::new(
            vec![],
            vec!["safe-cmd".into()],
            vec![],
            vec![],
            HashMap::new(),
        );
        let ctx = CommandContext::from_command("tool safe-cmd > out.txt");
        let r = m.evaluate(&ctx, "tool", "safe-cmd");
        assert_eq!(r.decision, Decision::Ask);
    }

    // ── mutating ──

    #[test]
    fn mutating_asks() {
        let m = matcher_basic();
        let ctx = CommandContext::from_command("git push origin main");
        let r = m.evaluate(&ctx, "git", "push");
        assert_eq!(r.decision, Decision::Ask);
    }

    // ── fallthrough ──

    #[test]
    fn unknown_subcommand_asks() {
        let m = matcher_basic();
        let ctx = CommandContext::from_command("git unknown-sub");
        let r = m.evaluate(&ctx, "git", "unknown-sub");
        assert_eq!(r.decision, Decision::Ask);
        assert!(r.reason.contains("requires confirmation"));
    }

    // ── env-gated ──

    #[test]
    fn env_gated_with_matching_env_allows() {
        let m = matcher_with_env_gate();
        let ctx = CommandContext::from_command(
            "KUBECONFIG=~/.kube/config.ai kubectl apply -f deploy.yaml",
        );
        let r = m.evaluate(&ctx, "kubectl", "apply");
        assert_eq!(r.decision, Decision::Allow);
        assert!(r.reason.contains("KUBECONFIG"));
    }

    #[test]
    fn env_gated_with_wrong_env_asks() {
        let m = matcher_with_env_gate();
        let ctx = CommandContext::from_command("KUBECONFIG=~/.kube/config kubectl apply -f f.yaml");
        let r = m.evaluate(&ctx, "kubectl", "apply");
        assert_eq!(r.decision, Decision::Ask);
    }

    #[test]
    fn env_gated_without_env_asks() {
        require_nextest();
        // Requires process isolation: KUBECONFIG may be set in the real environment.
        unsafe { std::env::remove_var("KUBECONFIG") };
        let m = matcher_with_env_gate();
        let ctx = CommandContext::from_command("kubectl apply -f deploy.yaml");
        let r = m.evaluate(&ctx, "kubectl", "apply");
        assert_eq!(r.decision, Decision::Ask);
    }

    #[test]
    fn env_gated_with_redirection_asks() {
        let m = matcher_with_env_gate();
        let ctx = CommandContext::from_command(
            "KUBECONFIG=~/.kube/config.ai kubectl apply -f f.yaml > out",
        );
        let r = m.evaluate(&ctx, "kubectl", "apply");
        assert_eq!(r.decision, Decision::Ask);
    }

    // ── env_keys_display ──

    #[test]
    fn env_keys_display_sorted() {
        let m = SubcommandMatcher::new(
            vec![],
            vec![],
            vec![],
            vec!["deploy".into()],
            HashMap::from([
                ("ZEBRA_VAR".into(), "z".into()),
                ("ALPHA_VAR".into(), "a".into()),
            ]),
        );
        // Keys are sorted
        assert_eq!(m.env_keys_display(), "ALPHA_VAR, ZEBRA_VAR");
    }

    #[test]
    fn env_keys_display_empty() {
        let m = SubcommandMatcher::new(vec![], vec![], vec![], vec![], HashMap::new());
        assert_eq!(m.env_keys_display(), "");
    }
}
