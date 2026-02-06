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

    /// Check if a specific env var key is present.
    pub fn has_env(&self, key: &str) -> bool {
        self.env_vars.iter().any(|(k, _)| k == key)
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
