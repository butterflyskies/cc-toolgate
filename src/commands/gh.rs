use crate::commands::CommandSpec;
use crate::config::GhConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct GhSpec {
    read_only: Vec<String>,
    mutating: Vec<String>,
}

impl GhSpec {
    pub fn from_config(config: &GhConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            mutating: config.mutating.clone(),
        }
    }

    /// Get the two-word subcommand (e.g. "pr list") and one-word fallback.
    fn subcommands(ctx: &CommandContext) -> (String, String) {
        let sub_two = if ctx.words.len() >= 3 {
            format!("{} {}", ctx.words[1], ctx.words[2])
        } else {
            String::new()
        };
        let sub_one = ctx
            .words
            .get(1)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        (sub_two, sub_one)
    }
}

impl CommandSpec for GhSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let (sub_two, sub_one) = Self::subcommands(ctx);

        let in_read_only = self.read_only.iter().any(|s| s == &sub_two)
            || self.read_only.iter().any(|s| s == &sub_one);
        if in_read_only {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("gh {sub_one} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only gh {sub_two}"),
            };
        }

        let in_mutating = self.mutating.iter().any(|s| s == &sub_two)
            || self.mutating.iter().any(|s| s == &sub_one);
        if in_mutating {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("gh {sub_two} requires confirmation"),
            };
        }

        RuleMatch {
            decision: Decision::Ask,
            reason: format!("gh {sub_one} requires confirmation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

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
    fn redir_pr_list() {
        assert_eq!(eval("gh pr list > /tmp/prs.txt"), Decision::Ask);
    }
}
