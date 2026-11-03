//! Native transport limits and exact decimal SQL boundary values.
use serde::Serialize;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = 90 * 1024;

#[derive(Debug, Serialize)]
pub struct BoundaryError {
    pub code: &'static str,
    pub reason: String,
    pub permanent: bool,
}

impl BoundaryError {
    /// A trusted context/storage/encoding failure says nothing permanent about
    /// the signed client envelope. A host may reload facts and retry it.
    pub(crate) fn internal(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            permanent: false,
        }
    }
}

/// # Errors
/// Refuses empty, noncanonical, nondecimal or overflowing unsigned values.
pub fn parse_u64(value: &str) -> Result<u64, BoundaryError> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err(BoundaryError::internal(
            "invalid_integer",
            "invalid UInt64 decimal string",
        ));
    }
    value
        .parse()
        .map_err(|_| BoundaryError::internal("invalid_integer", "UInt64 overflow"))
}
