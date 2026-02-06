use crate::eval::RuleMatch;
use std::io::Write;

/// Append a decision record to ~/.local/share/cc-toolgate/decisions.log.
/// Best-effort: failures are silently ignored (logging must never block the hook).
pub fn log_decision(command: &str, result: &RuleMatch) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let log_dir = std::path::Path::new(&home).join(".local/share/cc-toolgate");
    let _ = std::fs::create_dir_all(&log_dir);

    let log_path = log_dir.join("decisions.log");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    else {
        return;
    };

    // Compact single-line reason for the log (replace newlines with "; ")
    let reason_oneline = result.reason.replace('\n', "; ");
    let cmd_truncated: String = command.chars().take(200).collect();
    let ts = timestamp_now();

    let _ = writeln!(
        file,
        "{ts}\t{decision}\t{cmd}\t{reason}",
        decision = result.decision.as_str(),
        cmd = cmd_truncated,
        reason = reason_oneline,
    );
}

/// Simple UTC timestamp without external deps.
fn timestamp_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    let (year, month, day) = epoch_days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    // Civil calendar from days algorithm (Howard Hinnant)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
