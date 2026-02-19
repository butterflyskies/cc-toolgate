//! Shell command parsing backed by tree-sitter-bash.
//!
//! This module provides two public functions:
//!
//! - [`parse_with_substitutions`] — decomposes a shell command string into a
//!   [`ParsedPipeline`] of segments joined by operators, plus a list of
//!   extracted command/process substitution contents.
//!
//! - [`has_output_redirection`] — checks whether a command string contains
//!   output redirection that could mutate filesystem state.
//!
//! Both functions parse their input with tree-sitter-bash, which provides a
//! full AST from a formal grammar.  This means quoting, heredocs, control flow
//! keywords, and nested substitutions are handled by the grammar itself —
//! the code here walks the resulting AST rather than scanning characters.
//!
//! # Control flow handling
//!
//! Shell keywords (`for`, `if`, `while`, `case`) are grammar structure, not
//! commands.  The AST walker recurses into control flow bodies and extracts the
//! actual commands inside them as pipeline segments.  For example,
//! `for i in *; do rm "$i"; done` produces a segment for `rm "$i"`, not for
//! `for` or `done`.
//!
//! # Redirection propagation
//!
//! When a control flow construct is wrapped in a `redirected_statement`
//! (e.g. `for ... done > file`), the output redirection is propagated to the
//! inner segments via [`ShellSegment::redirection`].  The eval layer uses this
//! field to escalate decisions for commands that are contextually mutating even
//! though their own text contains no redirect.
//!
//! # Substitution extraction
//!
//! Outermost `$()`, backtick, `<()`, and `>()` nodes are collected and their
//! spans replaced with `__SUBST__` placeholders in the segment text.  The eval
//! layer recursively evaluates each substitution's inner command independently.

use super::types::{Operator, ParsedPipeline, Redirection, ShellSegment};
use std::cell::RefCell;
use tree_sitter::{Node, Parser, Tree};

// ---------------------------------------------------------------------------
// Thread-local parser
// ---------------------------------------------------------------------------

thread_local! {
    /// tree-sitter `Parser` is `!Send`, so we use `thread_local!` storage.
    static TS_PARSER: RefCell<Parser> = RefCell::new({
        let mut p = Parser::new();
        p.set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("failed to load bash grammar");
        p
    });
}

/// Parse `source` into a tree-sitter syntax tree.
fn parse_tree(source: &str) -> Tree {
    TS_PARSER.with(|p| {
        p.borrow_mut()
            .parse(source, None)
            .expect("tree-sitter parse failed")
    })
}

// ---------------------------------------------------------------------------
// Substitution extraction
// ---------------------------------------------------------------------------

/// A substitution's byte range in the source and its inner command text.
struct SubstSpan {
    start: usize,
    end: usize,
    inner: String,
}

