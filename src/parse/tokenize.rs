/// Extract the first real command word, skipping leading VAR=value assignments.
pub fn base_command(command: &str) -> String {
    let mut rest = command.trim();
    // Skip VAR=value prefixes
    loop {
        if let Some(eq_pos) = rest.find('=') {
            let before_eq = &rest[..eq_pos];
            if !before_eq.is_empty()
                && before_eq
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && before_eq
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                let after_eq = &rest[eq_pos + 1..];
                if let Some(sp) = after_eq.find(char::is_whitespace) {
                    rest = after_eq[sp..].trim_start();
                    continue;
                }
            }
        }
        break;
    }
    rest.split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Extract leading KEY=VALUE pairs from a command string.
pub fn env_vars(command: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut rest = command.trim();
    loop {
        if let Some(eq_pos) = rest.find('=') {
            let before_eq = &rest[..eq_pos];
            if !before_eq.is_empty()
                && before_eq
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && before_eq
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                let after_eq = &rest[eq_pos + 1..];
                if let Some(sp) = after_eq.find(char::is_whitespace) {
                    let key = before_eq.to_string();
                    let val = after_eq[..sp].to_string();
                    result.push((key, val));
                    rest = after_eq[sp..].trim_start();
                    continue;
                }
            }
        }
        break;
    }
    result
}

/// Tokenize a command segment into words using shlex (POSIX word splitting).
pub fn tokenize(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_else(|| {
        // Fallback: simple whitespace splitting if shlex can't parse
        command.split_whitespace().map(String::from).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_command_simple() {
        assert_eq!(base_command("ls -la"), "ls");
    }

    #[test]
    fn base_command_with_env() {
        assert_eq!(
            base_command("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push"),
            "git"
        );
    }

    #[test]
    fn base_command_empty() {
        assert_eq!(base_command(""), "");
    }

    #[test]
    fn env_vars_single() {
        let vars = env_vars("FOO=bar cmd");
        assert_eq!(vars, vec![("FOO".into(), "bar".into())]);
    }

    #[test]
    fn env_vars_multiple() {
        let vars = env_vars("A=1 B=2 cmd");
        assert_eq!(
            vars,
            vec![("A".into(), "1".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn env_vars_none() {
        let vars = env_vars("cmd --flag");
        assert!(vars.is_empty());
    }

    #[test]
    fn tokenize_simple() {
        assert_eq!(tokenize("ls -la /tmp"), vec!["ls", "-la", "/tmp"]);
    }

    #[test]
    fn tokenize_quoted() {
        assert_eq!(
            tokenize("echo 'hello world'"),
            vec!["echo", "hello world"]
        );
    }

    #[test]
    fn tokenize_double_quoted() {
        assert_eq!(
            tokenize("echo \"hello world\""),
            vec!["echo", "hello world"]
        );
    }
}
