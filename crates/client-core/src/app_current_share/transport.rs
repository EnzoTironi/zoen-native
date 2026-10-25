//! Consent constrains ciphertext recipients before any checked projection.
use super::*;

pub(crate) fn request_share_mutation(request: &str) -> String {
    format!("app_request_{}", event_log::sha256_hex(request.as_bytes()))
}

impl Engine {
    pub(crate) fn app_share_consent_current(&self, event: &Event) -> bool {
        let EventBody::AppCurrentShared { share } = &event.body else {
            return true;
        };
        (self.is_e2e(&event.space) == share.epoch.is_some())
            && self
                .logs
                .get(&event.space)
                .is_some_and(|log| references_valid(log.events(), event))
    }

    pub(crate) fn app_share_epoch_matches(event: &Event, epoch: u64) -> bool {
        !matches!(&event.body, EventBody::AppCurrentShared { share }
            if share.epoch != Some(epoch))
    }

    pub(crate) fn refuse_stale_app_share(&mut self, event: &Event) -> R<()> {
        match self.reject(&event.client_id, &refused().to_string(), true, None) {
            crate::sync::RejectionOutcome::UserFailed => Ok(()),
            crate::sync::RejectionOutcome::StorageFailure(error) => Err(storage(error)),
            _ => Err(storage("pending App share refusal was not retained")),
        }
    }

    /// The existing failed outbox rows are the only failure receipt. Fail the
    /// exact composed request too, never every request or message in the Space.
    pub(crate) fn refuse_app_share_dependents(&self, event: &Event, reason: &str) -> R<()> {
        let EventBody::AppCurrentShared { share } = &event.body else {
            return Err(storage("expected App share"));
        };
        let rows = self.store.outbox()?;
        let tx = self
            .store
            .conn()
            .is_autocommit()
            .then(|| self.store.conn().unchecked_transaction())
            .transpose()
            .map_err(storage)?;
        for row in rows.iter().filter(|row| !row.failed) {
            let candidate = &row.event;
            let dependent = candidate.space == event.space
                && candidate.author == event.author
                && candidate.device == event.device
                && event_log::verify_author(candidate).is_ok()
                && matches!(&candidate.body, EventBody::AppEditRequested { reply, .. }
                    if reply.artifact.as_ref().is_some_and(|binding|
                        binding.item_id == share.item && binding.version == share.version
                        && share.content.document.as_ref().is_some_and(|file| file.sha256 == binding.sha256)
                        && request_share_mutation(&binding.request_id) == share.mutation));
            if candidate.client_id == event.client_id || dependent {
                self.store.outbox_note(&candidate.client_id, reason, true)?;
            }
        }
        if let Some(tx) = tx {
            tx.commit().map_err(storage)?;
        }
        Ok(())
    }
}
