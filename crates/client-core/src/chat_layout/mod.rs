//! Field-scoped local CAS and exact receipts in the existing signed Personal log.
//! This is not a relay acknowledgement or a distributed preference transaction.
mod history;
use crate::{
    engine::{Engine, R},
    CoreError,
};
use history::History;
use serde::{Deserialize, Serialize};
use types::{ChatPrefsField, ChatPrefsOperation, EventBody};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum ChatLayoutField {
    Pinned,
    Archived,
}
impl ChatLayoutField {
    fn persisted(self) -> ChatPrefsField {
        match self {
            Self::Pinned => ChatPrefsField::Pinned,
            Self::Archived => ChatPrefsField::Archived,
        }
    }
    fn value(self, prefs: &types::ChatPrefs) -> bool {
        match self {
            Self::Pinned => prefs.pinned,
            Self::Archived => prefs.archived,
        }
    }
    fn set(self, prefs: &mut types::ChatPrefs, value: bool) {
        match self {
            Self::Pinned => prefs.pinned = value,
            Self::Archived => prefs.archived = value,
        }
    }
}
impl From<ChatPrefsField> for ChatLayoutField {
    fn from(value: ChatPrefsField) -> Self {
        match value {
            ChatPrefsField::Pinned => Self::Pinned,
            ChatPrefsField::Archived => Self::Archived,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ChatLayoutScope {
    pub space_id: String,
    pub expected_identity_id: String,
    pub expected_device_id: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum ChatLayoutInput {
    Read {
        scope: ChatLayoutScope,
    },
    Change {
        scope: ChatLayoutScope,
        operation_id: String,
        field: ChatLayoutField,
        value: bool,
        observed_revision: String,
    },
    Reconcile {
        scope: ChatLayoutScope,
        operation_id: String,
    },
    Undo {
        scope: ChatLayoutScope,
        operation_id: String,
        original_operation_id: String,
        observed_revision: String,
    },
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ChatLayoutState {
    pub pinned: bool,
    pub archived: bool,
    pub pin_revision: String,
    pub archive_revision: String,
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ChatLayoutReceipt {
    pub operation_id: String,
    pub field: ChatLayoutField,
    pub before: bool,
    pub value: bool,
    pub revision: String,
    pub undo_of: Option<String>,
    pub undo_available: bool,
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ChatLayoutResult {
    pub current: ChatLayoutState,
    pub receipt: Option<ChatLayoutReceipt>,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t("operação de conversa inválida", "invalid chat operation"),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "a conversa mudou; reveja a ação",
            "chat preferences changed; review the action",
        ),
    }
}
fn operation_id(id: &str) -> R<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(invalid());
    }
    Ok(())
}
fn revision(id: &str) -> R<()> {
    if id != "0"
        && (id.len() != 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(invalid());
    }
    Ok(())
}
impl Engine {
    pub(crate) fn chat_layout(&mut self, input: &ChatLayoutInput) -> R<ChatLayoutResult> {
        let scope = match input {
            ChatLayoutInput::Read { scope }
            | ChatLayoutInput::Change { scope, .. }
            | ChatLayoutInput::Reconcile { scope, .. }
            | ChatLayoutInput::Undo { scope, .. } => scope,
        };
        self.chat_mute_owner(
            &scope.space_id,
            &scope.expected_identity_id,
            &scope.expected_device_id,
        )?;
        let history = History::read(self)?;
        let state = history.current(&scope.space_id);
        match input {
            ChatLayoutInput::Read { .. } => Ok(history.result(scope, None)),
            ChatLayoutInput::Reconcile {
                operation_id: id, ..
            } => {
                operation_id(id)?;
                let receipt = history.operation(scope, id)?;
                Ok(history.result(scope, receipt))
            }
            ChatLayoutInput::Change {
                operation_id: id,
                field,
                value,
                observed_revision,
                ..
            } => {
                operation_id(id)?;
                revision(observed_revision)?;
                if let Some(record) = history.operation(scope, id)? {
                    if record.receipt.field != *field
                        || record.receipt.value != *value
                        || record.previous_revision != *observed_revision
                        || record.receipt.undo_of.is_some()
                    {
                        return Err(invalid());
                    }
                    return Ok(history.result(scope, Some(record)));
                }
                if state.revision(*field) != observed_revision {
                    return Err(stale());
                }
                if field.value(&state.prefs) == *value {
                    return Ok(history.result(scope, None));
                }
                self.append_chat_layout(
                    scope,
                    state.prefs.clone(),
                    *value,
                    ChatPrefsOperation {
                        id: id.clone(),
                        field: field.persisted(),
                        previous_revision: observed_revision.clone(),
                        undo_of: None,
                    },
                )
            }
            ChatLayoutInput::Undo {
                operation_id: id,
                original_operation_id,
                observed_revision,
                ..
            } => {
                operation_id(id)?;
                operation_id(original_operation_id)?;
                revision(observed_revision)?;
                let original = history
                    .operation(scope, original_operation_id)?
                    .ok_or_else(stale)?;
                if original.receipt.undo_of.is_some()
                    || original.receipt.revision != *observed_revision
                {
                    return Err(stale());
                }
                if let Some(record) = history.operation(scope, id)? {
                    if record.receipt.undo_of.as_deref() != Some(original_operation_id)
                        || record.previous_revision != *observed_revision
                    {
                        return Err(invalid());
                    }
                    return Ok(history.result(scope, Some(record)));
                }
                if state.revision(original.receipt.field) != observed_revision {
                    return Err(stale());
                }
                self.append_chat_layout(
                    scope,
                    state.prefs.clone(),
                    original.receipt.before,
                    ChatPrefsOperation {
                        id: id.clone(),
                        field: original.receipt.field.persisted(),
                        previous_revision: observed_revision.clone(),
                        undo_of: Some(original_operation_id.clone()),
                    },
                )
            }
        }
    }
    fn append_chat_layout(
        &mut self,
        scope: &ChatLayoutScope,
        mut prefs: types::ChatPrefs,
        value: bool,
        operation: ChatPrefsOperation,
    ) -> R<ChatLayoutResult> {
        ChatLayoutField::from(operation.field).set(&mut prefs, value);
        let id = operation.id.clone();
        let personal = self.ensure_personal_space()?;
        if self.net.synced.contains(&personal) {
            return Err(invalid());
        }
        self.append(
            &personal,
            &scope.expected_identity_id,
            EventBody::ChatPrefsSet {
                space: scope.space_id.clone(),
                prefs,
                operation: Some(operation),
            },
        )?;
        let history = History::read(self)?;
        let receipt = history.operation(scope, &id)?.ok_or_else(invalid)?;
        Ok(history.result(scope, Some(receipt)))
    }
}
