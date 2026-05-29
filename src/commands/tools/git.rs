//! Subcommand-aware git evaluation.
//!
//! Handles global flags (`-C`, `--no-pager`, etc.) to correctly extract the
//! subcommand, distinguishes read-only from mutating operations, supports
//! env-gated auto-allow for configured subcommands, and detects force-push flags.

use super::super::CommandSpec;
use crate::config::GitConfig;
use crate::eval::matcher::SubcommandMatcher;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// Subcommand-aware git evaluator.
///
/// Evaluation order:
/// 1. Force-push flags on `push` → always ASK
/// 2. Read-only subcommands → ALLOW (redirection escalates to ASK)
/// 3. Unconditionally-allowed subcommands → ALLOW (redirection escalates to ASK)
/// 4. Env-gated subcommands → ALLOW if all `config_env` entries match, else ASK
/// 5. Explicitly mutating subcommands → ASK
/// 6. Everything else → ASK
///
/// `--version` lives in `read_only` (in config), so `git --version` is
/// handled by step 2. Redirection on `git --version > file` escalates to
/// ASK, which is correct behavior.
// TODO(review): --version moved from post-pipeline special case to config.read_only.
// The old check had `words.len() <= 3` guard; read_only has no such guard, but
// `git --version --extra-arg` is harmless to allow. Revisit if that matters.
pub struct GitSpec {
    matcher: SubcommandMatcher,
    /// Flags indicating force-push (always ASK regardless of env-gating).
    force_push_flags: Vec<String>,
}

impl GitSpec {
    /// Build a git spec from configuration.
    pub fn from_config(config: &GitConfig) -> Self {
        Self {
            matcher: SubcommandMatcher::new(
                config.read_only.clone(),
                config.allow.clone(),
                config.mutating.clone(),
                config.allowed_with_config.clone(),
                config.config_env.clone(),
            ),
            force_push_flags: config.force_push_flags.clone(),
        }
    }

    /// Global git flags that consume the next word as their argument.
    /// These appear before the subcommand: `git -C /path status`.
    const GLOBAL_ARG_FLAGS: &[&str] = &["-C", "-c", "--git-dir", "--work-tree", "--namespace"];

    /// Global git flags that are standalone (no argument consumed).
    const GLOBAL_SOLO_FLAGS: &[&str] = &[
        "--bare",
        "--no-pager",
        "--no-replace-objects",
        "--literal-pathspecs",
        "--glob-pathspecs",
        "--noglob-pathspecs",
        "--icase-pathspecs",
        "--no-optional-locks",
    ];

    /// Extract the git subcommand, returning both a two-word form and one-word form.
    ///
    /// Returns `(two_word, one_word)` where `two_word` is `"<sub> <next>"` (e.g.
    /// `"town hack"`) and `one_word` is just `"<sub>"` (e.g. `"town"`). When
    /// there is no following word, `two_word` is empty.
    ///
    /// This enables matching multi-word plugin subcommands like `git town hack`
    /// against config entries like `"town hack"`, using the same two-word probe
    /// pattern as the gh evaluator.
    ///
    /// Skips global flags like `-C <path>` that appear before the subcommand.
    fn subcommands(ctx: &CommandContext) -> (String, String) {
        let mut iter = ctx.words.iter();
        // Advance past env vars to find "git"
        for word in iter.by_ref() {
            if word == "git" {
                break;
            }
        }
        // Skip global flags to find the subcommand
        let sub_one = loop {
            let word = match iter.next() {
                Some(w) => w,
                None => return (String::new(), "?".into()),
            };
            if Self::GLOBAL_ARG_FLAGS.contains(&word.as_str()) {
                iter.next(); // consume flag argument
                continue;
            }
            if Self::GLOBAL_SOLO_FLAGS.contains(&word.as_str()) {
                continue;
            }
            break word.clone();
        };
        // Build two-word form if a next word exists (and it's not a flag)
        let sub_two = iter
            .find(|w| !w.starts_with('-'))
            .map(|w| format!("{sub_one} {w}"))
            .unwrap_or_default();
        (sub_two, sub_one)
    }
}

impl CommandSpec for GitSpec {
    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        let (sub_two, sub_one) = Self::subcommands(ctx);

