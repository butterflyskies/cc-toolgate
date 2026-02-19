//! Types produced by the shell parser and consumed by the eval layer.

/// Shell operator separating consecutive pipeline segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operator {
    /// `&&` — run next only if previous succeeded
    And,
    /// `||` — run next only if previous failed
    Or,
    /// `;` — run next unconditionally
    Semi,
    /// `|` — pipe stdout
    Pipe,
    /// `|&` — pipe stdout+stderr
    PipeErr,
}

impl Operator {
    /// The operator's shell syntax.
    pub fn as_str(&self) -> &'static str {
        match self {
            Operator::And => "&&",
            Operator::Or => "||",
            Operator::Semi => ";",
            Operator::Pipe => "|",
            Operator::PipeErr => "|&",
        }
    }
}

/// A single evaluable command within a compound pipeline.
///
/// `command` contains the text that will be passed to `base_command()`,
/// `tokenize()`, and `has_output_redirection()` in the eval layer.
/// Any `$()`, backtick, or process substitution spans have been replaced
/// with `__SUBST__` placeholders.
#[derive(Debug, Clone)]
pub struct ShellSegment {
    /// Command text, with substitution spans replaced by `__SUBST__`.
    pub command: String,

    /// Output redirection detected on a wrapping construct.
    ///
    /// When the parser extracts commands from inside a control flow block
    /// that has output redirection (e.g. `for ... done > file`), the
    /// redirect is not present in the segment's `command` text.  This field
    /// carries the redirection so the eval layer can escalate the decision.
    ///
    /// For segments where the redirect IS in the command text (e.g.
    /// `echo hi > file`), this field may also be set but is redundant —
    /// `CommandContext::from_command` will independently detect it.
    pub redirection: Option<Redirection>,
}

/// Describes an output redirection that may mutate filesystem state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    /// Human-readable description, e.g. `"output redirection (>)"`.
    pub description: String,
}

/// A fully decomposed compound command: segments interleaved with operators.
///
/// For a simple command like `ls -la`, there is one segment and no operators.
/// For `a && b | c`, there are three segments and two operators (`&&`, `|`).
#[derive(Debug, Clone)]
pub struct ParsedPipeline {
    pub segments: Vec<ShellSegment>,
    pub operators: Vec<Operator>,
}
