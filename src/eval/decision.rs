#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Decision::Allow => "ALLOW",
            Decision::Ask => "ASK",
            Decision::Deny => "DENY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub decision: Decision,
    pub reason: String,
}
