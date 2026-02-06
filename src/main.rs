//! cc-toolgate: PreToolUse hook for Claude Code.
//!
//! Validates Bash tool calls with compound-command-aware parsing.
//! Reads JSON from stdin, writes a permission decision to stdout.
//!
//! Handles:
//!   - Command chaining: &&, ||, ;
//!   - Pipes: |, |&
//!   - Command substitution: $(), backticks
//!   - Process substitution: <(), >()
//!   - Output redirection: >, >>, 2>, &>, etc.
//!   - Quoting awareness (won't split inside quotes)

use serde::Deserialize;
use std::io::Read;

// ─── Types ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Decision {
    Allow,
    Ask,
    Deny,
}

impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone)]
struct RuleMatch {
    decision: Decision,
    reason: String,
}

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
}

// ─── Parsing ─────────────────────────────────────────

/// Split a command at shell operators (&&, ||, ;, |, |&),
/// respecting single/double quotes and backslash escapes.
fn split_compound_command(command: &str) -> (Vec<String>, Vec<String>) {
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

        // Two-char operators
        if i + 1 < len {
            let two: String = chars[i..=i + 1].iter().collect();
            if two == "&&" || two == "||" || two == "|&" {
                let trimmed = buf.trim().to_string();
                parts.push(trimmed);
                operators.push(two);
                buf.clear();
                i += 2;
                continue;
            }
        }

        // Single-char operators
        if c == '|' || c == ';' {
            let trimmed = buf.trim().to_string();
            parts.push(trimmed);
            operators.push(c.to_string());
            buf.clear();
            i += 1;
            continue;
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
fn has_output_redirection(command: &str) -> Option<String> {
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
            return Some("output redirection (&>)".into());
        }

        // fd redirects: N>, N>>, N>&M, N>&-
        if c.is_ascii_digit() && i + 1 < len && chars[i + 1] == '>' {
            // N>&M or N>&- is fd duplication/closing, not file output
            if i + 2 < len && chars[i + 2] == '&'
                && i + 3 < len && (chars[i + 3].is_ascii_digit() || chars[i + 3] == '-')
            {
                i += 4;
                continue;
            }
            return Some(format!("output redirection ({c}>)"));
        }

        // > or >> but NOT >( (process substitution), >&N, or >&-
        if c == '>' {
            if i + 1 < len && chars[i + 1] == '(' {
                i += 1;
                continue;
            }
            // >&N or >&- is fd duplication/closing
            if i + 1 < len && chars[i + 1] == '&'
                && i + 2 < len && (chars[i + 2].is_ascii_digit() || chars[i + 2] == '-')
            {
                i += 3;
                continue;
            }
            return Some("output redirection (>)".into());
        }

        i += 1;
    }

    None
}

