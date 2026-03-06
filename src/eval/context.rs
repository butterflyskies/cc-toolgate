//! Per-segment command context: tokenization, env var extraction, and redirection detection.

use crate::parse::Redirection;

/// Context for evaluating a single command segment.
#[derive(Debug)]
pub struct CommandContext<'a> {
    /// The full command text of this segment.
    pub raw: &'a str,
    /// The base command name (e.g. "git", "ls", "cargo").
    pub base_command: String,
    /// All words in the command (tokenized via shlex).
    pub words: Vec<String>,
    /// Leading KEY=VALUE environment variable assignments.
    pub env_vars: Vec<(String, String)>,
    /// Detected output redirection, if any.
    pub redirection: Option<Redirection>,
}

impl<'a> CommandContext<'a> {
    /// Build a CommandContext from a raw command string.
    pub fn from_command(raw: &'a str) -> Self {
        let base_command = crate::parse::base_command(raw);
        let env_vars = crate::parse::env_vars(raw);
        let words = crate::parse::tokenize(raw);
        let redirection = crate::parse::has_output_redirection(raw);

        Self {
            raw,
            base_command,
            words,
            env_vars,
            redirection,
        }
    }

    /// Check if all required env var entries are satisfied.
    ///
    /// For each entry, checks the command's inline env vars first (exact key+value match),
    /// then falls back to the process environment (`std::env::var`).
    /// Returns true only if ALL entries match. Some entries may come from inline env
    /// and others from the process environment — each is checked independently.
    ///
    /// Config values are shell-expanded (`~`, `$HOME`, `$VAR`) before comparison,
    /// since shells expand these in env assignments before they reach the process.
    pub fn env_satisfies(&self, required: &std::collections::HashMap<String, String>) -> bool {
        required.iter().all(|(key, value)| {
            let expanded = shellexpand::full(value).unwrap_or(std::borrow::Cow::Borrowed(value));
            // Check inline env vars first (may contain literal ~ or expanded path)
            if let Some((_, v)) = self.env_vars.iter().find(|(k, _)| k == key) {
                return v == value || v == expanded.as_ref();
            }
            // Fall back to process environment (shell will have expanded already)
            std::env::var(key).is_ok_and(|v| v == *value || v == expanded.as_ref())
        })
    }

    /// Get words after skipping env vars and the base command.
    pub fn args(&self) -> &[String] {
        // Skip env var tokens and the base command itself
        let skip = self.env_vars.len() + 1; // each env var is one token in shlex, plus the command
        if self.words.len() > skip {
            &self.words[skip..]
        } else {
            &[]
        }
    }

    /// Check if any word matches a flag.
    pub fn has_flag(&self, flag: &str) -> bool {
        self.words.iter().any(|w| w == flag)
    }

    /// Check if any word matches any of the given flags.
    pub fn has_any_flag(&self, flags: &[&str]) -> bool {
        self.words.iter().any(|w| flags.contains(&w.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn env_satisfies_inline_exact() {
        let ctx = CommandContext::from_command("FOO=bar git push");
        let req = HashMap::from([("FOO".into(), "bar".into())]);
        assert!(ctx.env_satisfies(&req));
    }

    #[test]
    fn env_satisfies_inline_wrong_value() {
        let ctx = CommandContext::from_command("FOO=baz git push");
        let req = HashMap::from([("FOO".into(), "bar".into())]);
        assert!(!ctx.env_satisfies(&req));
    }

    #[test]
    fn env_satisfies_inline_missing() {
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([("FOO".into(), "bar".into())]);
        // No inline var, no process env → false
        assert!(!ctx.env_satisfies(&req));
    }

    #[test]
    fn env_satisfies_process_env() {
        // Set a unique env var for this test
        let key = "CC_TOOLGATE_TEST_PROCESS_ENV";
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe { std::env::set_var(key, "expected_value") };
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(key.into(), "expected_value".into())]);
        assert!(ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn env_satisfies_process_env_wrong_value() {
        let key = "CC_TOOLGATE_TEST_WRONG_VALUE";
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe { std::env::set_var(key, "actual") };
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(key.into(), "expected".into())]);
        assert!(!ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn env_satisfies_multi_source_one_inline_one_process() {
        // Simulates hook environment providing one var, command providing another
        let key_process = "CC_TOOLGATE_TEST_MULTI_PROC";
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe { std::env::set_var(key_process, "/correct/path") };
        let ctx = CommandContext::from_command("INLINE_VAR=correct git push");
        let req = HashMap::from([
            ("INLINE_VAR".into(), "correct".into()),
            (key_process.into(), "/correct/path".into()),
        ]);
        assert!(ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key_process) };
    }

    #[test]
    fn env_satisfies_multi_source_one_missing() {
        // One var satisfied via inline, other not set anywhere → fails
        let ctx = CommandContext::from_command("INLINE_VAR=correct git push");
        let req = HashMap::from([
            ("INLINE_VAR".into(), "correct".into()),
            ("MISSING_VAR".into(), "value".into()),
        ]);
        assert!(!ctx.env_satisfies(&req));
    }

    #[test]
    fn env_satisfies_multi_source_one_wrong() {
        // Both set, but process env has wrong value → fails
        let key_process = "CC_TOOLGATE_TEST_MULTI_WRONG";
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe { std::env::set_var(key_process, "/wrong/path") };
        let ctx = CommandContext::from_command("INLINE_VAR=correct git push");
        let req = HashMap::from([
            ("INLINE_VAR".into(), "correct".into()),
            (key_process.into(), "/correct/path".into()),
        ]);
        assert!(!ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key_process) };
    }

    #[test]
    fn env_satisfies_tilde_expansion() {
        // Config says ~/foo, process env has /home/user/foo
        let key = "CC_TOOLGATE_TEST_TILDE";
        let home = std::env::var("HOME").unwrap();
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe { std::env::set_var(key, format!("{home}/foo")) };
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(key.into(), "~/foo".into())]);
        assert!(ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn env_satisfies_empty_map() {
        let ctx = CommandContext::from_command("git push");
        assert!(ctx.env_satisfies(&HashMap::new()));
    }
}
