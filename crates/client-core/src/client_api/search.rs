//! Flat application request payloads for the existing and paginated FTS queries.
//! Internally tagged Command newtypes retain the exact public JSON shape.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct SearchInput {
    pub query: String,
    pub kinds: Vec<String>,
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPageInput {
    pub query: String,
    pub kinds: Vec<String>,
    pub cursor: Option<String>,
    pub limit: u32,
    pub expected_identity_id: String,
    pub expected_device_id: String,
}
