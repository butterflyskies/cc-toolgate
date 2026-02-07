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

/// Base disposition for a command or subcommand (what it does with no flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseDisposition {
    Allow,
    Ask,
    Deny,
}

impl BaseDisposition {
    pub fn to_decision(self) -> Decision {
        match self {
            BaseDisposition::Allow => Decision::Allow,
            BaseDisposition::Ask => Decision::Ask,
            BaseDisposition::Deny => Decision::Deny,
        }
    }
}

/// Per-flag disposition for known flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagDisposition {
    /// Flag is safe — does not change the base disposition.
    Safe,
    /// Flag escalates the decision (e.g. ALLOW → ASK).
    Escalate,
    /// Flag is dangerous — force DENY.
    Deny,
}