        // Force-push flags on `push` → ask regardless of config.
        if sub_one == "push" {
            let flag_strs: Vec<&str> = self.force_push_flags.iter().map(|s| s.as_str()).collect();
            if ctx.has_any_flag(&flag_strs) {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: "git force-push requires confirmation".into(),
                    matched: true,
                };
            }
        }

        // Try two-word subcommand first (e.g. "town hack"), fall back to one-word
        // (e.g. "town") only if the two-word form was unrecognized. If the two-word
        // form explicitly matched any list (including mutating), that's authoritative
        // — the one-word fallback must not override it.
        if !sub_two.is_empty() {
            let two_result = self.matcher.evaluate(ctx, "git", &sub_two);
            if two_result.decision == Decision::Allow {
                return two_result;
            }
            if !two_result.matched {
                let one_result = self.matcher.evaluate(ctx, "git", &sub_one);
                if one_result.decision == Decision::Allow {
                    return one_result;
                }
            }
            return two_result;
        }

        self.matcher.evaluate(ctx, "git", &sub_one)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, GitConfig};
    use crate::eval::CommandContext;
    use std::collections::HashMap;

    /// Clear `GIT_CONFIG_GLOBAL` from the process environment so the
    /// env-gate fallback in `env_satisfies` doesn't interfere with tests
    /// that assert "no config → Ask".  Requires nextest (process-per-test).
    fn clear_git_env() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process env and requires nextest (cargo nextest run)"
        );
        unsafe { std::env::remove_var("GIT_CONFIG_GLOBAL") };
    }

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
            read_only: vec![
                "status".into(),
                "log".into(),
                "diff".into(),
                "branch".into(),
            ],
            allow: vec![],
            mutating: vec![],
            allowed_with_config: vec!["push".into(), "pull".into(), "add".into()],
            config_env: HashMap::from([("GIT_CONFIG_GLOBAL".into(), "~/.gitconfig.ai".into())]),
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
        // Default config has empty config_env, so env var presence doesn't help
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
    fn allow_version() {
        assert_eq!(eval("git --version"), Decision::Allow);
    }

    #[test]
    fn redir_log() {
        assert_eq!(eval("git log > /tmp/log.txt"), Decision::Ask);
    }

    // ── Custom config with env-gated commands ──

    #[test]
    fn env_gate_push_with_matching_value() {
        assert_eq!(
            eval_with_env_gate("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn env_gate_push_with_wrong_value() {
        assert_eq!(
            eval_with_env_gate("GIT_CONFIG_GLOBAL=~/.gitconfig git push origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn env_gate_push_no_config() {
        clear_git_env();
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

    // ── Global flag skipping (-C, -c, etc.) ──

    #[test]
    fn allow_git_c_dir_status() {
        assert_eq!(eval("git -C /some/path status"), Decision::Allow);
    }

    #[test]
    fn allow_git_c_dir_log() {
        assert_eq!(eval("git -C /some/repo log --oneline"), Decision::Allow);
    }

    #[test]
    fn allow_git_c_dir_diff() {
        assert_eq!(eval("git -C ../other diff"), Decision::Allow);
    }

    #[test]
    fn ask_git_c_dir_push() {
        assert_eq!(eval("git -C /some/repo push origin main"), Decision::Ask);
    }

    #[test]
    fn allow_git_no_pager_log() {
        assert_eq!(eval("git --no-pager log"), Decision::Allow);
    }

    #[test]
    fn allow_git_c_config_status() {
        // -c key=value is also a global flag
        assert_eq!(eval("git -c core.pager=cat status"), Decision::Allow);
    }

    // ── allow list (git-town local commands) ──

    #[test]
    fn allow_list_allows_town_hack() {
        let spec = GitSpec::from_config(&GitConfig {
            read_only: vec![],
            allow: vec!["town hack".into()],
            mutating: vec![],
            allowed_with_config: vec![],
            config_env: HashMap::new(),
            force_push_flags: vec![],
        });
        let ctx = CommandContext::from_command("git town hack feature-branch");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Allow);
    }

    // ── mutating list ──

    #[test]
    fn mutating_list_asks_town_sync() {
        let spec = GitSpec::from_config(&GitConfig {
            read_only: vec![],
            allow: vec![],
            mutating: vec!["town sync".into()],
            allowed_with_config: vec![],
            config_env: HashMap::new(),
            force_push_flags: vec![],
        });
        let ctx = CommandContext::from_command("git town sync");
        assert_eq!(spec.evaluate(&ctx).decision, Decision::Ask);
    }
}
