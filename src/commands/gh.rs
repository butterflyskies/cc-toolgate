use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// gh CLI subcommands that are read-only.
const GH_READ_ONLY: &[&str] = &[
    "status",
    // repo
    "repo view",
    "repo list",
    "repo clone",
    // pr
    "pr list",
    "pr view",
    "pr diff",
    "pr checks",
    "pr status",
    // issue
    "issue list",
    "issue view",
    "issue status",
    // run / workflow
    "run list",
    "run view",
    "run watch",
    "workflow list",
    "workflow view",
    // release
    "release list",
    "release view",
    // misc read
    "search",
    "browse",
    "api",
    "auth status",
    "auth token",
    "extension list",
    "label list",
    "cache list",
    "variable list",
    "variable get",
    "secret list",
];

/// gh CLI subcommands that mutate state (create, modify, delete).
const GH_MUTATING: &[&str] = &[
    // repo
    "repo create",
    "repo delete",
    "repo edit",
    "repo fork",
    "repo rename",
    "repo archive",
    // pr
    "pr create",
    "pr merge",
    "pr close",
    "pr reopen",
    "pr comment",
    "pr review",
    "pr edit",
    // issue
    "issue create",
    "issue close",
    "issue reopen",
    "issue comment",
    "issue edit",
    "issue delete",
    "issue transfer",
    "issue pin",
    "issue unpin",
    // run / workflow
    "run rerun",
    "run cancel",
    "run delete",
    "workflow enable",
    "workflow disable",
    "workflow run",
    // release
    "release create",
    "release delete",
    "release edit",
    // misc write
    "auth login",
    "auth logout",
    "auth refresh",
    "extension install",
    "extension remove",
    "extension upgrade",
    "label create",
    "label edit",
    "label delete",
    "cache delete",
    "variable set",
    "variable delete",
    "secret set",
    "secret delete",
    "config set",
];

pub struct GhSpec;

impl GhSpec {
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

    fn in_set(set: &[&str], val: &str) -> bool {
        set.contains(&val)
    }
}

impl CommandSpec for GhSpec {
    fn names(&self) -> &[&str] {
        &["gh"]
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let (sub_two, sub_one) = Self::subcommands(ctx);

        if Self::in_set(GH_READ_ONLY, &sub_two) || Self::in_set(GH_READ_ONLY, &sub_one) {
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

        if Self::in_set(GH_MUTATING, &sub_two) || Self::in_set(GH_MUTATING, &sub_one) {
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

pub static GH_SPEC: GhSpec = GhSpec;

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        GH_SPEC.evaluate(&ctx).decision
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
