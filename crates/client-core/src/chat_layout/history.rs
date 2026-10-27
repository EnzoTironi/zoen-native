//! Reconstruct revisions and exact receipts from authenticated local events, never a side ledger.
use super::{
    invalid, operation_id, revision, ChatLayoutField, ChatLayoutReceipt, ChatLayoutResult,
    ChatLayoutScope, ChatLayoutState,
};
use crate::engine::{Engine, R};
use std::collections::HashMap;
use types::{ChatPrefs, EventBody, SpaceKind};

#[derive(Default)]
pub(super) struct Current {
    pub prefs: ChatPrefs,
    pin_revision: Option<String>,
    archive_revision: Option<String>,
}
impl Current {
    pub fn revision(&self, field: ChatLayoutField) -> &str {
        match field {
            ChatLayoutField::Pinned => self.pin_revision.as_deref(),
            ChatLayoutField::Archived => self.archive_revision.as_deref(),
        }
        .unwrap_or("0")
    }
    fn dto(&self) -> ChatLayoutState {
        ChatLayoutState {
            pinned: self.prefs.pinned,
            archived: self.prefs.archived,
            pin_revision: self.revision(ChatLayoutField::Pinned).to_owned(),
            archive_revision: self.revision(ChatLayoutField::Archived).to_owned(),
        }
    }
}
pub(super) struct Recorded {
    space: String,
    device: String,
    pub previous_revision: String,
    pub receipt: ChatLayoutReceipt,
}
pub(super) struct History {
    states: HashMap<String, Current>,
    operations: HashMap<String, Recorded>,
}
impl History {
    pub fn read(engine: &Engine) -> R<Self> {
        let mut history = Self {
            states: HashMap::new(),
            operations: HashMap::new(),
        };
        let Some(personal) = engine.personal_space() else {
            return Ok(history);
        };
        if engine.broken_spaces.contains_key(&personal) {
            return Err(invalid());
        }
        let Some(log) = engine.logs.get(&personal) else {
            return Err(invalid());
        };
        // The Personal log is local and sequenced by the existing signed append, not optimistic outbox.
        if log.events().first().is_none_or(|e| {
            e.author != engine.me.as_deref().unwrap_or("")
                || !matches!(
                    e.body,
                    EventBody::SpaceCreated {
                        kind: SpaceKind::Personal,
                        ..
                    }
                )
        }) {
            return Err(invalid());
        }
        for event in log.events() {
            let EventBody::ChatPrefsSet {
                space,
                prefs,
                operation,
            } = &event.body
            else {
                continue;
            };
            if Some(event.author.as_str()) != engine.me.as_deref() {
                continue;
            }
            let current = history.states.entry(space.clone()).or_default();
            let event_revision = event_log::content_hash_of(event);
            if let Some(operation) = operation {
                operation_id(&operation.id)?;
                revision(&operation.previous_revision)?;
                let field = ChatLayoutField::from(operation.field);
                let mut expected = current.prefs.clone();
                field.set(&mut expected, field.value(prefs));
                if &expected != prefs
                    || field.value(&current.prefs) == field.value(prefs)
                    || current.revision(field) != operation.previous_revision
                    || history.operations.contains_key(&operation.id)
                {
                    return Err(invalid());
                }
                if let Some(original) = &operation.undo_of {
                    let previous = history.operations.get(original).ok_or_else(invalid)?;
                    if previous.space != *space
                        || previous.receipt.field != field
                        || previous.receipt.undo_of.is_some()
                        || previous.receipt.before != field.value(prefs)
                        || previous.receipt.revision != operation.previous_revision
                        || Some(previous.device.as_str()) != event.device.as_deref()
                    {
                        return Err(invalid());
                    }
                }
                history.operations.insert(
                    operation.id.clone(),
                    Recorded {
                        space: space.clone(),
                        device: event.device.clone().ok_or_else(invalid)?,
                        previous_revision: operation.previous_revision.clone(),
                        receipt: ChatLayoutReceipt {
                            operation_id: operation.id.clone(),
                            field,
                            before: field.value(&current.prefs),
                            value: field.value(prefs),
                            revision: event_revision.clone(),
                            undo_of: operation.undo_of.clone(),
                            undo_available: false,
                        },
                    },
                );
            }
            if current.prefs.pinned != prefs.pinned {
                current.pin_revision = Some(event_revision.clone());
            }
            if current.prefs.archived != prefs.archived {
                current.archive_revision = Some(event_revision);
            }
            current.prefs = prefs.clone();
        }
        Ok(history)
    }
    pub fn current(&self, space: &str) -> Current {
        self.states
            .get(space)
            .map_or_else(Current::default, |value| Current {
                prefs: value.prefs.clone(),
                pin_revision: value.pin_revision.clone(),
                archive_revision: value.archive_revision.clone(),
            })
    }
    pub fn operation(&self, scope: &ChatLayoutScope, id: &str) -> R<Option<&Recorded>> {
        match self.operations.get(id) {
            Some(record)
                if record.space != scope.space_id || record.device != scope.expected_device_id =>
            {
                Err(invalid())
            }
            value => Ok(value),
        }
    }
    pub fn result(&self, scope: &ChatLayoutScope, receipt: Option<&Recorded>) -> ChatLayoutResult {
        let current = self.current(&scope.space_id);
        let receipt = receipt.map(|record| {
            let mut receipt = record.receipt.clone();
            receipt.undo_available =
                receipt.undo_of.is_none() && current.revision(receipt.field) == receipt.revision;
            receipt
        });
        ChatLayoutResult {
            current: current.dto(),
            receipt,
        }
    }
}
