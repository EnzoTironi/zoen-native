//! Shared relay admission rules, used by the native relay and portable Worker boundary.

pub mod auth;
pub mod blobs;
pub mod closed;
pub mod directory;
pub mod gcra;
pub mod invite;
pub mod mls_projection;
pub mod portable;
mod rules;
pub mod sync;
pub mod transfer;

pub use rules::{admit, identity_key, Effect, Facts, MAX_MEMBERS};

/// Same canonical invite lookup key used by every native and portable backend.
pub fn code_hash(code: &str) -> String {
    roda_log::sha256_hex(code.trim().to_uppercase().as_bytes())
}

/// Why an envelope was refused. `permanent` = retrying the same envelope can't succeed.
#[derive(Debug)]
pub struct Reject {
    pub reason: String,
    pub permanent: bool,
}

impl Reject {
    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            permanent: true,
        }
    }

    pub fn retry(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            permanent: false,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            reason: "log store unavailable".into(),
            permanent: false,
        }
    }
}