/// Walk `node` for outermost `command_substitution` and `process_substitution`
/// nodes, appending each to `out`.  Does not recurse into found substitutions;
/// the eval layer handles nested evaluation.
fn collect_substitutions(node: Node, source: &[u8], out: &mut Vec<SubstSpan>) {
    if matches!(node.kind(), "command_substitution" | "process_substitution") {
        let full = node.utf8_text(source).unwrap_or("");
        let inner = strip_subst_delimiters(full);
        if !inner.is_empty() {
            out.push(SubstSpan {
                start: node.start_byte(),
                end: node.end_byte(),
                inner: inner.to_string(),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_substitutions(child, source, out);
    }
}

/// Strip the outer delimiters from a substitution node's text.
///
/// `$(cmd)` → `cmd`, `` `cmd` `` → `cmd`, `<(cmd)` / `>(cmd)` → `cmd`.
fn strip_subst_delimiters(text: &str) -> &str {
    let t = if text.starts_with("$(") || text.starts_with("<(") || text.starts_with(">(") {
        text.get(2..text.len().saturating_sub(1)).unwrap_or("")
    } else if text.starts_with('`') && text.ends_with('`') && text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    };
    t.trim()
}

/// Produce the text of `source[start..end]` with any substitution spans inside
/// that range replaced by `__SUBST__` placeholders.  Replacement is performed
/// right-to-left so that earlier byte offsets remain valid.
fn text_replacing_substitutions(
    source: &str,
    start: usize,
    end: usize,
    subs: &[SubstSpan],
) -> String {
    let mut relevant: Vec<&SubstSpan> = subs
        .iter()
        .filter(|s| s.start >= start && s.end <= end)
        .collect();
    if relevant.is_empty() {
        return source[start..end].to_string();
    }
    relevant.sort_by(|a, b| b.start.cmp(&a.start));
    let mut text = source[start..end].to_string();
    for sub in relevant {
        text.replace_range((sub.start - start)..(sub.end - start), "__SUBST__");
    }
    text
}

// ---------------------------------------------------------------------------
// Output redirection detection
// ---------------------------------------------------------------------------

/// Inspect a `file_redirect` AST node and decide whether it represents an
/// output redirection that could mutate filesystem state.
///
/// # Safe patterns (returns `None`)
///
/// - Input redirects: `<`, `<<`, `<<<`, `<&`
/// - Any redirect targeting `/dev/null`
/// - fd duplication to standard streams: `>&1`, `>&2`, `2>&1`, etc.
/// - fd closing: `>&-`, `2>&-`
///
/// # Flagged patterns (returns `Some`)
///
/// - `>`, `>>`, `>|` to any path other than `/dev/null`
/// - `<>` (read-write open, detected via ERROR node in tree-sitter AST)
/// - `&>`, `&>>` to any path other than `/dev/null`
/// - `>&N` or `M>&N` where N ≥ 3 (custom fd target)
/// - `N>` or `N>>` to any path other than `/dev/null`
fn check_file_redirect(node: Node, source: &[u8]) -> Option<Redirection> {
    let mut fd: Option<String> = None;
    let mut operator = "";
    let mut dest = String::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "file_descriptor" {
            fd = child.utf8_text(source).ok().map(str::to_string);
        } else if child.is_named() {
            dest = child.utf8_text(source).unwrap_or("").to_string();
        } else {
            let k = child.kind();
            if matches!(
                k,
                ">" | ">>" | ">|" | "&>" | "&>>" | ">&" | "<" | "<<<" | "<<" | "<&"
            ) {
                operator = k;
            }
        }
    }

    if matches!(operator, "" | "<" | "<<<" | "<<" | "<&") {
        // tree-sitter-bash parses `<>` (read-write) as `<` + ERROR(`>`).
        // Detect this by checking the node's raw text for the `<>` sequence.
        if operator == "<" {
            let text = node.utf8_text(source).unwrap_or("");
            if text.contains("<>") {
                return Some(Redirection {
                    description: "output redirection (<> read-write)".into(),
                });
            }
        }
        return None;
    }

    if matches!(operator, "&>" | "&>>") {
        if dest == "/dev/null" {
            return None;
        }
        return Some(Redirection {
            description: format!("output redirection ({operator})"),
        });
    }

    if operator == ">&" {
        if dest == "-" {
            return None;
        }
        if let Some(ref f) = fd {
            if matches!(dest.as_str(), "0" | "1" | "2") {
                return None;
            }
            return Some(Redirection {
                description: format!("output redirection ({f}>&{dest}, custom fd target)"),
            });
        }
        if matches!(dest.as_str(), "0" | "1" | "2") {
            return None;
        }
        return Some(Redirection {
            description: format!("output redirection (>&{dest}, custom fd target)"),
        });
    }

    if matches!(operator, ">" | ">>" | ">|") {
        if dest == "/dev/null" {
            return None;
        }
        if let Some(ref f) = fd {
            return Some(Redirection {
                description: format!("output redirection ({f}{operator})"),
            });
        }
        return Some(Redirection {
            description: format!("output redirection ({operator})"),
        });
    }

    None
}

/// Recursively search `node` for `file_redirect` descendants, returning the
/// first output redirection found.  Skips `heredoc_body` subtrees entirely so
/// that text inside heredoc bodies (e.g. email addresses containing `>`) never
/// triggers a false positive.
fn detect_redirections(node: Node, source: &[u8]) -> Option<Redirection> {
    if node.kind() == "file_redirect" {
        return check_file_redirect(node, source);
    }
    if node.kind() == "heredoc_body" {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(r) = detect_redirections(child, source) {
            return Some(r);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// AST walking — compound command decomposition
// ---------------------------------------------------------------------------

/// Intermediate result of walking a subtree: a flat sequence of segment byte
/// ranges interleaved with operators.
struct WalkResult {
    segments: Vec<SegmentInfo>,
    operators: Vec<Operator>,
}

/// A segment's position in the source and any redirection inherited from a
/// wrapping `redirected_statement`.
struct SegmentInfo {
    start: usize,
    end: usize,
    redirection: Option<Redirection>,
}

impl WalkResult {
    fn empty() -> Self {
        Self {
            segments: vec![],
            operators: vec![],
        }
    }

    fn single(start: usize, end: usize, redir: Option<Redirection>) -> Self {
        Self {
            segments: vec![SegmentInfo {
                start,
                end,
                redirection: redir,
            }],
            operators: vec![],
        }
    }

    /// Merge `other` into `self`, inserting `join_op` between the two if both
    /// contain segments.
    fn append(&mut self, other: WalkResult, join_op: Option<Operator>) {
        if other.segments.is_empty() {
            return;
        }
        if !self.segments.is_empty()
            && let Some(op) = join_op
        {
            self.operators.push(op);
        }
        self.segments.extend(other.segments);
        self.operators.extend(other.operators);
    }
}

/// Dispatch on the AST `node` kind and return a flat segment/operator sequence.
///
/// The match arms fall into three categories:
///
/// 1. **Structure nodes** (`program`, `list`, `pipeline`) — decompose into
///    children connected by operators.
/// 2. **Leaf command nodes** (`command`, `declaration_command`,
///    `variable_assignment`) — become a single segment whose byte range is the
///    node's span.
/// 3. **Control flow nodes** (`for_statement`, `if_statement`, etc.) — recurse
///    into their body to extract the actual commands.
///
/// Unknown named nodes are treated as single segments (conservative: the eval
/// layer will flag them as unrecognized → ASK).
fn walk_ast(node: Node, source: &[u8]) -> WalkResult {
    match node.kind() {
        "program" => walk_program(node, source),
        "list" => walk_list(node, source),
        "pipeline" => walk_pipeline(node, source),
        "command" | "declaration_command" => {
            let redir = detect_redirections(node, source);
            WalkResult::single(node.start_byte(), node.end_byte(), redir)
        }
        "redirected_statement" => walk_redirected(node, source),
        "for_statement" | "while_statement" | "until_statement" | "c_style_for_statement" => {
            walk_loop(node, source)
        }
        "if_statement" => walk_if(node, source),
        "case_statement" => walk_case(node, source),
        "subshell" | "compound_statement" | "do_group" | "else_clause" | "elif_clause" => {
            walk_block(node, source)
        }
        "case_item" => walk_case_item(node, source),
        "negated_command" => walk_negated(node, source),
        "function_definition" => walk_function(node, source),
        "variable_assignment" => WalkResult::single(node.start_byte(), node.end_byte(), None),
        "comment" | "heredoc_body" => WalkResult::empty(),
        _ if node.is_named() => WalkResult::single(node.start_byte(), node.end_byte(), None),
        _ => WalkResult::empty(),
    }
}

/// Top-level `program` node: join named children with [`Operator::Semi`].
fn walk_program(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        result.append(walk_ast(child, source), Some(Operator::Semi));
    }
    result
}

/// `list` is a left-recursive binary tree: `a && b || c` parses as
/// `list(list(a, &&, b), ||, c)`.  This function flattens it into a linear
/// segment/operator sequence.
fn walk_list(node: Node, source: &[u8]) -> WalkResult {
    let mut cursor = node.walk();
    let named: Vec<Node> = node.named_children(&mut cursor).collect();
    if named.len() < 2 {
        let mut result = WalkResult::empty();
        for child in named {
            result.append(walk_ast(child, source), Some(Operator::Semi));
        }
        return result;
    }
    let op = list_operator(node);
    let mut result = walk_ast(named[0], source);
    result.append(walk_ast(named[1], source), Some(op));
    result
}

/// Extract the operator from a `list` node's anonymous children.
fn list_operator(node: Node) -> Operator {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            match child.kind() {
                "&&" => return Operator::And,
                "||" => return Operator::Or,
                _ => {}
            }
        }
    }
    Operator::Semi
}

/// `pipeline` node: named children are commands, anonymous `|` / `|&` tokens
/// are the operators between them.
fn walk_pipeline(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut pending_op: Option<Operator> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            result.append(walk_ast(child, source), pending_op.take());
        } else {
            match child.kind() {
                "|" => pending_op = Some(Operator::Pipe),
                "|&" => pending_op = Some(Operator::PipeErr),
                _ => {}
            }
        }
    }
    result
}

