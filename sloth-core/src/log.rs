//! Minimal file logger (config: "enable_log"). Writes one line per entry, in
//! English, to a file named `log` next to the running executable. Disabled by
//! default; a no-op (never panics, NFR-4) if disabled or the file can't be
//! opened.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

/// Open (or create/append) `log` next to the current executable if `enabled`.
/// Call once at startup. Subsequent calls are ignored (OnceLock).
pub fn init(enabled: bool) {
    let file = if enabled { open_log_file() } else { None };
    LOG_FILE.set(Mutex::new(file)).ok();
}

fn open_log_file() -> Option<std::fs::File> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("log"))
        .ok()
}

/// Append one line to the log file (English only). No-op if logging is
/// disabled, the file failed to open, or `init` was never called.
pub fn log(msg: impl AsRef<str>) {
    let Some(lock) = LOG_FILE.get() else { return };
    let Ok(mut guard) = lock.lock() else { return };
    let Some(file) = guard.as_mut() else { return };
    let _ = writeln!(file, "[{}] {}", timestamp(), msg.as_ref());
}

/// Print a line to stdout and append the same line (English) to the log
/// file, so console output and the log file never drift apart.
#[macro_export]
macro_rules! notify {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{msg}");
        $crate::log::log(&msg);
    }};
}

/// Print a line to stderr and append the same line (English) to the log
/// file, so console output and the log file never drift apart.
#[macro_export]
macro_rules! notify_err {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("{msg}");
        $crate::log::log(&msg);
    }};
}

/// UTC timestamp "YYYY-MM-DD HH:MM:SS" without pulling in a date dependency.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

/// Howard Hinnant's days-from-epoch -> civil date algorithm (proleptic Gregorian, UTC).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
