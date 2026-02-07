use crate::commands::CommandSpec;
use crate::eval::{CommandContext, Decision, RuleMatch};

/// A data-driven command spec for flat allow/ask commands.
///
/// For allow commands: returns Allow unless output redirection is detected (→ Ask).
/// For ask commands: always returns Ask.
pub struct SimpleCommandSpec {
    names: &'static [&'static str],
    decision: Decision,
}

impl CommandSpec for SimpleCommandSpec {
    fn names(&self) -> &[&str] {
        self.names
    }

    fn evaluate(&self, ctx: &CommandContext) -> RuleMatch {
        match self.decision {
            Decision::Allow => {
                // Check for --version on any allowed command
                if ctx.words.len() <= 3 && ctx.has_flag("--version") {
                    return RuleMatch {
                        decision: Decision::Allow,
                        reason: format!("{} --version", ctx.base_command),
                    };
                }
                // Redirection escalates ALLOW → ASK
                if let Some(ref r) = ctx.redirection {
                    return RuleMatch {
                        decision: Decision::Ask,
                        reason: format!("{} with {}", ctx.base_command, r.description),
                    };
                }
                RuleMatch {
                    decision: Decision::Allow,
                    reason: format!("allowed: {}", ctx.base_command),
                }
            }
            Decision::Ask => RuleMatch {
                decision: Decision::Ask,
                reason: format!("{} requires confirmation", ctx.base_command),
            },
            Decision::Deny => RuleMatch {
                decision: Decision::Deny,
                reason: format!("blocked command: {}", ctx.base_command),
            },
        }
    }
}

macro_rules! allow_spec {
    ($name:ident, $($cmd:expr),+) => {
        static $name: SimpleCommandSpec = SimpleCommandSpec {
            names: &[$($cmd),+],
            decision: Decision::Allow,
        };
    };
}

macro_rules! ask_spec {
    ($name:ident, $($cmd:expr),+) => {
        static $name: SimpleCommandSpec = SimpleCommandSpec {
            names: &[$($cmd),+],
            decision: Decision::Ask,
        };
    };
}

// ── Allow commands ──
allow_spec!(LS, "ls");
allow_spec!(TREE, "tree");
allow_spec!(WHICH, "which");
allow_spec!(CD, "cd", "chdir");
allow_spec!(PWD, "pwd");
allow_spec!(CAT, "cat");
allow_spec!(HEAD, "head");
allow_spec!(TAIL, "tail");
allow_spec!(LESS, "less", "more");
allow_spec!(ECHO, "echo");
allow_spec!(PRINTF, "printf");
allow_spec!(GREP, "grep");
allow_spec!(SORT, "sort");
allow_spec!(UNIQ, "uniq");
allow_spec!(DIFF, "diff");
allow_spec!(COMM, "comm");
allow_spec!(TR, "tr");
allow_spec!(CUT, "cut");
allow_spec!(REV, "rev");
allow_spec!(WC, "wc");
allow_spec!(COLUMN, "column");
allow_spec!(PASTE, "paste");
allow_spec!(EXPAND, "expand", "unexpand");
allow_spec!(FOLD, "fold");
allow_spec!(FMT, "fmt");
allow_spec!(NL, "nl");
allow_spec!(STAT, "stat");
allow_spec!(FILE, "file");
allow_spec!(DIRNAME, "dirname");
allow_spec!(BASENAME, "basename");
allow_spec!(REALPATH, "realpath");
allow_spec!(READLINK, "readlink");
allow_spec!(UNAME, "uname");
allow_spec!(HOSTNAME, "hostname");
allow_spec!(ID, "id");
allow_spec!(WHOAMI, "whoami");
allow_spec!(GROUPS, "groups");
allow_spec!(NPROC, "nproc");
allow_spec!(UPTIME, "uptime");
allow_spec!(ARCH, "arch");
allow_spec!(DATE, "date");
allow_spec!(FREE, "free");
allow_spec!(DF, "df");
allow_spec!(DU, "du");
allow_spec!(LSBLK, "lsblk");
allow_spec!(ENV, "env");
allow_spec!(PRINTENV, "printenv");
allow_spec!(LOCALE, "locale");
allow_spec!(TEST, "test", "[");
allow_spec!(TRUE_FALSE, "true", "false");
allow_spec!(TYPE, "type");
allow_spec!(COMMAND, "command");
allow_spec!(HASH, "hash");
allow_spec!(EXPORT, "export");
allow_spec!(UNSET, "unset");
allow_spec!(SET, "set");
allow_spec!(SOURCE, "source", ".");
allow_spec!(SLEEP, "sleep");
allow_spec!(SEQ, "seq");
allow_spec!(YES, "yes");
allow_spec!(PS, "ps");
allow_spec!(TOP, "top", "htop");
allow_spec!(PGREP, "pgrep");
allow_spec!(FIND, "find");
allow_spec!(XARGS, "xargs");
allow_spec!(CLEAR, "clear");
allow_spec!(TPUT, "tput");
allow_spec!(RESET, "reset");
// Rust CLI tools
allow_spec!(EZA, "eza");
allow_spec!(BAT, "bat");
allow_spec!(FD, "fd");
allow_spec!(RG, "rg");
allow_spec!(SD, "sd");
allow_spec!(DUST, "dust");
allow_spec!(PROCS, "procs");
allow_spec!(TOKEI, "tokei");
allow_spec!(DELTA, "delta");
allow_spec!(ZOXIDE, "zoxide");
allow_spec!(HYPERFINE, "hyperfine");
allow_spec!(JUST, "just");