/// `redirected_statement` wraps a body node (command, pipeline, control flow,
/// etc.) together with one or more redirect nodes (`file_redirect`,
/// `heredoc_redirect`, `herestring_redirect`).
///
/// For a leaf command body, the full `redirected_statement` text (minus any
/// heredoc body content) becomes the segment text — this preserves redirect
/// tokens like `> file` in the text that downstream `base_command()` and
/// `has_output_redirection()` will see.
///
/// For a compound body (e.g. `for ... done > file`), the walker recurses into
/// the body and propagates the detected redirection to each inner segment.
///
/// `heredoc_redirect` nodes may contain same-line pipeline/list children
/// (e.g. `cat <<EOF | grep foo` produces a `pipeline` inside
/// `heredoc_redirect`).  These are checked **first** because the body
/// command (e.g. `cat`) appears as an earlier sibling and would otherwise
/// trigger the leaf-command short-circuit.
fn walk_redirected(node: Node, source: &[u8]) -> WalkResult {
    let redir = detect_redirections(node, source);

    // First pass: check if any heredoc_redirect contains same-line commands.
    // This must run before the leaf-command path because tree-sitter places
    // `cat <<EOF | cmd` as: redirected_statement { command("cat"),
    // heredoc_redirect { pipeline("| cmd") } }.  The command("cat") child
    // would otherwise trigger an early return.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "heredoc_redirect" {
            let inner = walk_heredoc_redirect(child, source);
            if !inner.segments.is_empty() {
                let mut full = WalkResult::empty();
                // Find the body command (sibling before heredoc_redirect).
                let mut c2 = node.walk();
                for sib in node.named_children(&mut c2) {
                    if sib.kind() == "heredoc_redirect" {
                        break;
                    }
                    if matches!(sib.kind(), "file_redirect" | "herestring_redirect") {
                        continue;
                    }
                    if is_leaf_command(sib) {
                        let end = effective_end(node).min(child.start_byte());
                        full.append(
                            WalkResult::single(sib.start_byte(), end, redir.clone()),
                            None,
                        );
                    } else {
                        // Compound body (for/while/if/case): recurse to
                        // extract inner commands instead of flattening.
                        let mut body = walk_ast(sib, source);
                        if let Some(ref r) = redir {
                            for seg in &mut body.segments {
                                if seg.redirection.is_none() {
                                    seg.redirection = Some(r.clone());
                                }
                            }
                        }
                        full.append(body, None);
                    }
                    break;
                }
                // The first operator token in heredoc_redirect determines how
                // the body command joins the same-line content.
                let join_op = heredoc_join_operator(child);
                full.append(inner, Some(join_op));
                return full;
            }
        }
    }

    // Second pass: no heredoc piped content.  Handle body normally.
    let mut cursor2 = node.walk();
    for child in node.named_children(&mut cursor2) {
        if matches!(
            child.kind(),
            "file_redirect" | "herestring_redirect" | "heredoc_redirect"
        ) {
            continue;
        }
        if is_leaf_command(child) {
            let end = effective_end(node);
            return WalkResult::single(node.start_byte(), end, redir);
        }
        // Compound body (e.g. for loop with redirect).
        let mut result = walk_ast(child, source);
        if let Some(ref r) = redir {
            for seg in &mut result.segments {
                if seg.redirection.is_none() {
                    seg.redirection = Some(r.clone());
                }
            }
        }
        return result;
    }

    // Fallback: no recognized body.
    let end = effective_end(node);
    WalkResult::single(node.start_byte(), end, redir)
}

