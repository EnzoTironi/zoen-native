//! Admission and cryptographic policy used by the native relay.

pub mod auth;
pub mod blobs;
pub mod directory;
pub mod gcra;
pub mod invite;
pub mod managed_images;
pub mod mls_projection;
pub mod portable;
pub mod receipts;
mod rules;
pub mod sites;
pub mod sync;
pub mod transfer;

pub use rules::{admit, identity_key, Effect, Facts, MAX_MEMBERS};

/// Canonical invite lookup key used by the native relay.
pub fn code_hash(code: &str) -> String {
    event_log::sha256_hex(code.trim().to_uppercase().as_bytes())
}

/// Why an envelope was refused. `permanent` = retrying the same envelope can't succeed.
#[derive(Debug)]
pub struct Reject {
    pub reason: String,
    pub permanent: bool,
    /// Candidate only; the paired log owner must durably confirm it.
    pub managed_refusal: Option<managed_images::PublishRefusal>,
}

impl Reject {
    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            permanent: true,
            managed_refusal: None,
        }
    }

    pub fn retry(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            permanent: false,
            managed_refusal: None,
        }
    }

    pub fn managed_candidate(refusal: managed_images::PublishRefusal) -> Self {
        Self {
            reason: refusal.message().into(),
            permanent: false,
            managed_refusal: Some(refusal),
        }
    }

    pub fn unavailable() -> Self {
        Self::retry("log store unavailable")
    }
}