pub static ALLOW_SPECS: &[&dyn CommandSpec] = &[
    &LS, &TREE, &WHICH, &CD, &PWD, &CAT, &HEAD, &TAIL, &LESS, &ECHO, &PRINTF, &GREP, &SORT,
    &UNIQ, &DIFF, &COMM, &TR, &CUT, &REV, &WC, &COLUMN, &PASTE, &EXPAND, &FOLD, &FMT, &NL,
    &STAT, &FILE, &DIRNAME, &BASENAME, &REALPATH, &READLINK, &UNAME, &HOSTNAME, &ID, &WHOAMI,
    &GROUPS, &NPROC, &UPTIME, &ARCH, &DATE, &FREE, &DF, &DU, &LSBLK, &ENV, &PRINTENV, &LOCALE,
    &TEST, &TRUE_FALSE, &TYPE, &COMMAND, &HASH, &EXPORT, &UNSET, &SET, &SOURCE, &SLEEP, &SEQ,
    &YES, &PS, &TOP, &PGREP, &FIND, &XARGS, &CLEAR, &TPUT, &RESET, &EZA, &BAT, &FD, &RG, &SD,
    &DUST, &PROCS, &TOKEI, &DELTA, &ZOXIDE, &HYPERFINE, &JUST,
];

// ── Ask commands ──
ask_spec!(RM, "rm");
ask_spec!(RMDIR, "rmdir");
ask_spec!(SUDO, "sudo");
ask_spec!(SU, "su");
ask_spec!(DOAS, "doas");
ask_spec!(PKEXEC, "pkexec");
ask_spec!(MKDIR, "mkdir");
ask_spec!(TOUCH, "touch");
ask_spec!(MV, "mv");
ask_spec!(CP, "cp");
ask_spec!(LN, "ln");
ask_spec!(CHMOD, "chmod");
ask_spec!(CHOWN, "chown");
ask_spec!(CHGRP, "chgrp");
ask_spec!(TEE, "tee");
ask_spec!(CURL, "curl");
ask_spec!(WGET, "wget");
ask_spec!(PIP, "pip", "pip3");
ask_spec!(NPM, "npm");
ask_spec!(NPX, "npx");
ask_spec!(YARN, "yarn");
ask_spec!(PNPM, "pnpm");
ask_spec!(PYTHON, "python", "python3");
ask_spec!(NODE, "node");
ask_spec!(RUBY, "ruby");
ask_spec!(PERL, "perl");
ask_spec!(MAKE, "make");
ask_spec!(CMAKE, "cmake");
ask_spec!(NINJA, "ninja");

pub static ASK_SPECS: &[&dyn CommandSpec] = &[
    &RM, &RMDIR, &SUDO, &SU, &DOAS, &PKEXEC, &MKDIR, &TOUCH, &MV, &CP, &LN, &CHMOD, &CHOWN,
    &CHGRP, &TEE, &CURL, &WGET, &PIP, &NPM, &NPX, &YARN, &PNPM, &PYTHON, &NODE, &RUBY, &PERL,
    &MAKE, &CMAKE, &NINJA,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(cmd: &str) -> Decision {
        let ctx = CommandContext::from_command(cmd);
        // Check allow specs
        for spec in ALLOW_SPECS {
            if spec.names().contains(&ctx.base_command.as_str()) {
                return spec.evaluate(&ctx).decision;
            }
        }
        // Check ask specs
        for spec in ASK_SPECS {
            if spec.names().contains(&ctx.base_command.as_str()) {
                return spec.evaluate(&ctx).decision;
            }
        }
        Decision::Ask // fallthrough
    }

    #[test]
    fn allow_ls() {
        assert_eq!(eval("ls -la"), Decision::Allow);
    }

    #[test]
    fn allow_with_redir() {
        assert_eq!(eval("ls > file.txt"), Decision::Ask);
    }

    #[test]
    fn ask_rm() {
        assert_eq!(eval("rm -rf /tmp"), Decision::Ask);
    }

    #[test]
    fn ask_sudo() {
        assert_eq!(eval("sudo apt install vim"), Decision::Ask);
    }
}
