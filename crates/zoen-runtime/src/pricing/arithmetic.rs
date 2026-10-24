//! Exact configured tariff calculation shared by chat and image profiles.
use crate::RuntimeError;

pub(super) fn million_token_cost(terms: &[(u64, u64)]) -> Result<i64, RuntimeError> {
    let numerator = terms.iter().try_fold(0u128, |sum, (tokens, rate)| {
        u128::from(*tokens)
            .checked_mul(u128::from(*rate))
            .and_then(|value| sum.checked_add(value))
            .ok_or(RuntimeError::InvalidProfile)
    })?;
    let rounded = numerator
        .checked_add(999_999)
        .ok_or(RuntimeError::InvalidProfile)?
        / 1_000_000;
    i64::try_from(rounded).map_err(|_| RuntimeError::InvalidProfile)
}
