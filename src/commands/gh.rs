use crate::commands::CommandSpec;
use crate::config::GhConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct GhSpec {
    read_only: Vec<String>,
    mutating: Vec<String>,
    allowed_with_config: Vec<String>,
    config_env_var: String,
}

impl GhSpec {
    pub fn from_config(config: &GhConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            mutating: config.mutating.clone(),
            allowed_with_config: config.allowed_with_config.clone(),
            config_env_var: config.config_env_var.clone(),
        }
    }

    /// Get the two-word subcommand (e.g. "pr list") and one-word fallback.
    /// Handles env var prefixes like `GH_TOKEN=abc gh pr create`.
    fn subcommands(ctx: &CommandContext) -> (String, String) {
        // Find position of "gh" in the word list (may be preceded by env vars)
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

        // Env-gated subcommands: allowed only when config_env_var is set and present
        let in_env_gated = self.allowed_with_config.iter().any(|s| s == &sub_two)
            || self.allowed_with_config.iter().any(|s| s == &sub_one);
        if in_env_gated {
            if !self.config_env_var.is_empty() && ctx.has_env(&self.config_env_var) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("gh {sub_one} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("gh {sub_two} with {}", self.config_env_var),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("gh {sub_two} requires confirmation"),
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

    // ── Env-gated commands ──

    fn spec_with_env_gate() -> GhSpec {
        GhSpec::from_config(&GhConfig {
            read_only: vec!["pr list".into(), "pr view".into(), "status".into()],
            mutating: vec!["repo delete".into()],
            allowed_with_config: vec!["pr create".into(), "pr merge".into()],
            config_env_var: "GH_TOKEN".into(),
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn env_gate_pr_create_with_config() {
        assert_eq!(
            eval_with_env_gate("GH_TOKEN=gho_abc123 gh pr create --title 'Fix'"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_pr_create_no_config() {
        assert_eq!(eval_with_env_gate("gh pr create --title 'Fix'"), Decision::Ask);
    }

    #[test]
    fn env_gate_pr_merge_with_config() {
        assert_eq!(
            eval_with_env_gate("GH_TOKEN=gho_abc123 gh pr merge 123"),
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
            eval_with_env_gate("GH_TOKEN=gho_abc123 gh repo delete my-repo"),
            Decision::Ask
        );
    }
}
