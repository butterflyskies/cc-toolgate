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
            let expanded = match shellexpand::full(value) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("shellexpand failed for config_env {key}={value}: {e}");
                    std::borrow::Cow::Borrowed(value.as_str())
                }
            };
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

    /// Panic unless running under nextest (process-per-test isolation).
    ///
    /// Tests that call `std::env::set_var` / `remove_var` are unsound under
    /// `cargo test`, which runs tests concurrently in a single process.
    /// Nextest sets `NEXTEST=1` in every child process.
    fn require_nextest() {
        assert!(
            std::env::var("NEXTEST").is_ok(),
            "this test mutates process env and requires nextest (cargo nextest run)"
        );
    }

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
        require_nextest();
        let key = "CC_TOOLGATE_TEST_PROCESS_ENV";
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
        unsafe { std::env::set_var(key, "expected_value") };
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(key.into(), "expected_value".into())]);
        assert!(ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn env_satisfies_process_env_wrong_value() {
        require_nextest();
        let key = "CC_TOOLGATE_TEST_WRONG_VALUE";
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
        unsafe { std::env::set_var(key, "actual") };
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(key.into(), "expected".into())]);
        assert!(!ctx.env_satisfies(&req));
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn env_satisfies_multi_source_one_inline_one_process() {
        require_nextest();
        let key_process = "CC_TOOLGATE_TEST_MULTI_PROC";
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
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
        // No env mutation — safe under cargo test
        let ctx = CommandContext::from_command("INLINE_VAR=correct git push");
        let req = HashMap::from([
            ("INLINE_VAR".into(), "correct".into()),
            ("MISSING_VAR".into(), "value".into()),
        ]);
        assert!(!ctx.env_satisfies(&req));
    }

    #[test]
    fn env_satisfies_multi_source_one_wrong() {
        require_nextest();
        let key_process = "CC_TOOLGATE_TEST_MULTI_WRONG";
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
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
        require_nextest();
        let key = "CC_TOOLGATE_TEST_TILDE";
        let home = std::env::var("HOME").unwrap();
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
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

    // ── Collision tests ──
    //
    // These two tests use the SAME env var key with DIFFERENT expected values.
    // Under nextest (process-per-test), both pass reliably because each process
    // has its own environment. Under `cargo test` (shared process, concurrent
    // threads), one would see the other's write and produce a wrong result.

    const COLLISION_KEY: &str = "CC_TOOLGATE_TEST_COLLISION";

    #[test]
    fn env_collision_value_alpha() {
        require_nextest();
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
        unsafe { std::env::set_var(COLLISION_KEY, "alpha") };
        // Spin briefly to widen the race window under concurrent execution
        std::thread::sleep(std::time::Duration::from_millis(5));
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(COLLISION_KEY.into(), "alpha".into())]);
        assert!(ctx.env_satisfies(&req), "expected 'alpha', env was tampered");
        unsafe { std::env::remove_var(COLLISION_KEY) };
    }

    #[test]
    fn env_collision_value_beta() {
        require_nextest();
        // SAFETY: nextest runs each test in its own process (verified by require_nextest)
        unsafe { std::env::set_var(COLLISION_KEY, "beta") };
        std::thread::sleep(std::time::Duration::from_millis(5));
        let ctx = CommandContext::from_command("git push");
        let req = HashMap::from([(COLLISION_KEY.into(), "beta".into())]);
        assert!(ctx.env_satisfies(&req), "expected 'beta', env was tampered");
        unsafe { std::env::remove_var(COLLISION_KEY) };
    }
}