/// Walk a `heredoc_redirect` node for commands that appear on the same line
/// as the heredoc marker.
///
/// In tree-sitter-bash, `cat <<EOF | grep foo` places the `| grep foo`
/// pipeline inside the `heredoc_redirect` node rather than as a sibling in an
/// outer pipeline.  Similarly, `cat <<EOF && rm file` places `&& rm file` as
/// an anonymous operator token + named `command` child.
///
/// For parse errors (e.g. `;` in heredoc context), commands may appear as
/// loose `word` nodes.  These are collected into a synthetic segment so the
/// eval layer can flag them.
fn walk_heredoc_redirect(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    let mut loose_words_start: Option<usize> = None;
    let mut loose_words_end: usize = 0;

    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "pipeline" | "list" | "command" | "redirected_statement" => {
                // Flush accumulated loose words as a segment.
                if let Some(start) = loose_words_start.take() {
                    result.append(
                        WalkResult::single(start, loose_words_end, None),
                        Some(Operator::Semi),
                    );
                }
                let op = heredoc_operator_before(node, child);
                result.append(walk_ast(child, source), Some(op));
            }
            "word" => {
                if loose_words_start.is_none() {
                    loose_words_start = Some(child.start_byte());
                }
                loose_words_end = child.end_byte();
            }
            _ => {}
        }
    }

    // Flush any trailing loose words.
    if let Some(start) = loose_words_start {
        result.append(
            WalkResult::single(start, loose_words_end, None),
            Some(Operator::Semi),
        );
    }

    result
}

/// Determine the operator that precedes `child` inside a `heredoc_redirect`.
///
/// Scans the anonymous children of `heredoc_node` for operator tokens (`&&`,
/// `||`, `|`, `|&`) that appear before `child`.  Returns the corresponding
/// [`Operator`], defaulting to [`Operator::Pipe`] when no explicit operator is
/// found (the most common heredoc pattern is piping).
fn heredoc_operator_before(heredoc_node: Node, child: Node) -> Operator {
    let mut cursor = heredoc_node.walk();
    let mut last_op = None;
    for sib in heredoc_node.children(&mut cursor) {
        if sib.start_byte() >= child.start_byte() {
            break;
        }
        if !sib.is_named() {
            match sib.kind() {
                "&&" => last_op = Some(Operator::And),
                "||" => last_op = Some(Operator::Or),
                "|&" => last_op = Some(Operator::PipeErr),
                "|" => last_op = Some(Operator::Pipe),
                _ => {}
            }
        }
    }
    last_op.unwrap_or(Operator::Pipe)
}

