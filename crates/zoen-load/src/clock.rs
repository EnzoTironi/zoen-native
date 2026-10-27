//! Timestamp representation used by signed load events.
use anyhow::Context;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn unix_ms(now: SystemTime) -> anyhow::Result<i64> {
    let elapsed = now
        .duration_since(UNIX_EPOCH)
        .context("load clock precedes Unix epoch")?;
    i64::try_from(elapsed.as_millis()).context("load timestamp exceeds signed event range")
}

#[cfg(test)]
mod tests {
    use super::unix_ms;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn invalid_clock_must_not_masquerade_as_the_unix_epoch() {
        let before = UNIX_EPOCH.checked_sub(Duration::from_millis(1)).unwrap();
        assert!(
            unix_ms(before).is_err(),
            "clock error cannot become a valid epoch timestamp"
        );
    }

    #[test]
    fn timestamp_outside_signed_wire_range_cannot_wrap_negative() {
        let millis = u64::try_from(i64::MAX).unwrap() + 1;
        let future = UNIX_EPOCH
            .checked_add(Duration::from_millis(millis))
            .unwrap();
        assert!(
            unix_ms(future).is_err(),
            "positive time outside the signed range must be refused"
        );
    }

    #[test]
    fn epoch_and_signed_integer_boundary_have_exact_milliseconds() {
        assert_eq!(unix_ms(UNIX_EPOCH).unwrap(), 0);
        let millis = u64::try_from(i64::MAX).unwrap();
        let boundary = UNIX_EPOCH
            .checked_add(Duration::from_millis(millis))
            .unwrap();
        assert_eq!(unix_ms(boundary).unwrap(), i64::MAX);
    }
}
