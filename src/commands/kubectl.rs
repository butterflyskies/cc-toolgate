use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

const KUBECTL_READ_ONLY: &[&str] = &[
    "get",
    "describe",
    "logs",
    "top",
    "explain",
    "api-resources",
    "api-versions",
    "version",
    "cluster-info",
];

const KUBECTL_MUTATING: &[&str] = &[
    "apply",
    "delete",
    "rollout",
    "scale",
    "autoscale",
    "patch",
    "replace",
    "create",
    "edit",
    "drain",
    "cordon",
    "uncordon",
    "taint",
    "exec",
    "run",
    "port-forward",
    "cp",
];

pub struct KubectlSpec;

impl KubectlSpec {
    fn subcommand<'a>(ctx: &'a CommandContext) -> Option<&'a str> {
        ctx.words
            .iter()
            .skip(1)
            .find(|w| !w.starts_with('-'))
            .map(|s| s.as_str())
    }
}

impl CommandSpec for KubectlSpec {
    fn names(&self) -> &[&str] {
        &["kubectl"]
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub_str = Self::subcommand(ctx).unwrap_or("?");

        if KUBECTL_READ_ONLY.contains(&sub_str) {
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

        if KUBECTL_MUTATING.contains(&sub_str) {
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

pub static KUBECTL_SPEC: KubectlSpec = KubectlSpec;

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        KUBECTL_SPEC.evaluate(&ctx).decision
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
}
