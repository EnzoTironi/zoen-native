//! Non-model recovery of a configured Runtime's missing MLS group. No options,
//! refusal, policy installation or paid execution authority comes from this hint.
use super::*;
use types::{Event, Seen};

mod reference;
use reference::{marker, JoinReference};

/// Opaque pre-IPC lease. A response cannot borrow a replacement actor/group.
pub struct OwnerPolicyWelcomeAttempt {
    source: String,
    digest: String,
    actor: String,
    device: String,
    certificate: String,
    epoch: u64,
    leaves: ::mls::LeafKeys,
    connection: Option<(crate::link_api::DirectoryOwner, crate::net::RequestHandle)>,
}

impl Engine {
    pub(crate) fn owner_policy_welcome_attempt(
        &self,
        query: &OwnerPolicyInquiryControl,
    ) -> R<OwnerPolicyWelcomeAttempt> {
        let current = self.owner_policy_inquiry(query.space(), query.source())?;
        if current.digest() != query.digest() || current.signed() != query.signed() {
            return Err(denied());
        }
        let account = self.account().ok_or_else(denied)?;
        self.native_surface_owner(&account.identity, &account.device)?;
        let (active, revoked) = self.linked_device_directory()?;
        if !active.contains(&account.device) || revoked.contains(&account.device) {
            return Err(denied());
        }
        let device = self.device()?;
        let unavailable = |_| CoreError::Storage {
            message: "Could not observe current MLS recovery scope.".into(),
        };
        Ok(OwnerPolicyWelcomeAttempt {
            source: current.source().into(),
            digest: current.digest().into(),
            actor: account.identity.clone(),
            device: account.device.clone(),
            certificate: account.cert.clone(),
            epoch: device.epoch(query.space()).map_err(unavailable)?,
            leaves: device.leaves(query.space()).map_err(unavailable)?,
            connection: None,
        })
    }

    /// Called only for a precise configured Runtime MlsGroup reply, not generic
    /// IPC/storage/pending errors. Rechecks everything after the asynchronous cut.
    pub(crate) fn request_owner_policy_welcome(
        &mut self,
        query: &OwnerPolicyInquiryControl,
        attempt: &OwnerPolicyWelcomeAttempt,
        target: &str,
    ) -> R<bool> {
        self.request_owner_policy_welcome_at(query, attempt, target, crate::engine::now_ms())
    }

    pub(crate) fn request_owner_policy_welcome_at(
        &mut self,
        query: &OwnerPolicyInquiryControl,
        attempt: &OwnerPolicyWelcomeAttempt,
        target: &str,
        at_ms: i64,
    ) -> R<bool> {
        self.owner_policy_context_input(&query.signed().inquiry, at_ms)?;
        let current = self.owner_policy_welcome_attempt(query)?;
        if current.source != attempt.source
            || current.digest != attempt.digest
            || current.actor != attempt.actor
            || current.device != attempt.device
            || current.certificate != attempt.certificate
            || current.epoch != attempt.epoch
            || current.leaves != attempt.leaves
        {
            return Err(denied());
        }
        if self.owner_policy_options_reported(query)? {
            return Ok(false);
        }
        let (active, revoked) = self.linked_device_directory()?;
        if target == current.device || !active.contains(target) || revoked.contains(target) {
            return Err(denied());
        }
        let key = marker(query.space(), target);
        let original = self.store.meta(&key)?;
        let initialized = self
            .store
            .meta(&format!("{key}:owner-policy-initialized"))?;
        if original.is_some() != initialized.is_some()
            || initialized.as_deref().is_some_and(|value| value != "1")
        {
            return Err(denied());
        }
        if let Some(raw) = original {
            // Legacy suppression has no original association. It is never
            // silently erased/replaced into permission to rotate a leaf.
            let reference = JoinReference::decode(&raw)?;
            let accepted =
                self.validate_policy_welcome_reference(query.space(), target, &reference)?;
            if reference.inquiry_digest == current.digest || accepted.is_none() {
                return Ok(false);
            }
            let accepted = accepted.ok_or_else(denied)?;
            let event = self.policy_welcome_query_event(query)?;
            let seen = event.seen.as_ref().ok_or_else(denied)?;
            if seen.seq <= accepted.seq {
                return Ok(false);
            }
            // A removal or not-yet-rotated original leaf still owes the same
            // Welcome. Newer inquiries cannot mint another joining in that cut.
            let leaf = current.leaves.get(&(current.actor.clone(), target.into()));
            if leaf.is_none() || leaf == reference.stale_leaf.as_ref() {
                return Ok(false);
            }
        }
        let stale_leaf = current
            .leaves
            .get(&(current.actor.clone(), target.into()))
            .cloned();
        self.atomic_agent_action(|engine| {
            let event = engine.append(
                query.space(),
                &current.actor,
                EventBody::DeviceJoining {
                    device: target.into(),
                },
            )?;
            let source = engine.policy_welcome_query_event(query)?;
            let reference = JoinReference::new(event, source, query.digest(), stale_leaf);
            engine.store.set_meta(&key, &reference.encode()?)?;
            engine
                .store
                .set_meta(&format!("{key}:owner-policy-initialized"), "1")?;
            Ok(true)
        })
    }

    fn policy_welcome_query_event(&self, query: &OwnerPolicyInquiryControl) -> R<&Event> {
        let event = self
            .logs
            .get(query.space())
            .and_then(|log| {
                usize::try_from(query.sequence())
                    .ok()
                    .and_then(|seq| log.events().get(seq))
            })
            .ok_or_else(denied)?;
        if event.client_id != query.source() || event_log::verify_event(event).is_err() {
            return Err(denied());
        }
        Ok(event)
    }
}

impl Client {
    pub fn owner_policy_welcome_attempt(
        &self,
        query: &OwnerPolicyInquiryControl,
        worker_device: &str,
    ) -> R<OwnerPolicyWelcomeAttempt> {
        let (owner, channel) = self.device_directory_owner()?;
        self.with_device_directory_completion(&owner, &channel, |engine| {
            let mut attempt = engine.owner_policy_welcome_attempt(query)?;
            if attempt.device != worker_device {
                return Err(denied());
            }
            attempt.connection = Some((owner.clone(), channel.clone()));
            Ok(attempt)
        })
    }

    pub fn request_owner_policy_welcome(
        &self,
        query: &OwnerPolicyInquiryControl,
        attempt: &OwnerPolicyWelcomeAttempt,
        configured_device: &str,
    ) -> R<bool> {
        let (owner, channel) = attempt.connection.as_ref().ok_or_else(denied)?;
        self.with_device_directory_completion(owner, channel, |engine| {
            engine.request_owner_policy_welcome(query, attempt, configured_device)
        })
    }
}
