//! Decision logging to `~/.local/share/cc-toolgate/decisions.log`.
//!
//! Initializes a file logger on first call and writes one line per evaluated
//! command with the decision, truncated command text, and reason.

use crate::eval::RuleMatch;
use log::info;
use simplelog::{Config, LevelFilter, WriteLogger};
use std::sync::Once;

/// Ensures the logger is initialized exactly once per process.
static INIT: Once = Once::new();

/// Initialize the file logger. Best-effort: failures are silently ignored.
pub fn init() {
    INIT.call_once(|| {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let log_dir = std::path::Path::new(&home).join(".local/share/cc-toolgate");
        let _ = std::fs::create_dir_all(&log_dir);

        let log_path = log_dir.join("decisions.log");
        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        else {
            return;
        };

        let _ = WriteLogger::init(LevelFilter::Info, Config::default(), file);
    });
}

/// Log a decision record.
/// Format: `{decision}\t{command_truncated}\t{reason_oneline}`
/// Timestamp is provided by simplelog.
pub fn log_decision(command: &str, result: &RuleMatch) {
    let reason_oneline = result.reason.replace('\n', "; ");
    let cmd_truncated: String = command.chars().take(200).collect();

    info!(
        "{decision}\t{cmd}\t{reason}",
        decision = result.decision.as_str(),
        cmd = cmd_truncated,
        reason = reason_oneline,
    );
}
