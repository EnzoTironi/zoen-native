//! The metrics clock: UTC wall time. Debug builds (journeys) may shift it by whole days with
//! `ZOEN_METRICS_CLOCK_FILE`, a file holding a day offset the test rewrites to simulate a
//! month in seconds. Release builds ignore it.

use std::time::{SystemTime, UNIX_EPOCH};

const DAY_MS: i64 = 86_400_000;

pub fn now_ms() -> i64 {
    let real = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    real + offset_days() * DAY_MS
}

#[cfg(debug_assertions)]
fn offset_days() -> i64 {
    std::env::var_os("ZOEN_METRICS_CLOCK_FILE")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(not(debug_assertions))]
fn offset_days() -> i64 {
    0
}

/// Days since 1970-01-01 (UTC) of a millisecond timestamp.
pub fn day_of(ms: i64) -> i32 {
    ms.div_euclid(DAY_MS) as i32
}

pub fn today() -> i32 {
    day_of(now_ms())
}
