/// Represents a shell operator that separates pipeline segments.
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

/// A single segment of a compound command (between operators).
#[derive(Debug, Clone)]
pub struct ShellSegment {
    /// The command text with substitutions replaced by `__SUBST__` placeholders.
    pub command: String,
    /// Extracted substitution contents (from `$()`, backticks, `<()`, `>()`).
    pub substitutions: Vec<String>,
    /// Whether output redirection was detected.
    pub redirection: Option<Redirection>,
}

/// Output redirection descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    pub description: String,
}

/// A fully parsed pipeline: segments separated by operators.
#[derive(Debug, Clone)]
pub struct ParsedPipeline {
    pub segments: Vec<ShellSegment>,
    pub operators: Vec<Operator>,
}
