//! Reviewed input runs inside the existing serialized Rust client/queue.
use super::*;
use protocol::message_edit::MessageEdit;
impl<S: Signer> Client<S> {
    pub fn queue_reviewed_edit(&mut self, input: MessageEdit, now: i64) -> Result<Update, String> {
        let changed = || {
            localization::t(
                "A mensagem mudou. Reabra a edição; seu texto foi preservado.",
                "The message changed. Reopen the edit; your text was preserved.",
            )
        };
        if self.account.identity != input.expected_identity_id
            || self.account.device != input.expected_device_id
        {
            return Err(localization::t(
                "A conta ou o aparelho da edição mudou.",
                "The edit account or device changed.",
            ));
        }
        if [&input.space_id, &input.message_id, &input.expected_revision]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(changed());
        }
        self.document_writer(&input.space_id)?;
        let message = self
            .conversations()
            .into_iter()
            .find(|c| c.id == input.space_id)
            .and_then(|c| {
                c.messages.into_iter().find(|m| {
                    m.id == input.message_id || m.entry.as_deref() == Some(&input.message_id)
                })
            })
            .ok_or_else(changed)?;
        if message.author != self.account.identity || message.deleted {
            return Err(changed());
        }
        let resolves = |target: &str| {
            target == message.id
                || message.entry.as_deref() == Some(target)
                || self
                    .resolve_post(&input.space_id, target)
                    .is_some_and(|e| e.client_id == message.id)
        };
        let mut pending = self
            .saved
            .outbox
            .values()
            .filter(|p| p.failure.is_none())
            .filter_map(|p| p.inner.as_ref().map(|e| (p.order, e)))
            .filter(|(_, e)| e.space == input.space_id && e.author == input.expected_identity_id)
            .filter_map(|(order, e)| match &e.body {
                EventBody::MessageAnnotated { target, note } if resolves(target) => {
                    Some((order, e.client_id.as_str(), note))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|(order, id, _)| (*order, *id));
        if pending
            .iter()
            .any(|(_, _, note)| matches!(note, types::Annotation::Delete))
        {
            return Err(changed());
        }
        if let Some((_, _, types::Annotation::Edit { text })) = pending
            .iter()
            .rev()
            .find(|(_, _, note)| matches!(note, types::Annotation::Edit { .. }))
        {
            return if text == &input.text {
                self.update(Vec::new())
            } else {
                Err(changed())
            };
        }
        if message.text == input.text {
            return self.update(Vec::new());
        }
        if message.text_revision.as_deref() != Some(&input.expected_revision) {
            return Err(changed());
        }
        self.queue_annotation(
            &input.space_id,
            &input.message_id,
            types::Annotation::Edit { text: input.text },
            now,
        )
    }
}
