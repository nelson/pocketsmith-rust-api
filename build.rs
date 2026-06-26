//! Build script: capture the short git commit and the build date so the
//! `version` subcommand can report exactly which revision a binary came from.
//!
//! Emits two compile-time env vars consumed via `env!` in `src/main.rs`:
//!   GIT_COMMIT  — `git rev-parse --short HEAD` (or "unknown" outside a repo)
//!   BUILD_DATE  — UTC build date as YYYY-MM-DD

use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_COMMIT={commit}");

    let date = build_date();
    println!("cargo:rustc-env=BUILD_DATE={date}");

    // Rebuild when HEAD moves so the embedded commit stays accurate.
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(reference) = head.strip_prefix("ref:").map(str::trim) {
            println!("cargo:rerun-if-changed=.git/{reference}");
        }
    }
    // Allow reproducible builds to pin the date.
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

/// UTC build date (YYYY-MM-DD). Honours `SOURCE_DATE_EPOCH` for reproducible
/// builds; otherwise uses the current wall-clock time. Computed without any
/// external crate via a plain civil-from-days conversion.
fn build_date() -> String {
    let secs: i64 = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });

    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert a count of days since the Unix epoch to a (year, month, day) in the
/// proleptic Gregorian calendar. Algorithm from Howard Hinnant's `chrono`
/// date utilities (public domain).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}
