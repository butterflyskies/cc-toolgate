//! Subcommand-aware kubectl evaluation.
//!
//! Distinguishes read-only subcommands (get, describe, logs) from mutating ones
//! (apply, delete, scale). Supports env-gated auto-allow for subcommands
//! like `apply` when specific environment variables match.

use super::super::CommandSpec;
use crate::config::KubectlConfig;
use crate::eval::matcher::SubcommandMatcher;
use crate::eval::{CommandContext, RuleMatch};

/// Subcommand-aware kubectl evaluator.
///
/// Evaluation order:
/// 1. Read-only subcommands → ALLOW (with redirection escalation)
/// 2. Unconditionally-allowed subcommands → ALLOW (with redirection escalation)
/// 3. Env-gated subcommands → ALLOW if all `config_env` entries match, else ASK
/// 4. Known mutating subcommands → ASK
/// 5. Everything else → ASK
pub struct KubectlSpec {
    matcher: SubcommandMatcher,
}

impl KubectlSpec {
    /// Build a kubectl spec from configuration.
    pub fn from_config(config: &KubectlConfig) -> Self {
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

    /// Extract the kubectl subcommand (first non-flag word after "kubectl").
    /// Handles env var prefixes like `KUBECONFIG=~/.kube/staging kubectl apply`.
    fn subcommand<'a>(ctx: &'a CommandContext) -> Option<&'a str> {
        let mut iter = ctx.words.iter();
        for word in iter.by_ref() {
            if word == "kubectl" {
                return iter.find(|w| !w.starts_with('-')).map(|s| s.as_str());
            }
        }
        None
    }
}

impl CommandSpec for KubectlSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub = Self::subcommand(ctx).unwrap_or("?");
        self.matcher.evaluate(ctx, "kubectl", sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, KubectlConfig};
    use crate::eval::{CommandContext, Decision};
    use std::collections::HashMap;

    /// Clear `KUBECONFIG` from the process environment so the env-gate
    /// fallback in `env_satisfies` doesn't interfere.  Requires nextest.
    fn clear_kubectl_env() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process env and requires nextest (cargo nextest run)"
        );
        unsafe { std::env::remove_var("KUBECONFIG") };
    }

    fn spec() -> KubectlSpec {
        KubectlSpec::from_config(&Config::default_config().kubectl)
    }

    fn eval(cmd: &str) -> Decision {
        let s = spec();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn allow_get() {
        assert_eq!(eval("kubectl get pods"), Decision::Allow);
    }

    #[test]
    fn allow_describe() {
        assert_eq!(eval("kubectl describe svc foo"), Decision::Allow);
    }

    #[test]
    fn allow_logs() {
        assert_eq!(eval("kubectl logs pod/foo"), Decision::Allow);
    }

    #[test]
    fn ask_apply() {
        assert_eq!(eval("kubectl apply -f deploy.yaml"), Decision::Ask);
    }

    #[test]
    fn ask_delete() {
        assert_eq!(eval("kubectl delete pod foo"), Decision::Ask);
    }

    #[test]
    fn redir_get() {
        assert_eq!(eval("kubectl get pods > pods.txt"), Decision::Ask);
    }

    // ── Env-gated commands ──

    fn spec_with_env_gate() -> KubectlSpec {
        KubectlSpec::from_config(&KubectlConfig {
            read_only: vec!["get".into(), "describe".into()],
            allow: vec![],
            mutating: vec!["delete".into()],
            allowed_with_config: vec!["apply".into(), "rollout".into()],
            config_env: HashMap::from([("KUBECONFIG".into(), "~/.kube/config.ai".into())]),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_apply_with_matching_value() {
        assert_eq!(
            eval_with_env_gate("KUBECONFIG=~/.kube/config.ai kubectl apply -f deploy.yaml"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_apply_with_wrong_value() {
        assert_eq!(
            eval_with_env_gate("KUBECONFIG=~/.kube/config kubectl apply -f deploy.yaml"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_apply_no_config() {
        clear_kubectl_env();
        assert_eq!(
            eval_with_env_gate("kubectl apply -f deploy.yaml"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_get_still_readonly() {
        // read_only commands don't need the env var
        assert_eq!(eval_with_env_gate("kubectl get pods"), Decision::Allow);
    }

    #[test]
    fn env_gate_delete_still_asks() {
        // mutating commands not in allowed_with_config always ask
        assert_eq!(
            eval_with_env_gate("KUBECONFIG=~/.kube/config.ai kubectl delete pod foo"),
            Decision::Ask
        );
    }
}
