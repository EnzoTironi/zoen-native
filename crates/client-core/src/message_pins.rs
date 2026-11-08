//! Shared pins are signed annotations in the original conversation, never a private side store.
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used
)]
pub(crate) mod projection;
mod reading;
mod writing;
use crate::{
    engine::{Engine, Entry, EntryBody, R},
    CoreError, TimelineEntry,
};
use serde::{Deserialize, Serialize};
use types::{Annotation, Event, EventBody, MessagePinChange};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessagePinScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub expected_relay_url: String,
    pub space_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessagePinEdit {
    pub scope: MessagePinScope,
    pub message_id: String,
    pub change: MessagePinChange,
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessagePinReview {
    pub scope: MessagePinScope,
    pub entry: TimelineEntry,
    pub pin_revision: String,
    pub pinned: bool,
    pub can_pin: bool,
    pub pending: bool,
}
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessagePinResult {
    pub review: MessagePinReview,
    pub applied_revision: String,
}
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct MessagePinPage {
    pub revision: String,
    pub entries: Vec<TimelineEntry>,
    pub total: u32,
    pub next: Option<String>,
}
fn changed() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "A mensagem ou a fixação mudou. Reveja antes de continuar.",
            "The message or pin changed. Review it before continuing.",
        ),
    }
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl Engine {
    fn pin_owner(&self, scope: &MessagePinScope) -> R<()> {
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        if self
            .account()
            .is_none_or(|a| !a.registered || a.relay_url != scope.expected_relay_url)
            || !self.is_member(&scope.space_id, &scope.expected_identity_id)
            || self.space_state(&scope.space_id)?.integrity_error.is_some()
        {
            return Err(changed());
        }
        Ok(())
    }
    fn pin_source(&self, scope: &MessagePinScope, message: &str) -> R<&Entry> {
        self.pin_owner(scope)?;
        if !valid_id(message) {
            return Err(changed());
        }
        let entry = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .find(|e| e.id() == message)
            .ok_or_else(changed)?;
        if !self.visible_message(entry)
            || !matches!(&entry.body, EntryBody::Message {marks,..} if !marks.deleted)
        {
            return Err(changed());
        }
        Ok(entry)
    }
    pub(crate) fn message_pin_review(
        &self,
        scope: &MessagePinScope,
        message: &str,
    ) -> R<MessagePinReview> {
        let entry = self.pin_source(scope, message)?;
        let EntryBody::Message { marks, .. } = &entry.body else {
            return Err(changed());
        };
        Ok(MessagePinReview {
            scope: scope.clone(),
            entry: self.entry_dto(entry),
            pin_revision: marks
                .pin
                .as_ref()
                .map_or_else(|| entry.id().to_string(), |p| p.revision.clone()),
            pinned: marks.pin.as_ref().is_some_and(|p| p.pinned),
            pending: marks.pin.as_ref().is_some_and(|p| p.sequence == 0),
            can_pin: !entry.hash.is_empty()
                && entry.seq > 0
                && self
                    .state
                    .pin_writer(&scope.space_id, &scope.expected_identity_id),
        })
    }
}

#[cfg(test)]
mod tests;
