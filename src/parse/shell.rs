use super::types::{Operator, ParsedPipeline, Redirection, ShellSegment};

/// When `<<` is encountered at `chars[start]`, parse the heredoc delimiter
/// and find where the body ends.
///
/// Returns `Some((end_pos, is_quoted))`:
/// - `end_pos`: position after the closing delimiter line
/// - `is_quoted`: true if delimiter was quoted (`<<'EOF'` or `<<"EOF"`), suppressing expansion
///
/// Returns `None` if not a valid heredoc (e.g., `<<<` here-string, missing delimiter).
fn skip_heredoc(chars: &[char], start: usize) -> Option<(usize, bool)> {
    let len = chars.len();
    let mut i = start;

    // Verify <<
    if i + 1 >= len || chars[i] != '<' || chars[i + 1] != '<' {
        return None;
    }
    i += 2;

    // Reject <<< (here-string)
    if i < len && chars[i] == '<' {
        return None;
    }

    // Optional - for <<-
    if i < len && chars[i] == '-' {
        i += 1;
    }

    // Skip spaces/tabs before delimiter (not newlines)
    while i < len && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }

    if i >= len || chars[i] == '\n' {
        return None;
    }

    // Read delimiter word (possibly quoted)
    let mut delimiter = String::new();
    let mut is_quoted = false;

    if chars[i] == '\'' {
        is_quoted = true;
        i += 1;
        while i < len && chars[i] != '\'' && chars[i] != '\n' {
            delimiter.push(chars[i]);
            i += 1;
        }
        if i < len && chars[i] == '\'' {
            i += 1;
        }
    } else if chars[i] == '"' {
        is_quoted = true;
        i += 1;
        while i < len && chars[i] != '"' && chars[i] != '\n' {
            delimiter.push(chars[i]);
            i += 1;
        }
        if i < len && chars[i] == '"' {
            i += 1;
        }
    } else {
        while i < len && !chars[i].is_whitespace() {
            delimiter.push(chars[i]);
            i += 1;
        }
    }

    if delimiter.is_empty() {
        return None;
    }

    // Skip to end of current line (heredoc body starts on next line)
    while i < len && chars[i] != '\n' {
        i += 1;
    }
    if i < len {
        i += 1; // skip \n
    }

    // Scan lines for the closing delimiter
    let delim_chars: Vec<char> = delimiter.chars().collect();
    while i < len {
        // Check if this line matches the delimiter exactly
        let check = i;
        let mut matches = check + delim_chars.len() <= len;
        if matches {
            for (j, dc) in delim_chars.iter().enumerate() {
                if chars[check + j] != *dc {
                    matches = false;
                    break;
                }
            }
        }
        if matches {
            let after = check + delim_chars.len();
            if after >= len || chars[after] == '\n' {
                return Some((if after < len { after + 1 } else { after }, is_quoted));
            }
        }

        // Skip to next line
        while i < len && chars[i] != '\n' {
            i += 1;
        }
        if i < len {
            i += 1;
        }
    }

    // Delimiter not found — treat rest as heredoc body
    Some((len, is_quoted))
}

