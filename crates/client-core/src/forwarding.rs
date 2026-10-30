//! A private reviewed intent over the existing signed forward and outbox.
//! Reading/recovery never sends; source and destination composers are untouched.
use crate::{
    engine::{Engine, EntryBody, R},
    i18n::t,
    Client, CoreError, TimelineEntry,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod batch;
mod storage;
pub use batch::{ForwardBatchReview, ForwardBatchScope, ForwardBatchSource};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ForwardSource {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub source_space: String,
    pub message_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ForwardScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub source_space: String,
    pub message_id: String,
    pub target_space: String,
}
impl ForwardScope {
    pub fn source(&self) -> ForwardSource {
        ForwardSource {
            expected_identity_id: self.expected_identity_id.clone(),
            expected_device_id: self.expected_device_id.clone(),
            source_space: self.source_space.clone(),
            message_id: self.message_id.clone(),
        }
    }
}
impl ForwardSource {
    fn target(&self, target_space: String) -> ForwardScope {
        ForwardScope {
            expected_identity_id: self.expected_identity_id.clone(),
            expected_device_id: self.expected_device_id.clone(),
            source_space: self.source_space.clone(),
            message_id: self.message_id.clone(),
            target_space,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ForwardReview {
    pub target_space: String,
    /// Durable local acceptance marker, available even after destination access was lost.
    pub accepted_message_id: Option<String>,
    pub operation_id: String,
    pub source_revision: String,
    /// The canonical source projection reviewed before confirmation, never a draft.
    pub message: TimelineEntry,
    /// Local signed acceptance, not a remote delivery acknowledgement.
    pub accepted: Option<TimelineEntry>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    target_space: String,
    operation_id: String,
    source_revision: String,
    message: TimelineEntry,
    accepted_message: Option<String>,
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "o encaminhamento mudou; revise novamente",
            "the forward changed; review it again",
        ),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t("encaminhamento inválido", "invalid forward"),
    }
}
fn storage_error() -> CoreError {
    CoreError::Storage {
        message: t(
            "não foi possível recuperar o encaminhamento",
            "could not recover the forward",
        ),
    }
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

impl Engine {
    fn forward_source_access(&self, source: &ForwardSource) -> R<()> {
        if !valid_id(&source.source_space) || !valid_id(&source.message_id) {
            return Err(invalid());
        }
        self.native_surface_owner(&source.expected_identity_id, &source.expected_device_id)?;
        if !self.is_member(&source.source_space, &source.expected_identity_id) {
            return Err(CoreError::Forbidden {
                reason: t("o acesso à conversa mudou", "conversation access changed"),
            });
        }
        Ok(())
    }
    fn forward_access(&self, scope: &ForwardScope) -> R<()> {
        self.forward_source_access(&scope.source())?;
        if !valid_id(&scope.target_space)
            || !self.is_member(&scope.target_space, &scope.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: t("o acesso ao destino mudou", "destination access changed"),
            });
        }
        Ok(())
    }
    fn forward_source(&self, scope: &ForwardScope) -> R<(TimelineEntry, String)> {
        let source = self
            .space_state(&scope.source_space)?
            .entries
            .iter()
            .find(|e| e.id() == scope.message_id)
            .ok_or_else(stale)?;
        if !self.visible_message(source)
            || !matches!(&source.body, EntryBody::Message { marks, .. } if !marks.deleted)
        {
            return Err(stale());
        }
        let message = self.entry_dto(source);
        // Current Item version participates via its projected card. Voice includes
        // its signed content hash/metadata. Delivery/reactions/read counters do not.
        let bytes = serde_json::to_vec(&(
            &scope.source_space,
            &message.id,
            &message.author.id,
            message.at_ms,
            &message.kind,
            &message.voice,
            &message.media,
            message.forwarded,
        ))
        .map_err(|_| invalid())?;
        Ok((message, hex::encode(Sha256::digest(bytes))))
    }
    fn forward_result(&self, scope: &ForwardScope, record: Record) -> R<ForwardReview> {
        let accepted = record.accepted_message.as_ref().and_then(|id| {
            // A local receipt may be acknowledged without exposing a destination
            // entry after membership revocation. It never grants read authority.
            self.is_member(&record.target_space, &scope.expected_identity_id)
                .then(|| {
                    self.space_state(&record.target_space)
                        .ok()?
                        .entries
                        .iter()
                        .find(|e| e.id() == id)
                        .map(|e| self.entry_dto(e))
                })
                .flatten()
        });
        Ok(ForwardReview {
            target_space: record.target_space,
            accepted_message_id: record.accepted_message,
            operation_id: record.operation_id,
            source_revision: record.source_revision,
            message: record.message,
            accepted,
        })
    }
    pub(crate) fn forward_status(&self, source: &ForwardSource) -> R<Option<ForwardReview>> {
        self.forward_source_access(source)?;
        self.load_forward(source)?
            .map(|r| self.forward_result(&source.target(r.target_space.clone()), r))
            .transpose()
    }
    pub(crate) fn forward_review(&mut self, scope: &ForwardScope) -> R<ForwardReview> {
        self.forward_access(scope)?;
        if let Some(record) = self.load_forward(&scope.source())? {
            return self.forward_result(scope, record);
        }
        if !self
            .state
            .writer(&scope.target_space, &scope.expected_identity_id)
        {
            return Err(stale());
        }
        let (message, source_revision) = self.forward_source(scope)?;
        let record = Record {
            target_space: scope.target_space.clone(),
            operation_id: types::new_id("forward")?,
            source_revision,
            message,
            accepted_message: None,
        };
        self.persist_forward(scope, &record)?;
        self.forward_result(scope, record)
    }
    pub(crate) fn confirm_forward(
        &mut self,
        scope: &ForwardScope,
        operation_id: &str,
        revision: &str,
    ) -> R<ForwardReview> {
        self.forward_access(scope)?;
        let mut record = self.load_forward(&scope.source())?.ok_or_else(stale)?;
        if record.target_space != scope.target_space
            || record.operation_id != operation_id
            || record.source_revision != revision
        {
            return Err(stale());
        }
        // Unknown ACK must resolve the already accepted signed entry before
        // consulting a source that may since have been edited or deleted.
        if record.accepted_message.is_some() {
            return self.forward_result(scope, record);
        }
        if !self
            .state
            .writer(&scope.target_space, &scope.expected_identity_id)
            || self.forward_source(scope)?.1 != revision
        {
            return Err(stale());
        }
        // The Client's single Engine lock covers the check and this transaction.
        self.atomic_agent_action(|engine| {
            let entry =
                engine.forward(&scope.source_space, &scope.message_id, &scope.target_space)?;
            record.accepted_message = Some(entry.id);
            engine.persist_forward(scope, &record)?;
            engine.forward_result(scope, record)
        })
    }
    pub(crate) fn dismiss_forward(
        &mut self,
        scope: &ForwardScope,
        operation_id: &str,
        accepted_message_id: Option<&str>,
    ) -> R<()> {
        self.forward_source_access(&scope.source())?;
        let Some(record) = self.load_forward(&scope.source())? else {
            return Ok(());
        };
        if record.target_space != scope.target_space
            || record.operation_id != operation_id
            || record.accepted_message.as_deref() != accepted_message_id
        {
            // A Cancel issued while pending cannot erase an acceptance that won
            // the lock first. Only a caller that observed its exact output may ACK.
            return Err(stale());
        }
        self.remove_forward(scope)
    }
}

impl Client {
    pub fn forward_review(&self, scope: ForwardScope) -> Result<ForwardReview, CoreError> {
        self.lock().forward_review(&scope)
    }
    pub fn forward_status(
        &self,
        source: ForwardSource,
    ) -> Result<Option<ForwardReview>, CoreError> {
        self.lock().forward_status(&source)
    }
    pub fn confirm_forward(
        &self,
        scope: ForwardScope,
        operation_id: String,
        source_revision: String,
    ) -> Result<ForwardReview, CoreError> {
        self.lock()
            .confirm_forward(&scope, &operation_id, &source_revision)
    }
    pub fn dismiss_forward(
        &self,
        scope: ForwardScope,
        operation_id: String,
        accepted_message_id: Option<String>,
    ) -> Result<(), CoreError> {
        self.lock()
            .dismiss_forward(&scope, &operation_id, accepted_message_id.as_deref())
    }
}
