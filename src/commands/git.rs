use crate::commands::CommandSpec;
use crate::config::GitConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct GitSpec {
    read_only: Vec<String>,
    allowed_with_config: Vec<String>,
    config_env_var: String,
    force_push_flags: Vec<String>,
}

impl GitSpec {
    pub fn from_config(config: &GitConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            allowed_with_config: config.allowed_with_config.clone(),
            config_env_var: config.config_env_var.clone(),
            force_push_flags: config.force_push_flags.clone(),
        }
    }

    /// Extract the git subcommand word (e.g. "push" from "git push origin main").
    fn subcommand(ctx: &CommandContext) -> Option<String> {
        let mut iter = ctx.words.iter();
        for word in iter.by_ref() {
            if word == "git" {
                return iter.next().cloned();
            }
        }
        None
    }
}

impl CommandSpec for GitSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub = Self::subcommand(ctx);
        let sub_str = sub.as_deref().unwrap_or("?");

        // Force-push → ask regardless of config
        if sub_str == "push" {
            let flag_strs: Vec<&str> = self.force_push_flags.iter().map(|s| s.as_str()).collect();
            if ctx.has_any_flag(&flag_strs) {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: "git force-push requires confirmation".into(),
                };
            }
        }

        // Read-only git subcommands — always allowed
        if self.read_only.iter().any(|s| s == sub_str) {
            if let Some(ref r) = ctx.redirection {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("git {sub_str} with {}", r.description),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only git {sub_str}"),
            };
        }

        // Env-gated subcommands: allowed only when config_env_var is set and present
        if self.allowed_with_config.iter().any(|s| s == sub_str) {
            // Feature requires config_env_var to be configured
            if !self.config_env_var.is_empty() && ctx.has_env(&self.config_env_var) {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("git {sub_str} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("git {sub_str} with {}", self.config_env_var),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("git {sub_str} requires confirmation"),
            };
        }

        // --version check
        if ctx.has_flag("--version") && ctx.words.len() <= 3 {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "git --version".into(),
            };
        }

        RuleMatch {
            decision: Decision::Ask,
            reason: format!("git {sub_str} requires confirmation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GitConfig};

    fn default_spec() -> GitSpec {
        GitSpec::from_config(&Config::default_config().git)
    }

    fn eval(cmd: &str) -> Decision {
        let s = default_spec();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    /// Build a spec with env-gated config enabled (like a user's custom config).
    fn spec_with_env_gate() -> GitSpec {
        GitSpec::from_config(&GitConfig {
            read_only: vec!["status".into(), "log".into(), "diff".into(), "branch".into()],
            allowed_with_config: vec!["push".into(), "pull".into(), "add".into()],
            config_env_var: "GIT_CONFIG_GLOBAL".into(),
            force_push_flags: vec!["--force".into(), "-f".into(), "--force-with-lease".into()],
        })
    }

    fn eval_with_env_gate(cmd: &str) -> Decision {
        let s = spec_with_env_gate();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    // ── Default config: no env-gated commands ──

    #[test]
    fn default_push_asks() {
        assert_eq!(eval("git push origin main"), Decision::Ask);
    }

    #[test]
    fn default_push_with_env_still_asks() {
        // Default config has empty config_env_var, so env var presence doesn't help
        assert_eq!(
            eval("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn allow_log() {
        assert_eq!(eval("git log --oneline -10"), Decision::Allow);
    }

    #[test]
    fn allow_diff() {
        assert_eq!(eval("git diff HEAD~1"), Decision::Allow);
    }

    #[test]
    fn allow_branch() {
        assert_eq!(eval("git branch -a"), Decision::Allow);
    }

    #[test]
    fn allow_status() {
        assert_eq!(eval("git status"), Decision::Allow);
    }

    #[test]
    fn redir_log() {
        assert_eq!(eval("git log > /tmp/log.txt"), Decision::Ask);
    }

    // ── Custom config with env-gated commands ──

    #[test]
    fn env_gate_push_with_config() {
        assert_eq!(
            eval_with_env_gate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_push_no_config() {
        assert_eq!(eval_with_env_gate("git push origin main"), Decision::Ask);
    }

    #[test]
    fn env_gate_force_push() {
        assert_eq!(
            eval_with_env_gate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_commit_still_asks() {
        // commit is not in allowed_with_config
        assert_eq!(
            eval_with_env_gate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git commit -m 'test'"),
            Decision::Ask
        );
    }
}
