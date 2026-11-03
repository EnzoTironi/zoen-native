//! Checked conversions for scheduler inputs and exact count-based reports.
use anyhow::{bail, Context};
use num_traits::ToPrimitive;

const MAX_EXACT_INTEGER: u64 = (1_u64 << 53) - 1;

pub(super) fn exact(value: u64) -> anyhow::Result<f64> {
    if value > MAX_EXACT_INTEGER {
        bail!("load integer exceeds the exact floating-point range");
    }
    value.to_f64().context("load integer cannot be represented")
}

/// Preserve the scheduler's floor-to-microsecond/message convention, refusing
/// negative, non-finite or overflowing values instead of saturating a cast.
pub(super) fn integer(value: f64) -> anyhow::Result<u64> {
    if !value.is_finite() || value < 0.0 {
        bail!("load schedule value must be finite and nonnegative");
    }
    value
        .to_u64()
        .context("load schedule value is outside the unsigned integer range")
}

#[cfg(test)]
mod tests {
    use super::{exact, integer, MAX_EXACT_INTEGER};

    #[test]
    fn exact_integer_limit_is_not_silently_rounded() {
        assert_eq!(exact(MAX_EXACT_INTEGER).unwrap(), 9_007_199_254_740_991.0);
        assert!(exact(MAX_EXACT_INTEGER + 1).is_err());
        assert_eq!(exact(0).unwrap(), 0.0);
    }

    #[test]
    fn scheduler_conversion_refuses_saturation_and_preserves_floor() {
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            -0.5,
            18_446_744_073_709_551_616.0,
        ] {
            assert!(integer(invalid).is_err());
        }
        assert_eq!(integer(0.0).unwrap(), 0);
        assert_eq!(integer(999.999).unwrap(), 999);
        assert_eq!(integer(1000.0).unwrap(), 1000);
    }
}
