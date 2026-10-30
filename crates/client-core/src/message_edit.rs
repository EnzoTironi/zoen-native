//! Confirm an edit against the source actually opened by this account/device.
use crate::{engine::EntryBody, Client, CoreError};
pub use protocol::message_edit::MessageEdit;

fn changed() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "A mensagem mudou. Abra a edição novamente; seu texto foi preservado.",
            "The message changed. Reopen the edit; your text was preserved.",
        ),
    }
}
impl Client {
    pub fn edit_reviewed_message(&self, input: MessageEdit) -> Result<(), CoreError> {
        let mut engine = self.lock();
        engine.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        engine.check_private_message_view()?;
        if [&input.space_id, &input.message_id, &input.expected_revision]
            .iter()
            .any(|v| v.is_empty() || v.len() > 256 || v.chars().any(char::is_control))
        {
            return Err(changed());
        }
        if !engine
            .state
            .writer(&input.space_id, &input.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: crate::i18n::t("o acesso à conversa mudou", "conversation access changed"),
            });
        }
        if let Some(reason) = engine.checked_integrity_error(&input.space_id)? {
            return Err(CoreError::Invalid { reason });
        }
        let source = engine
            .space_state(&input.space_id)?
            .entries
            .iter()
            .find(|e| e.id() == input.message_id)
            .ok_or_else(changed)?;
        let EntryBody::Message { text, marks, .. } = &source.body else {
            return Err(changed());
        };
        if source.author != input.expected_identity_id {
            return Err(CoreError::Forbidden {
                reason: crate::i18n::t("só quem escreveu edita", "only the author can edit"),
            });
        }
        if marks.deleted || engine.deleted_for_self(&input.space_id, &input.message_id) {
            return Err(changed());
        }
        // Reconcile an unknown acknowledgement without another signed annotation.
        // Authority/availability are checked even when the desired text is present.
        if text == &input.text {
            return Ok(());
        }
        if marks.text_revision.as_deref().unwrap_or(source.id()) != input.expected_revision {
            return Err(changed());
        }
        engine.atomic_agent_action(|engine| {
            engine.annotate(
                &input.space_id,
                &input.message_id,
                types::Annotation::Edit { text: input.text },
            )
        })
    }
}

#[cfg(test)]
mod tests;
