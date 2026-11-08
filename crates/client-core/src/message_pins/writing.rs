//! Existing signed events/outbox retain exact operation receipts across response loss and reopen.
use super::{
    changed, Annotation, Engine, EntryBody, Event, EventBody, MessagePinEdit, MessagePinResult, R,
};
impl Engine {
    fn pin_receipt(input: &MessagePinEdit, event: &Event, failed: bool) -> R<Option<String>> {
        let EventBody::MessageAnnotated {
            target,
            note: Annotation::Pin { change },
        } = &event.body
        else {
            return Ok(None);
        };
        if change.operation_id != input.change.operation_id {
            return Ok(None);
        }
        if failed
            || event.space != input.scope.space_id
            || event.author != input.scope.expected_identity_id
            || target != &input.message_id
            || change != &input.change
        {
            return Err(changed());
        }
        Ok(Some(event_log::content_hash_of(event)))
    }
    pub(crate) fn set_message_pin(&mut self, input: &MessagePinEdit) -> R<MessagePinResult> {
        self.pin_source(&input.scope, &input.message_id)?;
        if !self
            .state
            .pin_writer(&input.scope.space_id, &input.scope.expected_identity_id)
        {
            return Err(changed());
        }
        let mut receipt = None;
        for log in self.logs.values() {
            for event in log.events() {
                if let Some(value) = Self::pin_receipt(input, event, false)? {
                    receipt = Some(value);
                }
            }
        }
        for pending in self.store.outbox()? {
            if let Some(value) = Self::pin_receipt(input, &pending.event, pending.failed)? {
                receipt = Some(value);
            }
        }
        let revision = if let Some(receipt) = receipt {
            receipt
        } else {
            let body = EventBody::MessageAnnotated {
                target: input.message_id.clone(),
                note: Annotation::Pin {
                    change: input.change.clone(),
                },
            };
            // Use the actual per-space current projection, not a parallel pin store.
            let accepted = self
                .space_projections
                .get(&input.scope.space_id)
                .is_some_and(|s| {
                    s.pin_change_valid(
                        &input.scope.space_id,
                        &input.scope.expected_identity_id,
                        &input.message_id,
                        &input.change,
                    )
                });
            if !accepted {
                return Err(changed());
            }
            let event = self.append(
                &input.scope.space_id,
                &input.scope.expected_identity_id,
                body,
            )?;
            event_log::content_hash_of(&event)
        };
        let entry = self.pin_source(&input.scope, &input.message_id)?;
        if !matches!(&entry.body,EntryBody::Message { marks, .. } if marks.pin.as_ref().and_then(|p| p.receipts.get(&input.change.operation_id)) == Some(&revision))
        {
            return Err(changed());
        }
        Ok(MessagePinResult {
            review: self.message_pin_review(&input.scope, &input.message_id)?,
            applied_revision: revision,
        })
    }
}