/// Determine the operator joining the body command to same-line heredoc content.
///
/// Checks direct children of the `heredoc_redirect` node: anonymous operator
/// tokens (`&&`, `||`, `|&`) and named `pipeline` children (which imply `|`).
/// Returns [`Operator::Pipe`] as default since piping from a heredoc is the
/// most common pattern.
fn heredoc_join_operator(heredoc_node: Node) -> Operator {
    let mut cursor = heredoc_node.walk();
    for child in heredoc_node.children(&mut cursor) {
        if !child.is_named() {
            match child.kind() {
                "&&" => return Operator::And,
                "||" => return Operator::Or,
                "|&" => return Operator::PipeErr,
                _ => {}
            }
        } else {
            match child.kind() {
                "pipeline" => return Operator::Pipe,
                "command" | "list" | "redirected_statement" => break,
                _ => {}
            }
        }
    }
    Operator::Pipe
}

/// `for_statement`, `while_statement`, `until_statement`, `c_style_for_statement`:
/// recurse into child nodes to extract evaluable commands.
///
/// For `while` and `until`, the condition is itself a command (e.g. `true`,
/// `test -f foo`) and must be evaluated alongside the body.  For `for` and
/// `c_style_for`, non-`do_group` children are variable names and word lists,
/// not commands.
fn walk_loop(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "do_group" => result.append(walk_block(child, source), Some(Operator::Semi)),
            _ if node.kind() == "while_statement" || node.kind() == "until_statement" => {
                result.append(walk_ast(child, source), Some(Operator::Semi));
            }
            _ => {}
        }
    }
    result
}

/// `if_statement`: extract commands from the condition, then-body, and any
/// else/elif clauses.
fn walk_if(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "command"
            | "declaration_command"
            | "pipeline"
            | "list"
            | "redirected_statement"
            | "compound_statement"
            | "subshell"
            | "negated_command" => {
                result.append(walk_ast(child, source), Some(Operator::Semi));
            }
            "else_clause" | "elif_clause" => {
                result.append(walk_ast(child, source), Some(Operator::Semi));
            }
            _ => {}
        }
    }
    result
}

/// `case_statement`: recurse into each `case_item`, extracting only the body
/// commands (after the `)` delimiter), not the pattern labels before it.
fn walk_case(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "case_item" {
            result.append(walk_case_item(child, source), Some(Operator::Semi));
        }
    }
    result
}

/// Walk a `case_item` node, skipping pattern labels and extracting only the
/// body commands.
///
/// In tree-sitter-bash, `case_item` children before the `)` token are pattern
/// labels (e.g. `rm`, `*.txt`).  Children after `)` are the body commands to
/// execute when matched.  Only the body commands are evaluable.
fn walk_case_item(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut past_paren = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() && child.kind() == ")" {
            past_paren = true;
            continue;
        }
        if past_paren && child.is_named() {
            result.append(walk_ast(child, source), Some(Operator::Semi));
        }
    }
    result
}

/// Generic block walk: recurse into all named children, joining with
/// [`Operator::Semi`].  Used for `do_group`, `else_clause`, `elif_clause`,
/// `case_item`, `subshell`, and `compound_statement`.
fn walk_block(node: Node, source: &[u8]) -> WalkResult {
    let mut result = WalkResult::empty();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        result.append(walk_ast(child, source), Some(Operator::Semi));
    }
    result
}

/// `negated_command` (`! cmd`): walk the first named child (the negated body).
fn walk_negated(node: Node, source: &[u8]) -> WalkResult {
    let mut cursor = node.walk();
    if let Some(child) = node.named_children(&mut cursor).next() {
        return walk_ast(child, source);
    }
    WalkResult::empty()
}

/// `function_definition`: recurse into the `compound_statement` body.
fn walk_function(node: Node, source: &[u8]) -> WalkResult {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "compound_statement" {
            return walk_block(child, source);
        }
    }
    WalkResult::empty()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// True for node kinds that represent a single evaluable command.
fn is_leaf_command(node: Node) -> bool {
    matches!(
        node.kind(),
        "command" | "declaration_command" | "variable_assignment"
    )
}

/// Return the effective end byte of `node`, excluding any `heredoc_body`
/// descendant.  This trims heredoc body content from segment text so that only
/// the command line (including the `<<DELIM` token) is included.
fn effective_end(node: Node) -> usize {
    let mut end = node.end_byte();
    trim_at_heredoc_body(node, &mut end);
    end
}

