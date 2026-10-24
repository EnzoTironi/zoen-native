//! A registered managed device cannot downgrade through ordinary publication.
use super::*;
pub(super) use crate::log::ManagedPublication as Appended;

impl Session {
    pub(super) async fn append_effect(
        &self,
        env: &protocol::Envelope,
        target_known: bool,
    ) -> Result<Appended, crate::log::Reject> {
        let Some(configured) = self.st.log.managed_device(&self.device) else {
            return self
                .st
                .log
                .append(env, target_known)
                .await
                .map(Appended::Sequenced);
        };
        let request = self
            .managed_binding
            .as_ref()
            .ok_or_else(|| crate::log::Reject::retry("managed runtime binding required"))?;
        let requested = publication_checkpoint(request)?;
        crate::runtime_binding::expectation(
            &self.st,
            &self.identity,
            &self.device,
            request.clone(),
        )
        .await
        .map_err(crate::log::Reject::retry)?;
        let current = crate::runtime_binding::current_managed(&self.st, &configured, request)
            .await
            .map_err(crate::log::Reject::retry)?;
        let _generation = configured
            .upload_guard(&current)
            .map_err(crate::log::Reject::retry)?;
        self.st
            .log
            .append_managed_effect(env, target_known, &requested)
            .await
    }
}

/// A valid bootstrap checkpoint is known read-only before any effect is consumed.
/// Keep every other failure retryable: it may represent uncertain prior work.
fn publication_checkpoint(
    request: &protocol::runtime_binding::Request,
) -> Result<relay_admission::managed_images::Checkpoint, crate::log::Reject> {
    crate::runtime_binding::effect_checkpoint(request).map_err(|reason| {
        if request
            .managed
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.valid() && checkpoint.sequence == 0)
        {
            crate::log::Reject::no(reason)
        } else {
            crate::log::Reject::retry(reason)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::runtime_binding::{ManagedCheckpoint, Request};

    #[test]
    fn bootstrap_is_known_refused_without_making_other_checkpoint_errors_permanent() {
        let mut request = Request {
            namespace: "bootstrap-refusal".into(),
            nonce: vec![73; 32],
            managed: Some(ManagedCheckpoint {
                generation: 1,
                sequence: 0,
                digest: "ab".repeat(32),
            }),
        };
        let refused = publication_checkpoint(&request).unwrap_err();
        assert!(refused.permanent);
        assert_eq!(refused.reason, "managed bootstrap binding is read-only");
        request.managed.as_mut().unwrap().sequence = 1;
        assert_eq!(publication_checkpoint(&request).unwrap().sequence, 1);
        request.managed.as_mut().unwrap().digest.clear();
        assert!(!publication_checkpoint(&request).unwrap_err().permanent);
        request.managed = None;
        assert!(!publication_checkpoint(&request).unwrap_err().permanent);
    }
}
