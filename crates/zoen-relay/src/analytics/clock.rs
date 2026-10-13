//! The metrics clock: UTC wall time. Debug journeys may set `ZOEN_METRICS_CLOCK_FILE`
//! to a whole-day offset or `unix_ms:<timestamp>` for a fixed simulated time. Release
//! builds ignore it.

use std::time::{SystemTime, UNIX_EPOCH};

const DAY_MS: i64 = 86_400_000;

pub fn now_ms() -> i64 {
    let real = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    #[cfg(debug_assertions)]
    if let Some(text) =
        std::env::var_os("ZOEN_METRICS_CLOCK_FILE").and_then(|p| std::fs::read_to_string(p).ok())
    {
        return simulated_time(real, &text).unwrap_or(real);
    }
    real
}

#[cfg(debug_assertions)]
fn simulated_time(real: i64, text: &str) -> Option<i64> {
    let text = text.trim();
    if let Some(ms) = text.strip_prefix("unix_ms:") {
        return ms.trim().parse().ok();
    }
    text.parse::<i64>()
        .ok()?
        .checked_mul(DAY_MS)?
        .checked_add(real)
}

/// Days since 1970-01-01 (UTC) of a millisecond timestamp.
pub fn day_of(ms: i64) -> i32 {
    ms.div_euclid(DAY_MS) as i32
}

pub fn today() -> i32 {
    day_of(now_ms())
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn fixed_time_keeps_the_same_day_across_real_utc_midnight() {
        let midnight = 20_000 * DAY_MS;
        let fixed = midnight - DAY_MS / 2;
        let text = format!("unix_ms:{fixed}");
        for real in [midnight - 1, midnight, midnight + 1] {
            assert_eq!(simulated_time(real, &text), Some(fixed));
            assert_eq!(day_of(simulated_time(real, &text).unwrap()), 19_999);
        }
    }

    #[test]
    fn fixed_time_advances_only_when_the_fixture_advances() {
        let base = 20_000 * DAY_MS + DAY_MS / 2;
        for day in [0, 1, 7, 8] {
            let fixed = base + day * DAY_MS;
            assert_eq!(simulated_time(0, &format!("unix_ms:{fixed}")), Some(fixed));
            assert_eq!(day_of(fixed), 20_000 + day as i32);
        }
    }

    #[test]
    fn legacy_day_offsets_keep_their_wall_clock_behavior() {
        let real = 20_000 * DAY_MS + 123;
        assert_eq!(simulated_time(real, " 0\n"), Some(real));
        assert_eq!(simulated_time(real, "1"), Some(real + DAY_MS));
        assert_eq!(simulated_time(real, "-1"), Some(real - DAY_MS));
    }

    #[test]
    fn invalid_or_overflowing_overrides_fall_back_to_wall_time() {
        for text in ["", "bad", "unix_ms:", "unix_ms:bad", "9223372036854775807"] {
            assert_eq!(simulated_time(123, text), None);
        }
        assert_eq!(simulated_time(i64::MAX, "1"), None);
    }
}
