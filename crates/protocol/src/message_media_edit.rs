//! A replacement targets one canonical message, never every reference to its original file.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageMediaEdit {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub message_id: String,
    pub edit: types::MediaChange,
}
