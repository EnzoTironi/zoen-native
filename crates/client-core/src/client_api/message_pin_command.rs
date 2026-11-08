//! Shared pins use the existing JSON bridge and the original Engine transaction.
use super::{encode, page_size, Client, CoreError};
use crate::message_pins::{MessagePinEdit, MessagePinScope};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(super) enum MessagePinCommand {
    #[serde(rename = "message_pin_review")]
    Review {
        scope: MessagePinScope,
        message_id: String,
    },
    #[serde(rename = "message_pins")]
    List {
        scope: MessagePinScope,
        cursor: Option<String>,
        #[serde(default = "page_size")]
        limit: u32,
    },
    #[serde(rename = "set_message_pin")]
    Set { input: MessagePinEdit },
}
impl MessagePinCommand {
    pub(super) fn execute(self, client: &Client) -> Result<String, CoreError> {
        let mut engine = client.lock();
        match self {
            Self::Review { scope, message_id } => {
                encode(&engine.message_pin_review(&scope, &message_id)?)
            }
            Self::List {
                scope,
                cursor,
                limit,
            } => encode(&engine.message_pins(&scope, cursor.as_deref(), limit)?),
            Self::Set { input } => encode(&engine.set_message_pin(&input)?),
        }
    }
}
