use super::{MessageInfo, MessageInfoScope, MessageReadDetails, MessageReadParticipant};
use crate::{
    engine::{now_ms, Engine, EntryBody, R},
    CoreError,
};
use protocol::read_time::RETENTION_MS;
use types::{Annotation, EventBody};

impl Engine {
    pub(crate) fn message_info(&self, scope: &MessageInfoScope) -> R<MessageInfo> {
        let invalid = || CoreError::Forbidden {
            reason: "message metadata scope changed".into(),
        };
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        let me = &scope.expected_identity_id;
        let members = self.receipt_members(&scope.space_id)?;
        let state = self.space_state(&scope.space_id)?;
        if state.integrity_error.is_some() {
            return Err(invalid());
        }
        let entry = state
            .entries
            .iter()
            .find(|e| e.id() == scope.message_id)
            .ok_or_else(invalid)?;
        if !members.iter().any(|(id, _)| id == me)
            && !(entry.hash.is_empty()
                && &entry.author == me
                && self.is_member(&scope.space_id, me))
        {
            return Err(invalid());
        }
        let EntryBody::Message { marks, .. } = &entry.body else {
            return Err(invalid());
        };
        if marks.deleted {
            return Err(invalid());
        }
        let edited_at_ms = if marks.edited {
            self.logs.get(&scope.space_id).and_then(|log| {
                log.events().iter().rev().find(|event| {
                    event.author == entry.author
                        && matches!(&event.body, EventBody::MessageAnnotated { target, note: Annotation::Edit { .. } } if target == entry.id())
                        && marks.text_revision.as_deref() == Some(event_log::content_hash_of(event).as_str())
                }).map(|event| event.at_ms)
            })
        } else {
            None
        };
        let details = if &entry.author != me {
            MessageReadDetails::NotSender
        } else if entry.hash.is_empty() || entry.seq == 0 {
            MessageReadDetails::Unconfirmed
        } else if !self.read_time_allowed(&scope.space_id)? {
            MessageReadDetails::NotSmallConversation
        } else {
            MessageReadDetails::Available
        };
        let mut participants = Vec::new();
        if details == MessageReadDetails::Available {
            for (peer, _) in members.iter().filter(|(id, _)| id != me) {
                let (delivered, read) = self
                    .net
                    .receipts
                    .get(&scope.space_id)
                    .and_then(|r| r.get(peer))
                    .copied()
                    .unwrap_or((0, 0));
                let read = read >= entry.seq;
                // Exact interval evidence only. A later prefix cannot date an older read.
                let report = if read {
                    self.store
                        .read_times(&scope.space_id, peer, now_ms().saturating_sub(RETENTION_MS))?
                        .into_iter()
                        .filter(|r| r.after_seq < entry.seq && entry.seq <= r.through_seq)
                        .min_by_key(|r| r.observed_at_ms)
                } else {
                    None
                };
                participants.push(MessageReadParticipant {
                    person: self.persona(peer),
                    delivered: delivered >= entry.seq || read,
                    read,
                    read_reported_at_ms: report.as_ref().map(|r| r.reported_at_ms),
                    read_observed_at_ms: report.map(|r| r.observed_at_ms),
                });
            }
        }
        Ok(MessageInfo {
            scope: scope.clone(),
            author: self.persona(&entry.author),
            sent_at_ms: entry.at_ms,
            edited: marks.edited,
            edited_at_ms,
            delivery: self.delivery(entry),
            read_details: details,
            participants,
        })
    }
}
