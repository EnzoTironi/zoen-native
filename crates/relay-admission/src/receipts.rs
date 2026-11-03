//! Exact unsigned receipt policy shared by the native relay and its SQL boundary.
//! The Session authenticates the device and the canonical log supplies current
//! membership. This module neither grants authority nor persists a second roster.
use crate::portable::BoundaryError;
use serde::{Deserialize, Serialize};

pub const PAGE_ROWS: usize = 128;

fn invalid(reason: &str) -> BoundaryError {
    BoundaryError::internal("invalid_receipt_context", reason)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watermark {
    delivered: u64,
    read: u64,
}

/// External JSON values are always exact decimal strings, never JSON numbers.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WatermarkDto {
    pub delivered: String,
    pub read: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Raised {
    pub watermark: Watermark,
    /// A first zero mark still creates a row, preserving complete native replay.
    pub changed: bool,
}

impl Watermark {
    /// Validate an existing authoritative stored pair. Never silently repair
    /// corrupt stored state, truncate an unsigned value, or wrap through i64.
    /// # Errors
    /// Refuses a stored read watermark above the delivered watermark.
    pub fn stored(delivered: u64, read: u64) -> Result<Self, BoundaryError> {
        if read > delivered {
            return Err(invalid("stored read exceeds delivered"));
        }
        Ok(Self { delivered, read })
    }
    #[must_use]
    pub fn pair(self) -> (u64, u64) {
        (self.delivered, self.read)
    }
    #[must_use]
    pub fn dto(self) -> WatermarkDto {
        WatermarkDto {
            delivered: self.delivered.to_string(),
            read: self.read.to_string(),
        }
    }
    /// # Errors
    /// Refuses noncanonical unsigned decimal watermarks or read values above delivery.
    pub fn from_dto(value: &WatermarkDto) -> Result<Self, BoundaryError> {
        Self::stored(
            crate::portable::parse_u64(&value.delivered)?,
            crate::portable::parse_u64(&value.read)?,
        )
    }
}

/// Native in-memory pending/inflight and authoritative host rows use this same
/// exact unsigned max operation. `read` also proves delivery. No clamp mode.
#[must_use]
pub fn raise(previous: Option<Watermark>, delivered: u64, read: u64) -> Raised {
    let old = previous.unwrap_or_default();
    let watermark = Watermark {
        delivered: old.delivered.max(delivered).max(read),
        read: old.read.max(read),
    };
    Raised {
        watermark,
        changed: previous != Some(watermark),
    }
}

/// # Errors
/// Refuses invalid identity keys, reversed continuation bounds or out-of-range page limits.
pub fn validate_page(after: &str, ceiling: &str, limit: u32) -> Result<(), BoundaryError> {
    if (!after.is_empty() && !crate::identity_key(after))
        || !crate::identity_key(ceiling)
        || after > ceiling
        || limit == 0
        || limit as usize > PAGE_ROWS
    {
        return Err(invalid("invalid receipt continuation"));
    }
    Ok(())
}

/// Shared live routing over a roster read from the actual current Space authority.
/// Caller-owned authentication is required. This intentionally corrects the old
/// native optimization that included authors who had since been removed.
/// # Errors
/// Refuses malformed authoritative identities or more than 16 author hints.
pub fn recipients(
    from: &str,
    current: &[String],
    receipt: bool,
    authors: &[String],
) -> Result<Option<Vec<String>>, BoundaryError> {
    if !crate::identity_key(from)
        || current.iter().any(|id| !crate::identity_key(id))
        || authors.len() > 16
        || authors.iter().any(|id| !crate::identity_key(id))
    {
        return Err(invalid("invalid authoritative receipt audience"));
    }
    let members: std::collections::BTreeSet<_> = current.iter().cloned().collect();
    if !members.contains(from) {
        return Ok(None);
    }
    let to: std::collections::BTreeSet<String> = if receipt && members.len() > 16 {
        std::iter::once(from.to_string())
            .chain(authors.iter().filter(|id| members.contains(*id)).cloned())
            .collect()
    } else {
        members
            .into_iter()
            .filter(|id| receipt || id != from)
            .collect()
    };
    Ok(Some(to.into_iter().collect()))
}