/// Split a command at shell operators (&&, ||, ;, |, |&),
/// respecting single/double quotes and backslash escapes.
///
/// Returns segments and the operators between them.
fn split_compound_command(command: &str) -> (Vec<String>, Vec<Operator>) {
    let mut parts = Vec::new();
    let mut operators = Vec::new();
    let mut buf = String::new();

    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let (mut sq, mut dq, mut esc) = (false, false, false);

    while i < len {
        let c = chars[i];

        if esc {
            buf.push(c);
            esc = false;
            i += 1;
            continue;
        }
        if c == '\\' && !sq {
            esc = true;
            buf.push(c);
            i += 1;
            continue;
        }
        if c == '\'' && !dq {
            sq = !sq;
            buf.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !sq {
            dq = !dq;
            buf.push(c);
            i += 1;
            continue;
        }
        if sq || dq {
            buf.push(c);
            i += 1;
            continue;
        }

        // Heredoc: skip body to avoid false operator splits inside it
        if c == '<' && i + 1 < len && chars[i + 1] == '<'
            && let Some((end_pos, _)) = skip_heredoc(&chars, i)
        {
            for ch in &chars[i..end_pos] {
                buf.push(*ch);
            }
            i = end_pos;
            continue;
        }

        // Two-char operators
        if i + 1 < len {
            let next = chars[i + 1];
            let op = match (c, next) {
                ('&', '&') => Some(Operator::And),
                ('|', '|') => Some(Operator::Or),
                ('|', '&') => Some(Operator::PipeErr),
                _ => None,
            };
            if let Some(op) = op {
                let trimmed = buf.trim().to_string();
                parts.push(trimmed);
                operators.push(op);
                buf.clear();
                i += 2;
                continue;
            }
        }

        // Single-char operators
        match c {
            '|' => {
                let trimmed = buf.trim().to_string();
                parts.push(trimmed);
                operators.push(Operator::Pipe);
                buf.clear();
                i += 1;
                continue;
            }
            ';' => {
                let trimmed = buf.trim().to_string();
                parts.push(trimmed);
                operators.push(Operator::Semi);
                buf.clear();
                i += 1;
                continue;
            }
            _ => {}
        }

        buf.push(c);
        i += 1;
    }

    let tail = buf.trim().to_string();
    if !tail.is_empty() {
        parts.push(tail);
    }

    // Filter empties
    parts.retain(|p| !p.is_empty());

    (parts, operators)
}

/// Extract command substitution contents from `$(...)` and backticks.
/// Returns the outer command with substitutions replaced by `__SUBST__`
/// placeholders, plus a vec of the extracted inner command strings.
///
/// Handles nesting: `$(cat $(which foo))` extracts `cat $(which foo)`,
/// which is then recursively evaluated by `evaluate()`.
///
/// `$()` is extracted even inside double quotes (shell expands it there).
/// Only single quotes block substitution detection.
fn extract_substitutions(command: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut outer = String::new();
    let mut inners = Vec::new();
    let mut i = 0;
    let (mut sq, mut dq, mut esc) = (false, false, false);

    while i < len {
        let c = chars[i];

        if esc {
            outer.push(c);
            esc = false;
            i += 1;
            continue;
        }
        if c == '\\' && !sq {
            esc = true;
            outer.push(c);
            i += 1;
            continue;
        }
        if c == '\'' && !dq {
            sq = !sq;
            outer.push(c);
            i += 1;
            continue;
        }
        if c == '"' && !sq {
            dq = !dq;
            outer.push(c);
            i += 1;
            continue;
        }
        // Single quotes block all substitution
        if sq {
            outer.push(c);
            i += 1;
            continue;
        }

        // Heredoc with quoted delimiter: skip body (no expansion)
        // Unquoted heredocs fall through — their bodies DO expand substitutions.
        if c == '<' && !dq && i + 1 < len && chars[i + 1] == '<'
            && let Some((end_pos, is_quoted)) = skip_heredoc(&chars, i)
            && is_quoted
        {
            for ch in &chars[i..end_pos] {
                outer.push(*ch);
            }
            i = end_pos;
            continue;
        }

        // $( — extract balanced content
        if c == '$' && i + 1 < len && chars[i + 1] == '(' {
            let mut depth: u32 = 1;
            let mut inner = String::new();
            let (mut isq, mut idq, mut iesc) = (false, false, false);
            i += 2; // skip $(
            while i < len && depth > 0 {
                let ic = chars[i];
                if iesc {
                    inner.push(ic);
                    iesc = false;
                    i += 1;
                    continue;
                }
                if ic == '\\' && !isq {
                    iesc = true;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if ic == '\'' && !idq {
                    isq = !isq;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if ic == '"' && !isq {
                    idq = !idq;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if !isq && !idq {
                    if ic == '(' {
                        depth += 1;
                    }
                    if ic == ')' {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                }
                inner.push(ic);
                i += 1;
            }
            let trimmed = inner.trim().to_string();
            if !trimmed.is_empty() {
                inners.push(trimmed);
            }
            outer.push_str("__SUBST__");
            continue;
        }

        // Backtick — extract to matching backtick (no nesting)
        if c == '`' {
            let mut inner = String::new();
            i += 1; // skip opening `
            while i < len && chars[i] != '`' {
                if chars[i] == '\\' && i + 1 < len {
                    inner.push(chars[i]);
                    inner.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                inner.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1; // skip closing `
            }
            let trimmed = inner.trim().to_string();
            if !trimmed.is_empty() {
                inners.push(trimmed);
            }
            outer.push_str("__SUBST__");
            continue;
        }

        // Process substitution <() / >() — extract inner command
        if (c == '<' || c == '>') && i + 1 < len && chars[i + 1] == '(' && !dq {
            let mut depth: u32 = 1;
            let mut inner = String::new();
            let (mut isq, mut idq, mut iesc) = (false, false, false);
            i += 2; // skip <( or >(
            while i < len && depth > 0 {
                let ic = chars[i];
                if iesc {
                    inner.push(ic);
                    iesc = false;
                    i += 1;
                    continue;
                }
                if ic == '\\' && !isq {
                    iesc = true;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if ic == '\'' && !idq {
                    isq = !isq;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if ic == '"' && !isq {
                    idq = !idq;
                    inner.push(ic);
                    i += 1;
                    continue;
                }
                if !isq && !idq {
                    if ic == '(' {
                        depth += 1;
                    }
                    if ic == ')' {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                }
                inner.push(ic);
                i += 1;
            }
            let trimmed = inner.trim().to_string();
            if !trimmed.is_empty() {
                inners.push(trimmed);
            }
            // Don't include < or > prefix — would false-trigger redirection detection
            outer.push_str("__SUBST__");
            continue;
        }

        outer.push(c);
        i += 1;
    }

    (outer, inners)
}

/// Detect output redirection (>, >>, &>, fd>) outside quotes.
/// Does NOT flag:
///   - Input redirection (<) or here-docs (<<, <<<)
///   - fd-to-fd duplication: >&N, N>&M, >&-, N>&- (e.g. 2>&1)
pub fn has_output_redirection(command: &str) -> Option<Redirection> {
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let (mut sq, mut dq, mut esc) = (false, false, false);

    while i < len {
        let c = chars[i];

        if esc {
            esc = false;
            i += 1;
            continue;
        }
        if c == '\\' && !sq {
            esc = true;
            i += 1;
            continue;
        }
        if c == '\'' && !dq {
            sq = !sq;
            i += 1;
            continue;
        }
        if c == '"' && !sq {
            dq = !dq;
            i += 1;
            continue;
        }
        if sq || dq {
            i += 1;
            continue;
        }

        // &> or &>> (redirect both stdout+stderr to file — always mutation)
        if c == '&' && i + 1 < len && chars[i + 1] == '>' {
            return Some(Redirection {
                description: "output redirection (&>)".into(),
            });
        }

        // fd redirects: N>, N>>, N>&M, N>&-
        if c.is_ascii_digit() && i + 1 < len && chars[i + 1] == '>' {
            // N>&M or N>&- is fd duplication/closing, not file output
            if i + 2 < len
                && chars[i + 2] == '&'
                && i + 3 < len
                && (chars[i + 3].is_ascii_digit() || chars[i + 3] == '-')
            {
                i += 4;
                continue;
            }
            return Some(Redirection {
                description: format!("output redirection ({c}>)"),
            });
        }

        // > or >> but NOT >( (process substitution), >&N, or >&-
        if c == '>' {
            if i + 1 < len && chars[i + 1] == '(' {
                i += 1;
                continue;
            }
            // >&N or >&- is fd duplication/closing
            if i + 1 < len
                && chars[i + 1] == '&'
                && i + 2 < len
                && (chars[i + 2].is_ascii_digit() || chars[i + 2] == '-')
            {
                i += 3;
                continue;
            }
            return Some(Redirection {
                description: "output redirection (>)".into(),
            });
        }

        i += 1;
    }

    None
}

/// Parse a full command string into a `ParsedPipeline`.
///
/// Extracts substitutions first, then splits at compound operators.
/// Each segment carries its substitutions and redirection state.
pub fn parse(command: &str) -> ParsedPipeline {
    let (outer, substitutions) = extract_substitutions(command);
    let (parts, operators) = split_compound_command(&outer);

    // Simple case: no compound splitting needed
    if parts.len() <= 1 && substitutions.is_empty() {
        let redir = has_output_redirection(command);
        return ParsedPipeline {
            segments: vec![ShellSegment {
                command: command.trim().to_string(),
                substitutions: vec![],
                redirection: redir,
            }],
            operators: vec![],
        };
    }

    let segments = parts
        .into_iter()
        .map(|part| {
            let redir = has_output_redirection(&part);
            ShellSegment {
                command: part,
                substitutions: vec![], // substitutions attached at pipeline level for now
                redirection: redir,
            }
        })
        .collect();

    ParsedPipeline {
        segments,
        operators,
    }
}

/// Parse and return both the pipeline and the top-level substitutions.
///
/// This is the main entry point for the evaluation layer.
pub fn parse_with_substitutions(command: &str) -> (ParsedPipeline, Vec<String>) {
    let (outer, substitutions) = extract_substitutions(command);
    let (parts, operators) = split_compound_command(&outer);

    if parts.len() <= 1 && substitutions.is_empty() {
        let redir = has_output_redirection(command);
        return (
            ParsedPipeline {
                segments: vec![ShellSegment {
                    command: command.trim().to_string(),
                    substitutions: vec![],
                    redirection: redir,
                }],
                operators: vec![],
            },
            vec![],
        );
    }

    let segments = parts
        .into_iter()
        .map(|part| {
            let redir = has_output_redirection(&part);
            ShellSegment {
                command: part,
                substitutions: vec![],
                redirection: redir,
            }
        })
        .collect();

    (
        ParsedPipeline {
            segments,
            operators,
        },
        substitutions,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple() {
        let (parts, ops) = split_compound_command("ls -la");
        assert_eq!(parts, vec!["ls -la"]);
        assert!(ops.is_empty());
    }

    #[test]
    fn split_and() {
        let (parts, ops) = split_compound_command("ls && pwd");
        assert_eq!(parts, vec!["ls", "pwd"]);
        assert_eq!(ops, vec![Operator::And]);
    }

    #[test]
    fn split_pipe() {
        let (parts, ops) = split_compound_command("cat file | grep pat");
        assert_eq!(parts, vec!["cat file", "grep pat"]);
        assert_eq!(ops, vec![Operator::Pipe]);
    }

    #[test]
    fn split_quoted_operator() {
        let (parts, ops) = split_compound_command("echo 'a && b'");
        assert_eq!(parts, vec!["echo 'a && b'"]);
        assert!(ops.is_empty());
    }

    #[test]
    fn extract_dollar_paren() {
        let (outer, inners) = extract_substitutions("ls $(which cargo)");
        assert_eq!(outer, "ls __SUBST__");
        assert_eq!(inners, vec!["which cargo"]);
    }

    #[test]
    fn extract_backtick() {
        let (outer, inners) = extract_substitutions("echo `whoami`");
        assert_eq!(outer, "echo __SUBST__");
        assert_eq!(inners, vec!["whoami"]);
    }

    #[test]
    fn extract_single_quoted_suppressed() {
        let (_, inners) = extract_substitutions("echo '$(rm -rf /)'");
        assert!(inners.is_empty());
    }

    #[test]
    fn extract_double_quoted_expanded() {
        let (_, inners) = extract_substitutions("echo \"$(rm -rf /)\"");
        assert_eq!(inners, vec!["rm -rf /"]);
    }

    #[test]
    fn extract_process_substitution() {
        let (outer, inners) = extract_substitutions("diff <(sort a) <(sort b)");
        assert!(!outer.contains('<'));
        assert_eq!(inners, vec!["sort a", "sort b"]);
    }

    #[test]
    fn redir_simple_gt() {
        assert!(has_output_redirection("ls > file").is_some());
    }

    #[test]
    fn redir_append() {
        assert!(has_output_redirection("ls >> file").is_some());
    }

    #[test]
    fn redir_ampersand_gt() {
        assert!(has_output_redirection("cmd &> file").is_some());
    }

    #[test]
    fn no_redir_fd_dup() {
        assert!(has_output_redirection("cmd 2>&1").is_none());
    }

    #[test]
    fn no_redir_fd_close() {
        assert!(has_output_redirection("cmd 2>&-").is_none());
    }

    #[test]
    fn no_redir_bare_dup() {
        assert!(has_output_redirection("cmd >&2").is_none());
    }

    #[test]
    fn no_redir_process_subst() {
        assert!(has_output_redirection("diff >(sort)").is_none());
    }

    #[test]
    fn no_redir_quoted() {
        assert!(has_output_redirection("echo 'hello > world'").is_none());
    }

    // ── Heredoc parsing ──

    #[test]
    fn heredoc_skip_helper_basic() {
        let input: Vec<char> = "<<'EOF'\nbody\nEOF\n".chars().collect();
        let (end, quoted) = skip_heredoc(&input, 0).unwrap();
        assert!(quoted);
        assert_eq!(end, input.len());
    }

    #[test]
    fn heredoc_skip_helper_unquoted() {
        let input: Vec<char> = "<<EOF\nbody\nEOF\n".chars().collect();
        let (end, quoted) = skip_heredoc(&input, 0).unwrap();
        assert!(!quoted);
        assert_eq!(end, input.len());
    }

    #[test]
    fn heredoc_skip_helper_double_quoted() {
        let input: Vec<char> = "<<\"EOF\"\nbody\nEOF\n".chars().collect();
        let (end, quoted) = skip_heredoc(&input, 0).unwrap();
        assert!(quoted);
        assert_eq!(end, input.len());
    }

    #[test]
    fn heredoc_skip_rejects_here_string() {
        let input: Vec<char> = "<<<word".chars().collect();
        assert!(skip_heredoc(&input, 0).is_none());
    }

    #[test]
    fn heredoc_quoted_no_backtick_substitutions() {
        // Backticks inside a quoted heredoc body should NOT be extracted
        let cmd = "cat <<'EOF'\nline with `backticks` here\nEOF\n";
        let (_, inners) = extract_substitutions(cmd);
        assert!(inners.is_empty(), "quoted heredoc body should suppress backtick extraction");
    }

    #[test]
    fn heredoc_quoted_no_dollar_substitutions() {
        // $() inside a quoted heredoc body should NOT be extracted
        let cmd = "cat <<'EOF'\nline with $(command) here\nEOF\n";
        let (_, inners) = extract_substitutions(cmd);
        assert!(inners.is_empty(), "quoted heredoc body should suppress $() extraction");
    }

    #[test]
    fn heredoc_unquoted_extracts_substitutions() {
        // Backticks inside an unquoted heredoc body ARE expanded
        let cmd = "cat <<EOF\n`whoami`\nEOF\n";
        let (_, inners) = extract_substitutions(cmd);
        assert_eq!(inners, vec!["whoami"]);
    }

    #[test]
    fn heredoc_no_false_compound_split() {
        // Operators inside heredoc body should NOT split the command
        let cmd = "cat <<'EOF'\nline && other ; stuff\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 1, "heredoc body operators should not split: {parts:?}");
        assert!(ops.is_empty());
    }

    #[test]
    fn heredoc_markdown_backticks_not_substitutions() {
        // The exact pattern that triggered the original bug:
        // markdown inline code with backticks inside a quoted heredoc
        let cmd = "cat <<'EOF'\n## Changes\n- **New:** `config.rs` — Config struct\n- **New:** `eval/mod.rs` — rewritten\nEOF\n";
        let (_, inners) = extract_substitutions(cmd);
        assert!(inners.is_empty(), "markdown backticks in heredoc should not be substitutions");
    }

    #[test]
    fn heredoc_in_dollar_subst() {
        // Heredoc wrapped in $() — the common Claude Code pattern
        let cmd = "gh pr create --body \"$(cat <<'EOF'\n## Summary\nCode with `backticks` and $(stuff)\nEOF\n)\"";
        let (_, inners) = extract_substitutions(cmd);
        // Only the outer $() should be extracted, containing the cat heredoc
        assert_eq!(inners.len(), 1, "should extract one $() substitution");
        assert!(inners[0].starts_with("cat <<'EOF'"));
        // Now recursively parse the inner — should find NO further substitutions
        let (_, inner_subs) = extract_substitutions(&inners[0]);
        assert!(inner_subs.is_empty(), "heredoc body should not yield substitutions on recursive parse");
    }
}
