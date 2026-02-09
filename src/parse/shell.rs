use super::types::{Operator, ParsedPipeline, Redirection, ShellSegment};

/// Result of parsing a heredoc at `<<DELIM`.
///
/// In shell, the text after the delimiter word on the same line is NOT part of
/// the heredoc body — it's still live shell syntax (pipes, redirections, etc.).
/// For example: `cat <<'EOF' | kubectl apply -f -`
///   - `after_delim`: position right after the delimiter word (before ` | kubectl...`)
///   - `body_end`: position after the closing delimiter line
///   - `is_quoted`: whether the delimiter was quoted, suppressing expansion
struct HeredocSpan {
    after_delim: usize,
    body_end: usize,
    is_quoted: bool,
}

/// When `<<` is encountered at `chars[start]`, parse the heredoc delimiter
/// and find where the body ends.
///
/// Returns `None` if not a valid heredoc (e.g., `<<<` here-string, missing delimiter).
fn skip_heredoc(chars: &[char], start: usize) -> Option<HeredocSpan> {
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

    // Mark position right after delimiter word — the rest of this line
    // is still live shell syntax (pipes, redirections, etc.)
    let after_delim = i;

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
                let body_end = if after < len { after + 1 } else { after };
                return Some(HeredocSpan { after_delim, body_end, is_quoted });
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
    Some(HeredocSpan { after_delim, body_end: len, is_quoted })
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

        // Heredoc: include <<DELIM token, let rest of line be processed
        // normally for operators, then skip the body entirely.
        if c == '<' && i + 1 < len && chars[i + 1] == '<'
            && let Some(span) = skip_heredoc(&chars, i)
        {
            // Push the <<DELIM token (up to after_delim)
            for ch in &chars[i..span.after_delim] {
                buf.push(*ch);
            }
            // Record that we need to skip the heredoc body later
            // For now, continue processing the rest of the <<DELIM line
            // for operators. We'll find the \n and then jump to body_end.
            //
            // Strategy: replace heredoc body (from the \n after the DELIM
            // line through body_end) with nothing — just advance past it.
            // But we need to let the rest of this line be processed first.
            // So we stash the body_end and jump there when we hit \n.
            //
            // Simplest approach: scan rest of line normally for operators,
            // then when we find \n, jump to body_end instead of i+1.
            i = span.after_delim;
            // Find end of current line, processing operators along the way
            while i < len && chars[i] != '\n' {
                let c2 = chars[i];

                // Two-char operators on same line as heredoc
                if i + 1 < len {
                    let next2 = chars[i + 1];
                    let op = match (c2, next2) {
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

                // Single-char operators on same line as heredoc
                match c2 {
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
                    _ => {
                        buf.push(c2);
                        i += 1;
                    }
                }
            }
            // Skip heredoc body: jump from end-of-line to body_end
            i = span.body_end;
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
    let mut heredoc_body_end: Option<usize> = None;

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

        // Heredoc with quoted delimiter: skip body (no expansion).
        // Unquoted heredocs fall through — their bodies DO expand substitutions.
        // In both cases, the rest of the <<DELIM line is still live syntax.
        if c == '<' && !dq && i + 1 < len && chars[i + 1] == '<'
            && let Some(span) = skip_heredoc(&chars, i)
            && span.is_quoted
        {
            // Push <<DELIM token
            for ch in &chars[i..span.after_delim] {
                outer.push(*ch);
            }
            // Skip past the <<DELIM token; the outer loop will process the
            // rest of the line normally (handling $(), backticks, etc.).
            // When we hit \n, we jump over the heredoc body.
            i = span.after_delim;
            heredoc_body_end = Some(span.body_end);
            continue;
        }

        // If we're on a heredoc-bearing line and hit \n, skip the body.
        if c == '\n' && let Some(body_end) = heredoc_body_end.take() {
            outer.push('\n');
            i = body_end;
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

/// Skip past `>` or `>>` and any whitespace, then collect the unquoted target word.
/// Returns the target path (e.g. "/dev/null", "file.txt") or empty string if nothing follows.
fn extract_redir_target(chars: &[char], start: usize) -> String {
    let len = chars.len();
    let mut i = start;
    // Skip past > or >>
    if i < len && chars[i] == '>' {
        i += 1;
    }
    if i < len && chars[i] == '>' {
        i += 1; // >>
    }
    // Skip whitespace
    while i < len && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }
    // Collect target word (unquoted for now — sufficient for /dev/null detection)
    let mut target = String::new();
    while i < len && !chars[i].is_whitespace() && chars[i] != ';' && chars[i] != '|' && chars[i] != '&' {
        target.push(chars[i]);
        i += 1;
    }
    target
}

/// Returns true if an fd number (0-9) refers to a standard stream (stdin/stdout/stderr).
/// Custom fd numbers (3+) are NOT considered safe for duplication targets because they
/// could have been redirected to files earlier in the same compound command
/// (e.g. `exec 3>/tmp/evil && cmd >&3`).
fn is_standard_fd(c: char) -> bool {
    c == '0' || c == '1' || c == '2'
}

/// Detect output redirection (>, >>, &>, fd>) outside quotes.
/// Does NOT flag:
///   - Input redirection (<) or here-docs (<<, <<<)
///   - Redirection to /dev/null (discarding output is not mutating)
///   - fd-to-fd duplication to standard streams: >&1, >&2, N>&1, N>&2
///   - fd closing: >&-, N>&-
///
/// DOES flag (conservatively):
///   - >&N where N >= 3 (custom fds could point to files)
///   - N>&M where M >= 3 (same reason)
pub fn has_output_redirection(command: &str) -> Option<Redirection> {
    let chars: Vec<char> = command.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let (mut sq, mut dq, mut esc) = (false, false, false);
    let mut heredoc_body_end: Option<usize> = None;

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

        // Heredoc: skip only the body to avoid false redirection detection
        // inside it (e.g. <noreply@anthropic.com> in a commit message).
        // The rest of the <<DELIM line is still live syntax and must be scanned
        // for actual redirections (e.g. `cat <<EOF > file`).
        if c == '<' && i + 1 < len && chars[i + 1] == '<'
            && let Some(span) = skip_heredoc(&chars, i)
        {
            // Skip past <<DELIM token only
            i = span.after_delim;
            // Scan rest of line normally (the outer loop handles it),
            // then skip the heredoc body when we hit the newline.
            // We store body_end and check for newline transitions below.
            heredoc_body_end = Some(span.body_end);
            continue;
        }

        // If we're inside a heredoc-bearing line and hit \n, skip the body.
        if c == '\n' && let Some(body_end) = heredoc_body_end.take() {
            i = body_end;
            continue;
        }

        // &> or &>> (redirect both stdout+stderr to file)
        if c == '&' && i + 1 < len && chars[i + 1] == '>' {
            let target = extract_redir_target(&chars, i + 1);
            if target == "/dev/null" {
                // Skip past &> /dev/null
                i += 2;
                continue;
            }
            return Some(Redirection {
                description: "output redirection (&>)".into(),
            });
        }

        // fd redirects: N>, N>>, N>&M, N>&-
        if c.is_ascii_digit() && i + 1 < len && chars[i + 1] == '>' {
            // N>&- is fd closing — always safe
            if i + 2 < len
                && chars[i + 2] == '&'
                && i + 3 < len
                && chars[i + 3] == '-'
            {
                i += 4;
                continue;
            }
            // N>&M — safe only when M is a standard fd (0-2)
            if i + 2 < len
                && chars[i + 2] == '&'
                && i + 3 < len
                && chars[i + 3].is_ascii_digit()
            {
                if is_standard_fd(chars[i + 3]) {
                    i += 4;
                    continue;
                }
                return Some(Redirection {
                    description: format!("output redirection ({c}>&{}, custom fd target)", chars[i + 3]),
                });
            }
            // N> file or N>> file — check for /dev/null
            let target = extract_redir_target(&chars, i + 1);
            if target == "/dev/null" {
                i += 2;
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
            // >&- is fd closing — always safe
            if i + 1 < len
                && chars[i + 1] == '&'
                && i + 2 < len
                && chars[i + 2] == '-'
            {
                i += 3;
                continue;
            }
            // >&N — safe only when N is a standard fd (0-2)
            if i + 1 < len
                && chars[i + 1] == '&'
                && i + 2 < len
                && chars[i + 2].is_ascii_digit()
            {
                if is_standard_fd(chars[i + 2]) {
                    i += 3;
                    continue;
                }
                return Some(Redirection {
                    description: format!("output redirection (>&{}, custom fd target)", chars[i + 2]),
                });
            }
            // > file or >> file — check for /dev/null
            let target = extract_redir_target(&chars, i);
            if target == "/dev/null" {
                i += 1;
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

/// Parse a full command string into a pipeline and its top-level substitutions.
///
/// Extracts substitutions first, then splits at compound operators.
/// Each segment carries its redirection state.
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
    fn no_redir_fd_dup_stderr_to_stdout() {
        assert!(has_output_redirection("cmd 2>&1").is_none());
    }

    #[test]
    fn no_redir_fd_dup_stdout_to_stderr() {
        assert!(has_output_redirection("cmd 1>&2").is_none());
    }

    #[test]
    fn no_redir_fd_close() {
        assert!(has_output_redirection("cmd 2>&-").is_none());
    }

    #[test]
    fn no_redir_bare_dup_to_stderr() {
        assert!(has_output_redirection("cmd >&2").is_none());
    }

    #[test]
    fn no_redir_bare_close() {
        assert!(has_output_redirection("cmd >&-").is_none());
    }

    #[test]
    fn no_redir_process_subst() {
        assert!(has_output_redirection("diff >(sort)").is_none());
    }

    #[test]
    fn no_redir_quoted() {
        assert!(has_output_redirection("echo 'hello > world'").is_none());
    }

    // ── /dev/null is non-mutating ──

    #[test]
    fn no_redir_devnull() {
        assert!(has_output_redirection("cmd > /dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_append() {
        assert!(has_output_redirection("cmd >> /dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_stderr() {
        assert!(has_output_redirection("cmd 2> /dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_ampersand() {
        assert!(has_output_redirection("cmd &> /dev/null").is_none());
    }

    #[test]
    fn redir_devnull_plus_file() {
        // /dev/null for stderr is fine, but > file is still mutation
        assert!(has_output_redirection("cmd > /tmp/out 2> /dev/null").is_some());
    }

    // ── Custom fd targets are suspicious ──

    #[test]
    fn redir_custom_fd_dup_target() {
        // >&3 could be writing to a file if fd 3 was opened earlier
        assert!(has_output_redirection("cmd >&3").is_some());
    }

    #[test]
    fn redir_custom_fd_dup_numbered() {
        // 2>&3 — fd 3 could be a file
        assert!(has_output_redirection("cmd 2>&3").is_some());
    }

    #[test]
    fn no_redir_standard_fd_dup() {
        // >&1 is always safe (stdout to stdout)
        assert!(has_output_redirection("cmd >&1").is_none());
    }

    // ── Heredoc parsing ──

    #[test]
    fn heredoc_skip_helper_basic() {
        let input: Vec<char> = "<<'EOF'\nbody\nEOF\n".chars().collect();
        let span = skip_heredoc(&input, 0).unwrap();
        assert!(span.is_quoted);
        assert_eq!(span.body_end, input.len());
    }

    #[test]
    fn heredoc_skip_helper_unquoted() {
        let input: Vec<char> = "<<EOF\nbody\nEOF\n".chars().collect();
        let span = skip_heredoc(&input, 0).unwrap();
        assert!(!span.is_quoted);
        assert_eq!(span.body_end, input.len());
    }

    #[test]
    fn heredoc_skip_helper_double_quoted() {
        let input: Vec<char> = "<<\"EOF\"\nbody\nEOF\n".chars().collect();
        let span = skip_heredoc(&input, 0).unwrap();
        assert!(span.is_quoted);
        assert_eq!(span.body_end, input.len());
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
    fn heredoc_no_false_redirection() {
        // > inside heredoc body should NOT be detected as output redirection
        let cmd = "cat <<'EOF'\nCo-Authored-By: Name <noreply@anthropic.com>\nEOF\n";
        assert!(has_output_redirection(cmd).is_none(), "heredoc body > should not trigger redirection");
    }

    #[test]
    fn heredoc_unquoted_no_false_redirection() {
        // Even unquoted heredocs: > in body is not output redirection
        let cmd = "cat <<EOF\nsome text > with angle brackets\nEOF\n";
        assert!(has_output_redirection(cmd).is_none(), "unquoted heredoc body > should not trigger redirection");
    }

    #[test]
    fn heredoc_redir_before_heredoc_detected() {
        // Actual redirection BEFORE the heredoc should still be caught
        let cmd = "cat > /tmp/out <<'EOF'\nbody\nEOF\n";
        assert!(has_output_redirection(cmd).is_some(), "redirection before heredoc should be detected");
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

    // ── Heredoc + pipe splitting (the pipe-swallowing bug) ──

    #[test]
    fn heredoc_pipe_splits_correctly() {
        // The exact pattern that bypassed the gate:
        // `cat <<'EOF' | kubectl apply -f -` should split into TWO segments
        let cmd = "cat <<'EOF' | kubectl apply -f -\napiVersion: v1\nkind: Role\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "heredoc pipe should split into 2 parts: {parts:?}");
        assert_eq!(ops, vec![Operator::Pipe]);
        assert!(parts[0].starts_with("cat <<'EOF'"), "first part should be cat heredoc: {}", parts[0]);
        assert_eq!(parts[1].trim(), "kubectl apply -f -", "second part should be kubectl: {}", parts[1]);
    }

    #[test]
    fn heredoc_pipe_unquoted_splits_correctly() {
        let cmd = "cat <<EOF | grep pattern\nbody line\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "unquoted heredoc pipe should split: {parts:?}");
        assert_eq!(ops, vec![Operator::Pipe]);
        assert_eq!(parts[1].trim(), "grep pattern");
    }

    #[test]
    fn heredoc_and_splits_correctly() {
        // && on same line as heredoc
        let cmd = "cat <<'EOF' && echo done\nbody\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "heredoc && should split: {parts:?}");
        assert_eq!(ops, vec![Operator::And]);
        assert_eq!(parts[1].trim(), "echo done");
    }

    #[test]
    fn heredoc_semicolon_splits_correctly() {
        let cmd = "cat <<'EOF' ; rm -rf /\nbody\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "heredoc ; should split: {parts:?}");
        assert_eq!(ops, vec![Operator::Semi]);
        assert_eq!(parts[1].trim(), "rm -rf /");
    }

    #[test]
    fn heredoc_pipe_err_splits_correctly() {
        let cmd = "cat <<'EOF' |& grep error\nbody\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "heredoc |& should split: {parts:?}");
        assert_eq!(ops, vec![Operator::PipeErr]);
        assert_eq!(parts[1].trim(), "grep error");
    }

    #[test]
    fn heredoc_or_splits_correctly() {
        let cmd = "cat <<'EOF' || echo fallback\nbody\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 2, "heredoc || should split: {parts:?}");
        assert_eq!(ops, vec![Operator::Or]);
        assert_eq!(parts[1].trim(), "echo fallback");
    }

    #[test]
    fn heredoc_redir_on_delim_line_detected() {
        // `cat <<'EOF' > /tmp/out` — the > is on the DELIM line, should be detected
        let cmd = "cat <<'EOF' > /tmp/out\nbody\nEOF\n";
        assert!(has_output_redirection(cmd).is_some(),
            "redirection on heredoc DELIM line should be detected");
    }

    #[test]
    fn heredoc_skip_after_delim_position() {
        // Verify after_delim points right after the delimiter word
        let input: Vec<char> = "<<'EOF' | kubectl apply -f -\nbody\nEOF\n".chars().collect();
        let span = skip_heredoc(&input, 0).unwrap();
        assert!(span.is_quoted);
        // after_delim should be right after 'EOF' — before the space-pipe-space
        let rest: String = input[span.after_delim..].iter().collect();
        assert!(rest.starts_with(" | kubectl"), "rest of line after delim: {rest}");
        assert_eq!(span.body_end, input.len());
    }

    #[test]
    fn heredoc_body_operators_still_ignored() {
        // Operators inside the heredoc body should NOT cause splits
        let cmd = "cat <<'EOF'\nline && other | stuff ; more\nEOF\n";
        let (parts, ops) = split_compound_command(cmd);
        assert_eq!(parts.len(), 1, "body operators should not split: {parts:?}");
        assert!(ops.is_empty());
    }

    #[test]
    fn heredoc_pipe_substitutions_on_right_side() {
        // $() on the right side of a heredoc pipe should be extracted
        let cmd = "cat <<'EOF' | kubectl apply -f $(echo -)\nbody\nEOF\n";
        let (outer, subs) = extract_substitutions(cmd);
        assert_eq!(subs, vec!["echo -"], "substitution on pipe RHS should be extracted: {subs:?}");
        // The outer should still contain the pipe
        assert!(outer.contains('|'), "outer should contain pipe: {outer}");
    }
}
