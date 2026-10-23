//! Independent single-host Images checkpoints. The owned local directory MUST
//! be excluded from SQL/FDB snapshots and restore. This is not a financial
//! ledger, a network filesystem protocol, or authority to invoke a provider.
mod file;
mod state;
pub use file::{AnchorReader, AnchorWriter, GenerationGuard};
pub use state::{Checkpoint, Deployment, Prepared, Recovery};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorError {
    Invalid,
    Busy,
    Unavailable,
    Unreconciled,
}
impl std::fmt::Display for AnchorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Invalid => "invalid managed Images anchor",
            Self::Busy => "managed Images generation is still active",
            Self::Unavailable => "managed Images anchor unavailable",
            Self::Unreconciled => "managed Images stores require reconciliation",
        })
    }
}
impl std::error::Error for AnchorError {}

#[cfg(test)]
mod tests;
