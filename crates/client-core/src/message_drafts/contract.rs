//! The shared private composer wire contract. Storage and publication remain in Engine.
use super::MessageDraftEdit;
use crate::{replies::external::SharedReplyDraft, TimelineEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageDraftDto {
    pub revision: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub edit: Option<MessageDraftEdit>,
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub reply_quote: Option<crate::ReplySelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub shared_reply: Option<SharedReplyDraft>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessageDraftSendResult {
    pub entry: TimelineEntry,
    pub draft: MessageDraftDto,
}
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct DraftScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub thread_root_id: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct DraftContent {
    pub text: String,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub edit: Option<MessageDraftEdit>,
    pub reply_to: Option<String>,
    #[serde(default)]
    pub reply_quote: Option<crate::ReplySelection>,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub shared_reply: Option<SharedReplyDraft>,
}
