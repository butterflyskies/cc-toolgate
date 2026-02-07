use crate::commands::CommandSpec;
use crate::config::KubectlConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct KubectlSpec {
    read_only: Vec<String>,
    mutating: Vec<String>,
}

impl KubectlSpec {
    pub fn from_config(config: &KubectlConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            mutating: config.mutating.clone(),
        }
    }

    fn subcommand<'a>(ctx: &'a CommandContext) -> Option<&'a str> {
        ctx.words
            .iter()
            .skip(1)
            .find(|w| !w.starts_with('-'))
            .map(|s| s.as_str())
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
}
