//! Explicit current-state disclosure into the existing conversation audience.
use crate::{AppDoc, Grant, Seen};
use serde::{Deserialize, Serialize};

/// Public preimage for the existing binary-App ID factory. This binds the App
/// namespace to its conversation; it is neither a grant nor file-byte metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppItemOrigin {
    pub author: String,
    pub device: String,
    pub space: String,
    pub mutation: String,
}

/// References authenticated outer log positions; this is not proof that a new
/// reader decrypted the historical plaintext at those positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppShareRef {
    pub event: String,
    pub seq: u64,
    pub hash: String,
    pub author: String,
    pub device: Option<String>,
    pub at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppCurrentShare {
    pub schema: u32,
    pub item: String,
    pub target: String,
    pub mutation: String,
    pub version: u32,
    pub content: AppDoc,
    pub creation: AppShareRef,
    pub head: AppShareRef,
    pub anchor: AppShareRef,
    pub cut: Seen,
    pub epoch: Option<u64>,
    /// The existing normal Item/app:ID/Act grant, issued by the sharing writer.
    /// Snapshot rejection rejects this grant too; no separate orphan event.
    pub grant: Grant,
}
