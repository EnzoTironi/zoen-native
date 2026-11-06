//! Trusted service startup only; never accepted from conversations or models.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub agent: String,
    pub relay_url: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Response {
    External,
    Prepared { code: String },
}
