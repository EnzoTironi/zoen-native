//! Native relay re-export of the same portable rules. No storage backend owns a second policy.

pub(crate) use roda_relay_admission::identity_key;
pub use roda_relay_admission::{admit, Effect, Facts, MAX_MEMBERS};