/// Extract the first real command word, skipping leading VAR=value assignments.
fn base_command(command: &str) -> String {
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

/// Extract leading KEY=VALUE pairs.
fn env_vars(command: &str) -> Vec<(String, String)> {
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

/// Extract the git subcommand word (e.g. "push" from "git push origin main").
fn git_subcommand(command: &str) -> Option<String> {
    let mut iter = command.split_whitespace();
    for word in iter.by_ref() {
        if word == "git" {
            return iter.next().map(|s| s.to_string());
        }
    }
    None
}

// ─── Rule sets ───────────────────────────────────────
//
// Adjust these to match your intent.

/// Git subcommands that are auto-allowed WITH AI gitconfig.
const GIT_ALLOWED_WITH_CONFIG: &[&str] = &["push", "pull", "status", "add"];

/// Git subcommands that are read-only and safe without AI gitconfig.
const GIT_READ_ONLY: &[&str] = &[
    "log", "diff", "show", "branch", "tag", "remote",
    "rev-parse", "ls-files", "ls-tree", "shortlog",
    "blame", "describe", "stash",
];

const KUBECTL_READ_ONLY: &[&str] = &[
    "get",
    "describe",
    "logs",
    "top",
    "explain",
    "api-resources",
    "api-versions",
    "version",
    "cluster-info",
];

const KUBECTL_MUTATING: &[&str] = &[
    "apply",
    "delete",
    "rollout",
    "scale",
    "autoscale",
    "patch",
    "replace",
    "create",
    "edit",
    "drain",
    "cordon",
    "uncordon",
    "taint",
    "exec",
    "run",
    "port-forward",
    "cp",
];

const DENY_COMMANDS: &[&str] = &[
    "shred",    // destructive filesystem
    "dd",       // disk operations
    "mkfs", "fdisk", "parted",
    "shutdown", // system control
    "reboot", "halt", "poweroff",
    "eval",     // dynamic execution
];

const ASK_COMMANDS: &[&str] = &[
    "rm", "rmdir",  // destructive but sometimes needed
    "sudo", "su", "doas", "pkexec", // privilege escalation
];

const ALLOW_COMMANDS: &[&str] = &[
    "ls", "tree", "which", "cd", "chdir",
    // Rust-based CLI tools (deployed on ai-dev container)
    "eza",       // ls replacement
    "bat",       // cat replacement
    "fd",        // find replacement
    "rg",        // grep replacement (ripgrep)
    "sd",        // sed replacement
    "dust",      // du replacement
    "procs",     // ps replacement
    "tokei",     // code stats
    "delta",     // diff viewer
    "zoxide",    // cd replacement
    "hyperfine", // benchmarking
    "just",      // command runner
];

/// Cargo subcommands that are safe (build / check / informational).
const CARGO_SAFE: &[&str] = &[
    "build", "check", "test", "bench", "run",
    "clippy", "fmt", "doc", "clean", "update",
    "fetch", "tree", "metadata", "version",
    "verify-project", "search", "generate-lockfile",
];

/// gh CLI subcommands that are read-only.
const GH_READ_ONLY: &[&str] = &[
    "status",
    // repo
    "repo view", "repo list", "repo clone",
    // pr
    "pr list", "pr view", "pr diff", "pr checks", "pr status",
    // issue
    "issue list", "issue view", "issue status",
    // run / workflow
    "run list", "run view", "run watch",
    "workflow list", "workflow view",
    // release
    "release list", "release view",
    // misc read
    "search", "browse", "api",
    "auth status", "auth token",
    "extension list",
    "label list",
    "cache list",
    "variable list", "variable get",
    "secret list",
];

/// gh CLI subcommands that mutate state (create, modify, delete).
const GH_MUTATING: &[&str] = &[
    // repo
    "repo create", "repo delete", "repo edit", "repo fork", "repo rename", "repo archive",
    // pr
    "pr create", "pr merge", "pr close", "pr reopen", "pr comment", "pr review", "pr edit",
    // issue
    "issue create", "issue close", "issue reopen", "issue comment", "issue edit",
    "issue delete", "issue transfer", "issue pin", "issue unpin",
    // run / workflow
    "run rerun", "run cancel", "run delete",
    "workflow enable", "workflow disable", "workflow run",
    // release
    "release create", "release delete", "release edit",
    // misc write
    "auth login", "auth logout", "auth refresh",
    "extension install", "extension remove", "extension upgrade",
    "label create", "label edit", "label delete",
    "cache delete",
    "variable set", "variable delete",
    "secret set", "secret delete",
    "config set",
];

// ─── Evaluation ──────────────────────────────────────

fn in_set(set: &[&str], val: &str) -> bool {
    set.contains(&val)
}

/// Evaluate a single (non-compound) command against the rule set.
fn evaluate_single(command: &str) -> RuleMatch {
    let cmd = command.trim();
    if cmd.is_empty() {
        return RuleMatch {
            decision: Decision::Allow,
            reason: "empty".into(),
        };
    }

    let base = base_command(cmd);
    let envs = env_vars(cmd);
    let redir = has_output_redirection(cmd);
    let has_ai_config = envs.iter().any(|(k, _)| k == "GIT_CONFIG_GLOBAL");

    // ── Deny list (also matches dotted variants like mkfs.ext4) ──
    let base_prefix = base.split('.').next().unwrap_or("");
    if in_set(DENY_COMMANDS, &base) || in_set(DENY_COMMANDS, base_prefix) {
        return RuleMatch {
            decision: Decision::Deny,
            reason: format!("blocked command: {base}"),
        };
    }

    // ── Git ──
    if base == "git" || (has_ai_config && cmd.contains("git")) {
        let sub = git_subcommand(cmd);
        let sub_str = sub.as_deref().unwrap_or("?");

        // Force-push → ask regardless of config
        if sub_str == "push" {
            let force_flags = ["--force", "--force-with-lease", "-f"];
            if cmd.split_whitespace().any(|w| force_flags.contains(&w)) {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: "git force-push requires confirmation".into(),
                };
            }
        }

        // Read-only git subcommands — allowed without AI gitconfig
        if in_set(GIT_READ_ONLY, sub_str) {
            if let Some(r) = redir {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("git {sub_str} with {r}"),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only git {sub_str}"),
            };
        }

        // Write git subcommands require AI gitconfig
        if in_set(GIT_ALLOWED_WITH_CONFIG, sub_str) {
            if has_ai_config {
                if let Some(r) = redir {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("git {sub_str} with {r}"),
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

        return RuleMatch {
            decision: Decision::Ask,
            reason: format!("git {sub_str} requires confirmation"),
        };
    }

    // ── kubectl ──
    if base == "kubectl" {
        let sub = cmd
            .split_whitespace()
            .skip(1)
            .find(|w| !w.starts_with('-'));
        let sub_str = sub.unwrap_or("?");

        if in_set(KUBECTL_READ_ONLY, sub_str) {
            if let Some(r) = redir {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("kubectl {sub_str} with {r}"),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only kubectl {sub_str}"),
            };
        }

        if in_set(KUBECTL_MUTATING, sub_str) {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("kubectl {sub_str} requires confirmation"),
            };
        }

        return RuleMatch {
            decision: Decision::Ask,
            reason: format!("kubectl {sub_str} requires confirmation"),
        };
    }

    // ── cargo ──
    if base == "cargo" {
        let sub = cmd
            .split_whitespace()
            .skip(1)
            .find(|w| !w.starts_with('-'));
        let sub_str = sub.unwrap_or("?");

        if in_set(CARGO_SAFE, sub_str) {
            if let Some(r) = redir {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("cargo {sub_str} with {r}"),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("cargo {sub_str}"),
            };
        }

        // --version / -V at any position
        if cmd.split_whitespace().any(|w| w == "--version" || w == "-V") {
            return RuleMatch {
                decision: Decision::Allow,
                reason: "cargo --version".into(),
            };
        }

        return RuleMatch {
            decision: Decision::Ask,
            reason: format!("cargo {sub_str} requires confirmation"),
        };
    }

    // ── gh CLI ──
    if base == "gh" {
        let words: Vec<&str> = cmd.split_whitespace().collect();
        // gh subcommands are two words (e.g. "pr list") or one word (e.g. "status")
        let sub_two = if words.len() >= 3 {
            format!("{} {}", words[1], words[2])
        } else {
            String::new()
        };
        let sub_one = words.get(1).copied().unwrap_or("?");

        if in_set(GH_READ_ONLY, &sub_two) || in_set(GH_READ_ONLY, sub_one) {
            if let Some(r) = redir {
                return RuleMatch {
                    decision: Decision::Ask,
                    reason: format!("gh {sub_one} with {r}"),
                };
            }
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("read-only gh {sub_two}"),
            };
        }

        if in_set(GH_MUTATING, &sub_two) || in_set(GH_MUTATING, sub_one) {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("gh {sub_two} requires confirmation"),
            };
        }

        return RuleMatch {
            decision: Decision::Ask,
            reason: format!("gh {sub_one} requires confirmation"),
        };
    }

    // ── Ask list (destructive / privileged) ──
    if in_set(ASK_COMMANDS, &base) {
        return RuleMatch {
            decision: Decision::Ask,
            reason: format!("{base} requires confirmation"),
        };
    }

    // ── Allow list ──
    if in_set(ALLOW_COMMANDS, &base) {
        if let Some(r) = redir {
            return RuleMatch {
                decision: Decision::Ask,
                reason: format!("{base} with {r}"),
            };
        }
        return RuleMatch {
            decision: Decision::Allow,
            reason: format!("allowed: {base}"),
        };
    }

    // ── --version flag on any command (short invocations only) ──
    {
        if cmd.split_whitespace().count() <= 3
            && cmd.split_whitespace().any(|w| w == "--version" || w == "-V")
        {
            return RuleMatch {
                decision: Decision::Allow,
                reason: format!("{base} --version"),
            };
        }
    }

    // ── Fallthrough → ask ──
    RuleMatch {
        decision: Decision::Ask,
        reason: format!("unrecognized command: {base}"),
    }
}

fn decision_label(d: Decision) -> &'static str {
    match d {
        Decision::Allow => "ALLOW",
        Decision::Ask => "ASK",
        Decision::Deny => "DENY",
    }
}

