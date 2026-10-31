//! Read-only message metadata from signed events and authenticated receipt evidence.
use crate::{Delivery, Persona};
use serde::{Deserialize, Serialize};
mod projection;
mod receipts;
#[cfg(test)]
mod tests;

/// Exact account/device/message scope; labels and stale `is_me` never authorize metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageInfoScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub message_id: String,
}
/// Why participant details are available or intentionally absent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum MessageReadDetails {
    Available,
    NotSender,
    NotSmallConversation,
    Unconfirmed,
}
/// Current authorized recipient's actual watermark and distinct clock evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageReadParticipant {
    pub person: Persona,
    pub delivered: bool,
    pub read: bool,
    /// Participant-device reported read time. None is never replaced by local time.
    pub read_reported_at_ms: Option<i64>,
    /// This device's first admitted observation of that interval, not read time.
    pub read_observed_at_ms: Option<i64>,
}
/// Sent/edit dates are author-signed event clock claims, not relay arrival time.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageInfo {
    pub scope: MessageInfoScope,
    pub author: Persona,
    #[serde(default)]
    pub forwarded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub forwarded_author: Option<String>,
    pub sent_at_ms: i64,
    pub edited: bool,
    pub edited_at_ms: Option<i64>,
    pub delivery: Delivery,
    pub read_details: MessageReadDetails,
    pub participants: Vec<MessageReadParticipant>,
}
