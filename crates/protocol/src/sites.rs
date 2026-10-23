//! Explicit publication of a selected private file version. The publication
//! receipt is a delivery effect; it never replaces the encrypted source Item.

use serde::{Deserialize, Serialize};

pub const SCHEMA: u8 = 1;
pub const MAX_HTML_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema: u8,
    pub audience: String,
    pub management_origin: String,
    pub public_origin: String,
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub max_html_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SourceVersion {
    pub space_id: String,
    pub item_id: String,
    pub version: u32,
    pub sha256: String,
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub byte_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Publish { source: SourceVersion },
    Unpublish,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub schema: u8,
    /// The public destination shown during explicit consent, not just a Site ID.
    pub public_origin: String,
    pub site_id: String,
    pub request_id: String,
    /// None only creates a new site. Every subsequent action compares the head,
    /// including rollback (a new Publish) and unpublish.
    pub expected_head: Option<String>,
    pub change: Change,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SignedInspection {
    pub owner_id: String,
    pub device_id: String,
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub at_ms: i64,
    pub site_id: String,
    pub public_origin: String,
    /// Continue a receipt chain from this head; None starts at the current head.
    pub before: Option<String>,
    pub signature: String,
}

impl SignedInspection {
    pub fn signing_bytes(&self, audience: &str) -> Vec<u8> {
        serde_json::to_vec(&(
            "zoen-site-inspection.v1",
            audience,
            &self.owner_id,
            &self.device_id,
            self.at_ms,
            &self.site_id,
            &self.public_origin,
            &self.before,
        ))
        .expect("site inspection serialization")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    pub site_id: String,
    pub head: Option<String>,
    pub active: bool,
    /// Newest first, at most 100; historical order follows previous_head links.
    pub receipts: Vec<Receipt>,
    pub next_before: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SignedMutation {
    pub owner_id: String,
    pub device_id: String,
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub at_ms: i64,
    pub mutation: Mutation,
    pub signature: String,
}

impl SignedMutation {
    pub fn signing_bytes(&self, audience: &str) -> Vec<u8> {
        serde_json::to_vec(&(
            "zoen-site-publication.v1",
            audience,
            &self.owner_id,
            &self.device_id,
            self.at_ms,
            &self.mutation,
        ))
        .expect("site mutation serialization")
    }

    /// A renewed signature or another currently linked device can recover the
    /// same immutable receipt. Neither changes the logical publication intent.
    pub fn intent_digest(&self, audience: &str) -> String {
        let bytes = serde_json::to_vec(&(
            "zoen-site-intent.v1",
            audience,
            &self.owner_id,
            &self.mutation,
        ))
        .expect("site intent serialization");
        event_log::sha256_hex(&bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u8,
    pub site_id: String,
    pub request_id: String,
    pub owner_id: String,
    pub head: String,
    pub previous_head: Option<String>,
    pub source: Option<SourceVersion>,
    pub active: bool,
}

impl Receipt {
    pub fn for_mutation(signed: &SignedMutation, audience: &str) -> Self {
        let source = match &signed.mutation.change {
            Change::Publish { source } => Some(source.clone()),
            Change::Unpublish => None,
        };
        Self {
            schema: SCHEMA,
            site_id: signed.mutation.site_id.clone(),
            request_id: signed.mutation.request_id.clone(),
            owner_id: signed.owner_id.clone(),
            head: signed.intent_digest(audience),
            previous_head: signed.mutation.expected_head.clone(),
            active: source.is_some(),
            source,
        }
    }
}
