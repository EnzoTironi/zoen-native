//! Admission is identical for local writes, optimistic projection and authenticated peer replay.
use super::{valid_id, Annotation, EntryBody, Event, EventBody, MessagePinChange};
use crate::engine::{MessageMarks, State};
use std::collections::HashMap;
use types::{Role, SpaceKind};
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MessagePinMarks {
    pub pinned: bool,
    pub revision: String,
    pub sequence: u64,
    pub receipts: HashMap<String, String>,
}
fn valid(change: &MessagePinChange) -> bool {
    !change.operation_id.is_empty()
        && change.operation_id.len() <= 128
        && change
            .operation_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        && valid_id(&change.expected_message_revision)
        && valid_id(&change.expected_pin_revision)
}
impl State {
    pub(crate) fn pin_writer(&self, space: &str, author: &str) -> bool {
        self.spaces.get(space).is_some_and(|s| {
            self.writer(space, author)
                && s.members.iter().any(|(id, role)| {
                    id == author
                        && (matches!(s.kind, SpaceKind::Direct | SpaceKind::Personal)
                            || matches!(role, Role::Owner | Role::Admin))
                })
        })
    }
    pub(crate) fn message_pin_valid(&self, event: &Event) -> bool {
        let EventBody::MessageAnnotated {
            target,
            note: Annotation::Pin { change },
        } = &event.body
        else {
            return true;
        };
        self.pin_change_valid(&event.space, &event.author, target, change)
    }
    pub(crate) fn pin_change_valid(
        &self,
        space: &str,
        author: &str,
        target: &str,
        change: &MessagePinChange,
    ) -> bool {
        if !valid(change) || !self.pin_writer(space, author) {
            return false;
        }
        self.spaces
            .get(space)
            .and_then(|s| s.entries.iter().find(|e| e.id() == target))
            .is_some_and(|e| {
                let EntryBody::Message { marks, .. } = &e.body else {
                    return false;
                };
                !e.hash.is_empty()
                    && e.seq > 0
                    && !marks.deleted
                    && marks.text_revision.as_deref().unwrap_or(e.id())
                        == change.expected_message_revision
                    && marks.pin.as_ref().map_or(e.id(), |p| p.revision.as_str())
                        == change.expected_pin_revision
                    && marks
                        .pin
                        .as_ref()
                        .is_none_or(|p| !p.receipts.contains_key(&change.operation_id))
                    && marks.pin.as_ref().is_some_and(|p| p.pinned) != change.pinned
            })
    }
}
pub(crate) fn record_pin(marks: &mut MessageMarks, event: &Event, change: &MessagePinChange) {
    let revision = event_log::content_hash_of(event);
    let pin = marks.pin.get_or_insert_with(|| {
        Box::new(MessagePinMarks {
            pinned: false,
            revision: String::new(),
            sequence: 0,
            receipts: HashMap::new(),
        })
    });
    pin.pinned = change.pinned;
    pin.revision.clone_from(&revision);
    pin.sequence = event.seq;
    pin.receipts.insert(change.operation_id.clone(), revision);
}
