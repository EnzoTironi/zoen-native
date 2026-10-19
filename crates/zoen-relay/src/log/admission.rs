//! Native relay re-export of the same portable rules. No storage backend owns a second policy.

pub(crate) use relay_admission::identity_key;
pub use relay_admission::{admit, Effect, Facts, MAX_MEMBERS};