/// Evaluate a full command string, handling compound expressions and substitutions.
///
/// Substitutions (`$(...)`, backticks, `<()`, `>()`) are extracted and recursively
/// evaluated. The outer command (with placeholders) and each compound part are
/// evaluated via `evaluate_single`. Worst decision wins across all parts.
fn evaluate(command: &str) -> RuleMatch {
    let (outer, inners) = extract_substitutions(command);
    let (parts, operators) = split_compound_command(&outer);

    // Simple case: no substitutions and not compound → evaluate directly
    if parts.len() <= 1 && inners.is_empty() {
        return evaluate_single(command);
    }

    let mut worst = Decision::Allow;
    let mut reasons = Vec::new();

    // Recursively evaluate substitution contents
    for inner in &inners {
        let result = evaluate(inner);
        let label: String = inner.trim().chars().take(60).collect();
        reasons.push(format!(
            "  subst[$({label})] -> {}: {}",
            decision_label(result.decision),
            result.reason
        ));
        if result.decision > worst {
            worst = result.decision;
        }
    }

    // Evaluate each part of the (possibly compound) outer command
    for part in &parts {
        let result = evaluate_single(part);
        let label: String = part.trim().chars().take(60).collect();
        reasons.push(format!(
            "  [{label}] -> {}: {}",
            decision_label(result.decision),
            result.reason
        ));
        if result.decision > worst {
            worst = result.decision;
        }
    }

    // Build summary header
    let mut desc = Vec::new();
    if !operators.is_empty() {
        let mut unique_ops: Vec<&str> = operators.iter().map(|s| s.as_str()).collect();
        unique_ops.sort();
        unique_ops.dedup();
        desc.push(unique_ops.join(", "));
    }
    if !inners.is_empty() {
        desc.push(format!("{} substitution(s)", inners.len()));
    }
    let header = if desc.is_empty() {
        "compound command".into()
    } else {
        format!("compound command ({})", desc.join("; "))
    };

    RuleMatch {
        decision: worst,
        reason: format!("{}:\n{}", header, reasons.join("\n")),
    }
}

