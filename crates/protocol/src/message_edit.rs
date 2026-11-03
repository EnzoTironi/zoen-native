//! Shared reviewed edit intent; each client validates against its canonical source.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageEdit {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub message_id: String,
    pub expected_revision: String,
    pub text: String,
}
