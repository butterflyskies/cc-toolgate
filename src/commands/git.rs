use crate::commands::CommandSpec;
use crate::config::GitConfig;
use crate::eval::{CommandContext, Decision, RuleMatch};

pub struct GitSpec {
    read_only: Vec<String>,
    allowed_with_config: Vec<String>,
    force_push_flags: Vec<String>,
}

impl GitSpec {
    pub fn from_config(config: &GitConfig) -> Self {
        Self {
            read_only: config.read_only.clone(),
            allowed_with_config: config.allowed_with_config.clone(),
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
        let has_ai_config = ctx.has_env("GIT_CONFIG_GLOBAL");

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

        // Read-only git subcommands — allowed without AI gitconfig
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

        // Write git subcommands require AI gitconfig
        if self.allowed_with_config.iter().any(|s| s == sub_str) {
            if has_ai_config {
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("git {sub_str} with {}", r.description),
                    };
                }
                return RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("git {sub_str} with AI gitconfig"),
                };
            }
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("git {sub_str} without AI gitconfig"),
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
    use crate::config::Config;

    fn spec() -> GitSpec {
        GitSpec::from_config(&Config::default_config().git)
    }

    fn eval(cmd: &str) -> Decision {
        let s = spec();
        let ctx = CommandContext::from_command(cmd);
        s.evaluate(&ctx).decision
    }

    #[test]
    fn allow_push_with_config() {
        assert_eq!(
            eval("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn ask_push_no_config() {
        assert_eq!(eval("git push origin main"), Decision::Ask);
    }

    #[test]
    fn ask_force_push() {
        assert_eq!(
            eval("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force origin main"),
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
    fn ask_commit() {
        assert_eq!(
            eval("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git commit -m 'test'"),
            Decision::Ask
        );
    }

    #[test]
    fn redir_log() {
        assert_eq!(eval("git log > /tmp/log.txt"), Decision::Ask);
    }
}