// ─── Entry point ─────────────────────────────────────

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("failed to read stdin");
        std::process::exit(1);
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("JSON parse error: {e}");
            std::process::exit(1);
        }
    };

    if hook_input.tool_name.as_deref() != Some("Bash") {
        std::process::exit(0);
    }

    let command = hook_input
        .tool_input
        .and_then(|t| t.command)
        .unwrap_or_default();

    if command.is_empty() {
        std::process::exit(0);
    }

    let result = evaluate(&command);

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": result.decision.as_str(),
            "permissionDecisionReason": result.reason,
        }
    });

    println!("{}", serde_json::to_string(&output).unwrap());
}

// ─── Tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn decision_for(command: &str) -> Decision {
        evaluate(command).decision
    }

    fn reason_for(command: &str) -> String {
        evaluate(command).reason
    }

    // ── ALLOW ──

    #[test]
    fn allow_simple_ls() {
        assert_eq!(decision_for("ls -la"), Decision::Allow);
    }

    #[test]
    fn allow_tree() {
        assert_eq!(decision_for("tree /tmp"), Decision::Allow);
    }

    #[test]
    fn allow_which() {
        assert_eq!(decision_for("which cargo"), Decision::Allow);
    }

    #[test]
    fn allow_eza() {
        assert_eq!(decision_for("eza --icons --git"), Decision::Allow);
    }

    #[test]
    fn allow_bat() {
        assert_eq!(decision_for("bat README.md"), Decision::Allow);
    }

    #[test]
    fn allow_rg() {
        assert_eq!(decision_for("rg 'pattern' src/"), Decision::Allow);
    }

    #[test]
    fn allow_fd() {
        assert_eq!(decision_for("fd '*.rs' src/"), Decision::Allow);
    }

    #[test]
    fn allow_dust() {
        assert_eq!(decision_for("dust /home"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_get() {
        assert_eq!(decision_for("kubectl get pods"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_describe() {
        assert_eq!(decision_for("kubectl describe svc foo"), Decision::Allow);
    }

    #[test]
    fn allow_kubectl_logs() {
        assert_eq!(decision_for("kubectl logs pod/foo"), Decision::Allow);
    }

    #[test]
    fn allow_git_push_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_pull_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git pull origin main"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_status_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git status"),
            Decision::Allow
        );
    }

    #[test]
    fn allow_git_add_with_config() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git add ."),
            Decision::Allow
        );
    }

    #[test]
    fn allow_cargo_build() {
        assert_eq!(decision_for("cargo build --release"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_test() {
        assert_eq!(decision_for("cargo test"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_check() {
        assert_eq!(decision_for("cargo check"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_clippy() {
        assert_eq!(decision_for("cargo clippy"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_fmt() {
        assert_eq!(decision_for("cargo fmt"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_version() {
        assert_eq!(decision_for("cargo --version"), Decision::Allow);
    }

    #[test]
    fn allow_cargo_version_short() {
        assert_eq!(decision_for("cargo -V"), Decision::Allow);
    }

    #[test]
    fn allow_generic_version() {
        assert_eq!(decision_for("rustc --version"), Decision::Allow);
    }

    #[test]
    fn allow_generic_version_short() {
        assert_eq!(decision_for("node -V"), Decision::Allow);
    }

    // ── ASK ──

    #[test]
    fn ask_rm() {
        assert_eq!(decision_for("rm -rf /tmp/junk"), Decision::Ask);
    }

    #[test]
    fn ask_rmdir() {
        assert_eq!(decision_for("rmdir /tmp/empty"), Decision::Ask);
    }

    #[test]
    fn ask_sudo() {
        assert_eq!(decision_for("sudo apt install vim"), Decision::Ask);
    }

    #[test]
    fn ask_su() {
        assert_eq!(decision_for("su - root"), Decision::Ask);
    }

    #[test]
    fn ask_doas() {
        assert_eq!(decision_for("doas pacman -S vim"), Decision::Ask);
    }

    #[test]
    fn ask_git_push_no_config() {
        assert_eq!(decision_for("git push origin main"), Decision::Ask);
    }

    #[test]
    fn ask_git_pull_no_config() {
        assert_eq!(decision_for("git pull origin main"), Decision::Ask);
    }

    #[test]
    fn ask_force_push() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_force_push_short_flag() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push -f origin main"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_force_push_with_lease() {
        assert_eq!(
            decision_for(
                "GIT_CONFIG_GLOBAL=~/.gitconfig.ai git push --force-with-lease origin main"
            ),
            Decision::Ask
        );
    }

    #[test]
    fn ask_git_commit() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git commit -m 'test'"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_kubectl_apply() {
        assert_eq!(decision_for("kubectl apply -f deploy.yaml"), Decision::Ask);
    }

    #[test]
    fn ask_kubectl_delete() {
        assert_eq!(decision_for("kubectl delete pod foo"), Decision::Ask);
    }

    #[test]
    fn ask_kubectl_rollout() {
        assert_eq!(
            decision_for("kubectl rollout restart deploy/foo"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_kubectl_scale() {
        assert_eq!(
            decision_for("kubectl scale --replicas=3 deploy/foo"),
            Decision::Ask
        );
    }

    #[test]
    fn ask_unrecognized() {
        assert_eq!(decision_for("unknown-command --flag"), Decision::Ask);
    }

    #[test]
    fn ask_cargo_install() {
        assert_eq!(decision_for("cargo install ripgrep"), Decision::Ask);
    }

    #[test]
    fn ask_cargo_publish() {
        assert_eq!(decision_for("cargo publish"), Decision::Ask);
    }

    // ── Redirection escalation ──

    #[test]
    fn redir_ls_stdout() {
        assert_eq!(decision_for("ls -la > /tmp/out.txt"), Decision::Ask);
    }

    #[test]
    fn redir_ls_append() {
        assert_eq!(decision_for("ls -la >> /tmp/out.txt"), Decision::Ask);
    }

    #[test]
    fn redir_eza() {
        assert_eq!(decision_for("eza --icons > files.txt"), Decision::Ask);
    }

    #[test]
    fn redir_kubectl_get() {
        assert_eq!(decision_for("kubectl get pods > pods.txt"), Decision::Ask);
    }

    #[test]
    fn redir_git_status() {
        assert_eq!(
            decision_for("GIT_CONFIG_GLOBAL=~/.gitconfig.ai git status > /tmp/s.txt"),
            Decision::Ask
        );
    }

    #[test]
    fn redir_stderr() {
        assert_eq!(decision_for("bat file 2> /tmp/err"), Decision::Ask);
    }

    #[test]
    fn redir_combined() {
        assert_eq!(decision_for("bat file &> /tmp/out"), Decision::Ask);
    }

    #[test]
    fn redir_cargo_build() {
        assert_eq!(
            decision_for("cargo build --release > /tmp/log"),
            Decision::Ask
        );
    }

    // ── fd duplication (NOT mutation) ──

    #[test]
    fn fd_dup_2_to_1() {
        // 2>&1 is fd duplication, not file output → should not escalate
        assert_eq!(decision_for("ls -la 2>&1"), Decision::Allow);
    }

    #[test]
    fn fd_dup_1_to_2() {
        assert_eq!(decision_for("ls -la 1>&2"), Decision::Allow);
    }

    #[test]
    fn fd_dup_bare_to_2() {
        // >&2 is shorthand for 1>&2
        assert_eq!(decision_for("ls -la >&2"), Decision::Allow);
    }

    #[test]
    fn fd_close_2() {
        // 2>&- closes stderr
        assert_eq!(decision_for("ls -la 2>&-"), Decision::Allow);
    }

    #[test]
    fn fd_dup_with_real_redir() {
        // 2>&1 is fine but > file is still mutation
        assert_eq!(decision_for("ls -la > /tmp/out 2>&1"), Decision::Ask);
    }

    #[test]
    fn fd_dup_cargo_test() {
        // Common pattern: cargo test 2>&1 | rg FAILED
        assert_eq!(
            decision_for("cargo test 2>&1 | rg FAILED"),
            Decision::Allow
        );
    }

    // ── DENY ──

    #[test]
    fn deny_shred() {
        assert_eq!(decision_for("shred /dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_dd() {
        assert_eq!(decision_for("dd if=/dev/zero of=/dev/sda"), Decision::Deny);
    }

    #[test]
    fn deny_eval() {
        assert_eq!(decision_for("eval 'rm -rf /'"), Decision::Deny);
    }

    #[test]
    fn deny_shutdown() {
        assert_eq!(decision_for("shutdown -h now"), Decision::Deny);
    }

    #[test]
    fn deny_reboot() {
        assert_eq!(decision_for("reboot"), Decision::Deny);
    }

    #[test]
    fn deny_halt() {
        assert_eq!(decision_for("halt"), Decision::Deny);
    }

    #[test]
    fn deny_mkfs_dotted() {
        assert_eq!(decision_for("mkfs.ext4 /dev/sda1"), Decision::Deny);
    }

    // ── Compound commands ──

    #[test]
    fn chain_allow_and_ask() {
        assert_eq!(decision_for("ls -la && rm -rf /tmp"), Decision::Ask);
    }

    #[test]
    fn chain_allow_and_deny() {
        assert_eq!(decision_for("ls -la && shred foo"), Decision::Deny);
    }

    #[test]
    fn chain_allow_and_allow() {
        assert_eq!(decision_for("ls -la ; eza --icons"), Decision::Allow);
    }

    #[test]
    fn chain_kubectl_allow_and_allow() {
        assert_eq!(
            decision_for("kubectl get pods ; kubectl get svc"),
            Decision::Allow
        );
    }

    #[test]
    fn chain_tree_and_bat() {
        assert_eq!(decision_for("tree . && bat README.md"), Decision::Allow);
    }

    #[test]
    fn chain_kubectl_allow_and_ask() {
        assert_eq!(
            decision_for("kubectl get pods && kubectl delete pod foo"),
            Decision::Ask
        );
    }

    #[test]
    fn chain_allow_and_deny_dd() {
        assert_eq!(
            decision_for("ls -la ; dd if=/dev/zero of=disk"),
            Decision::Deny
        );
    }

    // ── Pipes ──

    #[test]
    fn pipe_allow_allow() {
        assert_eq!(
            decision_for("kubectl get pods | rg running"),
            Decision::Allow
        );
    }

    #[test]
    fn pipe_allow_allow_bat() {
        assert_eq!(decision_for("ls -la | bat"), Decision::Allow);
    }

    #[test]
    fn pipe_allow_ask() {
        assert_eq!(decision_for("eza | unknown-tool"), Decision::Ask);
    }

    // ── Substitution (recursive evaluation) ──

    #[test]
    fn subst_both_allowed() {
        // ls is ALLOW, which is ALLOW → overall ALLOW
        assert_eq!(decision_for("ls $(which cargo)"), Decision::Allow);
    }

    #[test]
    fn subst_both_allowed_bat_fd() {
        assert_eq!(decision_for("bat $(fd '*.rs' src/)"), Decision::Allow);
    }

    #[test]
    fn subst_inner_ask() {
        // ls is ALLOW, rm is ASK → overall ASK
        assert_eq!(decision_for("ls $(rm -rf /tmp)"), Decision::Ask);
    }

    #[test]
    fn subst_inner_deny() {
        // ls is ALLOW, shred is DENY → overall DENY
        assert_eq!(decision_for("ls $(shred foo)"), Decision::Deny);
    }

    #[test]
    fn subst_outer_unrecognized() {
        // echo is unrecognized (ASK), cat is unrecognized (ASK)
        assert_eq!(decision_for("echo $(cat /etc/passwd)"), Decision::Ask);
    }

    #[test]
    fn subst_backtick_unrecognized() {
        // echo is unrecognized (ASK), whoami is unrecognized (ASK)
        assert_eq!(decision_for("echo `whoami`"), Decision::Ask);
    }

    #[test]
    fn subst_nested() {
        // Inner: cat $(which foo) → which is ALLOW, cat is unrecognized (ASK)
        // Outer: ls __SUBST__ → ALLOW
        // Overall: ASK
        assert_eq!(decision_for("ls $(cat $(which foo))"), Decision::Ask);
    }

    #[test]
    fn subst_single_quoted_not_expanded() {
        // Single quotes prevent substitution extraction
        // echo is unrecognized → ASK, but NOT because of subst
        assert_eq!(decision_for("echo '$(rm -rf /)'"), Decision::Ask);
        let r = reason_for("echo '$(rm -rf /)'");
        assert!(
            r.contains("unrecognized"),
            "should be unrecognized (single quotes block subst): {r}"
        );
    }

    #[test]
    fn subst_double_quoted_expanded() {
        // Double quotes do NOT block $() expansion
        // Inner: rm -rf / → ASK
        assert_eq!(decision_for("echo \"$(rm -rf /)\""), Decision::Ask);
        let r = reason_for("echo \"$(rm -rf /)\"");
        assert!(
            r.contains("subst"),
            "should show substitution evaluation: {r}"
        );
    }

    #[test]
    fn subst_process_subst_no_false_redir() {
        // Process substitution >() should NOT trigger output redirection detection
        // diff is unrecognized (ASK), sort is unrecognized (ASK)
        assert_eq!(decision_for("diff <(sort a) <(sort b)"), Decision::Ask);
        let r = reason_for("diff <(sort a) <(sort b)");
        assert!(
            !r.contains("redirection"),
            "process substitution should not trigger redirection: {r}"
        );
    }

    #[test]
    fn subst_in_compound_allow() {
        // ls $(which cargo) && bat $(fd '*.rs')
        // All parts and substitutions are ALLOW
        assert_eq!(
            decision_for("ls $(which cargo) && bat $(fd '*.rs')"),
            Decision::Allow
        );
    }

    #[test]
    fn subst_in_compound_deny() {
        // ls $(shred foo) && bat README.md
        // Inner shred is DENY → overall DENY
        assert_eq!(
            decision_for("ls $(shred foo) && bat README.md"),
            Decision::Deny
        );
    }

    // ── Quoting ──

    #[test]
    fn quoted_redirect_single() {
        // > inside single quotes is NOT real redirection; echo itself is unrecognized → ask
        assert_eq!(decision_for("echo 'hello > world'"), Decision::Ask);
        let r = reason_for("echo 'hello > world'");
        assert!(
            r.contains("unrecognized"),
            "should be unrecognized, not redirection: {r}"
        );
    }

    #[test]
    fn quoted_chain_double() {
        // && inside double quotes should NOT split
        assert_eq!(decision_for("echo \"a && b\""), Decision::Ask);
        let r = reason_for("echo \"a && b\"");
        assert!(
            r.contains("unrecognized"),
            "should be a single command, not compound: {r}"
        );
    }

    // ── Cargo compound ──

    #[test]
    fn cargo_build_and_test() {
        assert_eq!(
            decision_for("cargo build --release && cargo test"),
            Decision::Allow
        );
    }

    #[test]
    fn cargo_fmt_and_clippy() {
        assert_eq!(
            decision_for("cargo fmt && cargo clippy"),
            Decision::Allow
        );
    }

    // ── which ──

    #[test]
    fn allow_which_single() {
        assert_eq!(decision_for("which python"), Decision::Allow);
    }

    #[test]
    fn allow_which_multiple() {
        assert_eq!(decision_for("which cargo rustc gcc"), Decision::Allow);
    }

    // ── cd / chdir ──

    #[test]
    fn allow_cd() {
        assert_eq!(decision_for("cd /tmp"), Decision::Allow);
    }

    #[test]
    fn allow_chdir() {
        assert_eq!(decision_for("chdir /home/user"), Decision::Allow);
    }

    // ── Git read-only (no config required) ──

    #[test]
    fn allow_git_log() {
        assert_eq!(decision_for("git log --oneline -10"), Decision::Allow);
    }

    #[test]
    fn allow_git_diff() {
        assert_eq!(decision_for("git diff HEAD~1"), Decision::Allow);
    }

    #[test]
    fn allow_git_show() {
        assert_eq!(decision_for("git show HEAD"), Decision::Allow);
    }

    #[test]
    fn allow_git_branch() {
        assert_eq!(decision_for("git branch -a"), Decision::Allow);
    }

    #[test]
    fn allow_git_blame() {
        assert_eq!(decision_for("git blame src/main.rs"), Decision::Allow);
    }

    #[test]
    fn allow_git_stash() {
        assert_eq!(decision_for("git stash list"), Decision::Allow);
    }

    #[test]
    fn redir_git_log() {
        assert_eq!(decision_for("git log > /tmp/log.txt"), Decision::Ask);
    }

    // ── gh CLI read-only ──

    #[test]
    fn allow_gh_pr_list() {
        assert_eq!(decision_for("gh pr list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_view() {
        assert_eq!(decision_for("gh pr view 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_diff() {
        assert_eq!(decision_for("gh pr diff 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_pr_checks() {
        assert_eq!(decision_for("gh pr checks 123"), Decision::Allow);
    }

    #[test]
    fn allow_gh_issue_list() {
        assert_eq!(decision_for("gh issue list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_issue_view() {
        assert_eq!(decision_for("gh issue view 42"), Decision::Allow);
    }

    #[test]
    fn allow_gh_repo_view() {
        assert_eq!(decision_for("gh repo view owner/repo"), Decision::Allow);
    }

    #[test]
    fn allow_gh_run_list() {
        assert_eq!(decision_for("gh run list"), Decision::Allow);
    }

    #[test]
    fn allow_gh_status() {
        assert_eq!(decision_for("gh status"), Decision::Allow);
    }

    #[test]
    fn allow_gh_search() {
        assert_eq!(decision_for("gh search repos rust"), Decision::Allow);
    }

    #[test]
    fn allow_gh_api() {
        assert_eq!(decision_for("gh api repos/owner/repo/pulls"), Decision::Allow);
    }

    #[test]
    fn allow_gh_auth_status() {
        assert_eq!(decision_for("gh auth status"), Decision::Allow);
    }

    // ── gh CLI mutating ──

    #[test]
    fn ask_gh_pr_create() {
        assert_eq!(decision_for("gh pr create --title 'Fix'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_merge() {
        assert_eq!(decision_for("gh pr merge 123"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_close() {
        assert_eq!(decision_for("gh pr close 123"), Decision::Ask);
    }

    #[test]
    fn ask_gh_pr_comment() {
        assert_eq!(decision_for("gh pr comment 123 --body 'LGTM'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_issue_create() {
        assert_eq!(decision_for("gh issue create --title 'Bug'"), Decision::Ask);
    }

    #[test]
    fn ask_gh_repo_create() {
        assert_eq!(decision_for("gh repo create my-repo --public"), Decision::Ask);
    }

    #[test]
    fn ask_gh_release_create() {
        assert_eq!(decision_for("gh release create v1.0"), Decision::Ask);
    }

    #[test]
    fn ask_gh_repo_delete() {
        assert_eq!(decision_for("gh repo delete my-repo --yes"), Decision::Ask);
    }

    // ── gh CLI redirection escalation ──

    #[test]
    fn redir_gh_pr_list() {
        assert_eq!(decision_for("gh pr list > /tmp/prs.txt"), Decision::Ask);
    }

    // ── Compound with new rules ──

    #[test]
    fn chain_git_log_and_gh_pr_list() {
        assert_eq!(
            decision_for("git log --oneline -5 && gh pr list"),
            Decision::Allow
        );
    }

    #[test]
    fn chain_cd_and_ls() {
        assert_eq!(decision_for("cd /tmp && ls -la"), Decision::Allow);
    }
}