fn trim_at_heredoc_body(node: Node, end: &mut usize) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "heredoc_body" {
            *end = (*end).min(child.start_byte());
            return;
        }
        trim_at_heredoc_body(child, end);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a shell command string into a pipeline of segments and a list of
/// extracted substitution contents.
///
/// # Returns
///
/// `(pipeline, substitutions)` where:
///
/// - `pipeline.segments` — one [`ShellSegment`] per evaluable command, with
///   `__SUBST__` placeholders where substitutions were extracted.
/// - `pipeline.operators` — the shell operators (`&&`, `||`, `;`, `|`, `|&`)
///   between consecutive segments.
/// - `substitutions` — the inner command text of each outermost `$()`,
///   backtick, `<()`, or `>()` substitution, in source order.  The eval layer
///   evaluates these recursively.
///
/// # Trivial case
///
/// When the command is a single simple statement with no substitutions and no
/// control flow unwrapping, the original command text is returned as-is in a
/// single segment.  This lets the eval layer's `evaluate_single` fast path
/// work on the exact input text.
pub fn parse_with_substitutions(command: &str) -> (ParsedPipeline, Vec<String>) {
    let tree = parse_tree(command);
    let root = tree.root_node();
    let source = command.as_bytes();

    let mut subst_spans = Vec::new();
    collect_substitutions(root, source, &mut subst_spans);

    let result = walk_ast(root, source);

    // Trivial: one segment spanning the full input, no substitutions, no
    // control flow unwrapping.  When the walker recurses into a for/if/while
    // body the segment byte range will be a sub-range of the input, so this
    // check correctly detects unwrapping.
    let is_trivial = result.segments.len() <= 1
        && subst_spans.is_empty()
        && result
            .segments
            .first()
            .is_none_or(|seg| seg.start == 0 && seg.end >= command.trim_end().len());

    if is_trivial {
        let redir = result
            .segments
            .first()
            .and_then(|seg| seg.redirection.clone())
            .or_else(|| detect_redirections(root, source));
        return (
            ParsedPipeline {
                segments: vec![ShellSegment {
                    command: command.trim().to_string(),
                    redirection: redir,
                }],
                operators: vec![],
            },
            vec![],
        );
    }

    let substitutions: Vec<String> = subst_spans.iter().map(|s| s.inner.clone()).collect();

    let segments: Vec<ShellSegment> = result
        .segments
        .iter()
        .map(|seg| {
            let text = text_replacing_substitutions(command, seg.start, seg.end, &subst_spans);
            ShellSegment {
                command: text.trim().to_string(),
                redirection: seg.redirection.clone(),
            }
        })
        .filter(|s| !s.command.is_empty())
        .collect();

    (
        ParsedPipeline {
            segments,
            operators: result.operators,
        },
        substitutions,
    )
}

/// Check whether `command` contains output redirection that could mutate
/// filesystem state.
///
/// Parses the command with tree-sitter-bash and inspects `file_redirect` nodes.
/// See `check_file_redirect` for the full safe/flagged policy.
pub fn has_output_redirection(command: &str) -> Option<Redirection> {
    let tree = parse_tree(command);
    detect_redirections(tree.root_node(), command.as_bytes())
}

