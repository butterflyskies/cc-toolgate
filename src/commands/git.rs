use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Git subcommands that are auto-allowed WITH AI gitconfig.
const GIT_ALLOWED_WITH_CONFIG: &[&str] = &["push", "pull", "status", "add"];

/// Git subcommands that are read-only and safe without AI gitconfig.
const GIT_READ_ONLY: &[&str] = &[
    "log",
    "diff",
    "show",
    "branch",
    "tag",
    "remote",
    "rev-parse",
    "ls-files",
    "ls-tree",
    "shortlog",
    "blame",
    "describe",
    "stash",
];

pub struct GitSpec;

impl GitSpec {
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
    fn names(&self) -> &[&str] {
        &["git"]
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let sub = Self::subcommand(ctx);
        let sub_str = sub.as_deref().unwrap_or("?");
        let has_ai_config = ctx.has_env("GIT_CONFIG_GLOBAL");

        // Force-push → ask regardless of config
        if sub_str == "push"
            && ctx.has_any_flag(&["--force", "--force-with-lease", "-f"])
        {
            return RuleMatch {
                decision: Decision::Ask,
                reason: "git force-push requires confirmation".into(),
            };
        }

        // Read-only git subcommands — allowed without AI gitconfig
        if GIT_READ_ONLY.contains(&sub_str) {
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
        if GIT_ALLOWED_WITH_CONFIG.contains(&sub_str) {
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

pub static GIT_SPEC: GitSpec = GitSpec;

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        GIT_SPEC.evaluate(&ctx).decision
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
