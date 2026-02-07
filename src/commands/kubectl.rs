use crate::commands::CommandSpec;
use crate::config::KubectlConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct KubectlSpec {
    read_only: Vec<String>,
    mutating: Vec<String>,
    allowed_with_config: Vec<String>,
    config_env_var: String,
}

impl KubectlSpec {
    pub fn from_config(config: &KubectlConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            mutating: config.mutating.clone(),
            allowed_with_config: config.allowed_with_config.clone(),
            config_env_var: config.config_env_var.clone(),
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
        let sub_str = Self::subcommand(ctx).unwrap_or("?");

        if self.read_only.iter().any(|s| s == sub_str) {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("kubectl {sub_str} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only kubectl {sub_str}"),
            };
        }

        // Env-gated subcommands: allowed only when config_env_var is set and present
        if self.allowed_with_config.iter().any(|s| s == sub_str) {
            if !self.config_env_var.is_empty() && ctx.has_env(&self.config_env_var) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("kubectl {sub_str} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("kubectl {sub_str} with {}", self.config_env_var),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("kubectl {sub_str} requires confirmation"),
            };
        }

        if self.mutating.iter().any(|s| s == sub_str) {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("kubectl {sub_str} requires confirmation"),
            };
        }

        RuleMatch {
            decision: Decision::Ask,
            reason: format!("kubectl {sub_str} requires confirmation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

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
            mutating: vec!["delete".into()],
            allowed_with_config: vec!["apply".into(), "rollout".into()],
            config_env_var: "KUBECONFIG".into(),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_apply_with_config() {
        assert_eq!(
            eval_with_env_gate("KUBECONFIG=~/.kube/staging kubectl apply -f deploy.yaml"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_apply_no_config() {
        assert_eq!(eval_with_env_gate("kubectl apply -f deploy.yaml"), Decision::Ask);
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
            eval_with_env_gate("KUBECONFIG=~/.kube/staging kubectl delete pod foo"),
            Decision::Ask
        );
    }
}