/// Dump the tree-sitter AST and parsed pipeline for a command string.
///
/// Returns a human-readable diagnostic string showing the raw AST tree,
/// the segments and operators produced by the walker, and any extracted
/// substitutions. Used by `--dump-ast` CLI flag.
pub fn dump_ast(command: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    // Raw AST
    let tree = parse_tree(command);
    writeln!(out, "── tree-sitter AST ──").unwrap();
    fn print_node(out: &mut String, node: tree_sitter::Node, source: &[u8], indent: usize) {
        let text = node.utf8_text(source).unwrap_or("???");
        let short: String = text.chars().take(60).collect();
        let tag = if node.is_named() { "named" } else { "anon" };
        writeln!(out, "{}{} [{}] {:?}", "  ".repeat(indent), node.kind(), tag, short).unwrap();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node(out, child, source, indent + 1);
        }
    }
    print_node(&mut out, tree.root_node(), command.as_bytes(), 0);

    // Parsed pipeline
    let (pipeline, substitutions) = parse_with_substitutions(command);
    writeln!(out, "\n── parsed pipeline ──").unwrap();
    for (i, seg) in pipeline.segments.iter().enumerate() {
        let redir = seg.redirection.as_ref().map(|r| format!(" [{}]", r.description)).unwrap_or_default();
        writeln!(out, "  segment {}: {:?}{}", i, seg.command, redir).unwrap();
        if i < pipeline.operators.len() {
            writeln!(out, "  operator: {}", pipeline.operators[i].as_str()).unwrap();
        }
    }
    if !substitutions.is_empty() {
        writeln!(out, "\n── substitutions ──").unwrap();
        for (i, sub) in substitutions.iter().enumerate() {
            writeln!(out, "  {}: {:?}", i, sub).unwrap();
        }
    }

    // Redirection check
    let redir = has_output_redirection(command);
    writeln!(out, "\n── output redirection ──").unwrap();
    match redir {
        Some(r) => writeln!(out, "  {}", r.description).unwrap(),
        None => writeln!(out, "  (none)").unwrap(),
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Compound splitting ---

    #[test]
    fn simple_command() {
        let (p, subs) = parse_with_substitutions("ls -la");
        assert_eq!(p.segments.len(), 1);
        assert_eq!(p.segments[0].command, "ls -la");
        assert!(p.operators.is_empty());
        assert!(subs.is_empty());
    }

    #[test]
    fn pipe() {
        let (p, _) = parse_with_substitutions("ls | grep foo");
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[0].command, "ls");
        assert_eq!(p.segments[1].command, "grep foo");
        assert_eq!(p.operators, vec![Operator::Pipe]);
    }

    #[test]
    fn and_then() {
        let (p, _) = parse_with_substitutions("mkdir foo && cd foo");
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[0].command, "mkdir foo");
        assert_eq!(p.segments[1].command, "cd foo");
        assert_eq!(p.operators, vec![Operator::And]);
    }

    #[test]
    fn or_else() {
        let (p, _) = parse_with_substitutions("test -f x || echo missing");
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.operators, vec![Operator::Or]);
    }

    #[test]
    fn semicolon() {
        let (p, _) = parse_with_substitutions("echo a; echo b");
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[0].command, "echo a");
        assert_eq!(p.segments[1].command, "echo b");
    }

    #[test]
    fn triple_and() {
        let (p, _) = parse_with_substitutions("a && b && c");
        assert_eq!(p.segments.len(), 3);
        assert_eq!(p.operators, vec![Operator::And, Operator::And]);
    }

    #[test]
    fn mixed_operators() {
        let (p, _) = parse_with_substitutions("a && b || c");
        assert_eq!(p.segments.len(), 3);
        assert_eq!(p.operators, vec![Operator::And, Operator::Or]);
    }

    #[test]
    fn quoted_operator_not_split() {
        let (p, subs) = parse_with_substitutions(r#"echo "a && b""#);
        assert_eq!(p.segments.len(), 1);
        assert!(subs.is_empty());
    }

    // --- Substitution extraction ---

    #[test]
    fn dollar_paren_substitution() {
        let (p, subs) = parse_with_substitutions("echo $(date)");
        assert_eq!(subs, vec!["date"]);
        assert_eq!(p.segments[0].command, "echo __SUBST__");
    }

    #[test]
    fn backtick_substitution() {
        let (p, subs) = parse_with_substitutions("echo `date`");
        assert_eq!(subs, vec!["date"]);
        assert_eq!(p.segments[0].command, "echo __SUBST__");
    }

    #[test]
    fn single_quoted_not_substituted() {
        let (_, subs) = parse_with_substitutions("echo '$(date)'");
        assert!(subs.is_empty());
    }

    #[test]
    fn double_quoted_is_substituted() {
        let (_, subs) = parse_with_substitutions(r#"echo "$(date)""#);
        assert_eq!(subs, vec!["date"]);
    }

    #[test]
    fn process_substitution() {
        let (_, subs) = parse_with_substitutions("diff <(ls a) <(ls b)");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], "ls a");
        assert_eq!(subs[1], "ls b");
    }

    // --- Redirection detection ---

    #[test]
    fn redir_simple_gt() {
        assert!(has_output_redirection("echo hi > file").is_some());
    }

    #[test]
    fn redir_append() {
        assert!(has_output_redirection("echo hi >> file").is_some());
    }

    #[test]
    fn redir_ampersand_gt() {
        assert!(has_output_redirection("cmd &> file").is_some());
    }

    #[test]
    fn no_redir_devnull() {
        assert!(has_output_redirection("cmd > /dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_stderr() {
        assert!(has_output_redirection("cmd 2>/dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_append() {
        assert!(has_output_redirection("cmd >> /dev/null").is_none());
    }

    #[test]
    fn no_redir_devnull_ampersand() {
        assert!(has_output_redirection("cmd &>/dev/null").is_none());
    }

    #[test]
    fn no_redir_fd_dup_stderr_to_stdout() {
        assert!(has_output_redirection("cmd 2>&1").is_none());
    }

    #[test]
    fn no_redir_fd_dup_stdout_to_stderr() {
        assert!(has_output_redirection("cmd >&2").is_none());
    }

    #[test]
    fn no_redir_fd_close() {
        assert!(has_output_redirection("cmd >&-").is_none());
    }

    #[test]
    fn redir_custom_fd_target() {
        let r = has_output_redirection("cmd >&3");
        assert!(r.is_some());
        assert!(r.unwrap().description.contains("custom fd target"));
    }

    #[test]
    fn no_redir_quoted() {
        assert!(has_output_redirection(r#"echo ">""#).is_none());
    }

    #[test]
    fn no_redir_process_subst() {
        assert!(has_output_redirection("diff <(ls) >(cat)").is_none());
    }

    #[test]
    fn redir_clobber() {
        let r = has_output_redirection("echo hi >| file.txt");
        assert!(r.is_some(), "expected >| to be flagged as output redirection");
        assert!(r.unwrap().description.contains(">|"));
    }

    #[test]
    fn redir_clobber_devnull() {
        assert!(has_output_redirection("echo hi >| /dev/null").is_none());
    }

    #[test]
    fn redir_read_write_detected() {
        // tree-sitter-bash parses `<>` as `<` + ERROR(`>`). We detect the
        // ERROR child and flag it as output redirection.
        let r = has_output_redirection("cat <> file.txt");
        assert!(r.is_some(), "expected <> to be flagged as output redirection");
        assert!(r.unwrap().description.contains("<>"));
    }

    // --- Control flow ---

    #[test]
    fn for_loop_extracts_body() {
        let (p, _) = parse_with_substitutions("for i in *; do echo \"$i\"; done");
        assert!(p.segments.iter().all(|s| !s.command.starts_with("for")));
        assert!(p.segments.iter().any(|s| s.command.contains("echo")));
    }

    #[test]
    fn if_statement_extracts_body() {
        let (p, _) = parse_with_substitutions("if test -f x; then echo yes; fi");
        assert!(p.segments.iter().all(|s| !s.command.starts_with("if")));
        assert!(p.segments.iter().any(|s| s.command.contains("test")));
        assert!(p.segments.iter().any(|s| s.command.contains("echo")));
    }

    #[test]
    fn while_loop_extracts_body() {
        let (p, _) = parse_with_substitutions("while true; do sleep 1; done");
        assert!(p.segments.iter().all(|s| !s.command.starts_with("while")));
        assert!(p.segments.iter().any(|s| s.command.contains("true")));
        assert!(p.segments.iter().any(|s| s.command.contains("sleep")));
    }

    #[test]
    fn case_pattern_not_treated_as_command() {
        let (p, _) =
            parse_with_substitutions(r#"case $x in rm) echo hi ;; kubectl) echo bye ;; esac"#);
        let commands: Vec<&str> = p.segments.iter().map(|s| s.command.as_str()).collect();
        // Pattern labels (rm, kubectl) must NOT appear as segments.
        // Only the body commands (echo hi, echo bye) should.
        assert!(
            !p.segments.iter().any(|s| s.command.trim() == "rm"),
            "case pattern 'rm' leaked as segment: {commands:?}",
        );
        assert!(
            !p.segments.iter().any(|s| s.command.trim() == "kubectl"),
            "case pattern 'kubectl' leaked as segment: {commands:?}",
        );
        assert!(
            p.segments.iter().any(|s| s.command.contains("echo hi")),
            "expected 'echo hi' body: {commands:?}",
        );
        assert!(
            p.segments.iter().any(|s| s.command.contains("echo bye")),
            "expected 'echo bye' body: {commands:?}",
        );
    }

    #[test]
    fn compound_heredoc_pipe_unwraps_body() {
        // When a compound command (while/for/if) is the body of a
        // redirected_statement with a heredoc pipe, the body must be
        // recursively walked so inner commands are extracted — not
        // flattened as "while ..." text.
        let cmd = "while true; do shred /dev/sda; done <<EOF | cat\nstuff\nEOF";
        let (p, _) = parse_with_substitutions(cmd);
        let commands: Vec<&str> = p.segments.iter().map(|s| s.command.as_str()).collect();
        // The while-loop body should be unwrapped to "shred /dev/sda",
        // not left as "while true; do shred /dev/sda; done".
        assert!(
            !p.segments.iter().any(|s| s.command.starts_with("while")),
            "while-loop was not unwrapped in heredoc pipe path: {commands:?}",
        );
        assert!(
            p.segments.iter().any(|s| s.command.contains("shred")),
            "expected 'shred' to be extracted from loop body: {commands:?}",
        );
        assert!(
            p.segments.iter().any(|s| s.command.trim() == "cat"),
            "expected piped 'cat' segment: {commands:?}",
        );
    }
}
