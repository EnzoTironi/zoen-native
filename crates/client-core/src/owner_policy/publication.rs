//! Immutable originals for the three non-model agent control receipts.
//! This is an echo association, never installation or financial authority.
use super::{control, denied, invalid, options, Deserialize, Engine, EventBody, Serialize, R};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use event_log::{content::SealedKind, content_hash_of, verify_author, Author};
use protocol::{ClientFrame, Envelope};
use types::Event;

const MAX_REFERENCE_BYTES: usize = 135_168;
fn key(client_id: &str) -> String {
    format!("runtime.owner-policy.reference/1/{client_id}")
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    space: String,
    source: String,
    original_hash: String,
    event_digest: String,
    wire: String,
}

fn source(body: &EventBody) -> Option<&str> {
    match body {
        EventBody::OwnerPolicyReported { source, .. }
        | EventBody::OwnerPolicyOptionsReported { source, .. }
        | EventBody::OwnerPolicyOptionsRefused { source, .. } => Some(source),
        _ => None,
    }
}

impl Engine {
    fn policy_report_original(&self, event: &Event) -> R<&Event> {
        let source = source(&event.body).ok_or_else(denied)?;
        let original = self
            .logs
            .get(&event.space)
            .and_then(|log| log.events().iter().find(|value| value.client_id == source))
            .ok_or_else(denied)?;
        if original.seq == 0 || original.sealed_wire.is_none() || verify_author(original).is_err() {
            return Err(denied());
        }
        let matches = match (&event.body, &original.body) {
            (
                EventBody::OwnerPolicyReported { policy_digest, .. },
                EventBody::OwnerPolicySubmitted { .. },
            ) => control::original(original, &event.author)
                .is_some_and(|(_, digest)| &digest == policy_digest),
            (body, EventBody::OwnerPolicyOptionsRequested { agent, signed }) => {
                agent == &event.author
                    && signed.inquiry.agent == event.author
                    && signed.inquiry.owner == original.author
                    && signed.inquiry.space == event.space
                    && original.device.as_deref() == Some(signed.device.as_str())
                    && original.cert.as_deref() == Some(signed.cert.as_str())
                    && event_log::owner_policy_inquiry::verify(signed)
                    && match body {
                        EventBody::OwnerPolicyOptionsReported { options, .. } => {
                            options::validate_options(signed, options, event.at_ms).is_ok()
                        }
                        EventBody::OwnerPolicyOptionsRefused { inquiry_digest, .. } => {
                            event_log::owner_policy_inquiry::digest(&signed.inquiry).as_ref()
                                == Some(inquiry_digest)
                        }
                        _ => false,
                    }
            }
            _ => false,
        };
        if !matches {
            return Err(denied());
        }
        Ok(original)
    }

    pub(super) fn append_owner_policy_report(
        &mut self,
        space: &str,
        author: &Author,
        body: EventBody,
    ) -> R<()> {
        // Append, seal and retain one producer original in the same transaction.
        // Failure reloads the durable Engine, including pending and MLS state.
        self.atomic_agent_action(|engine| {
            let event = engine.append_synced(space, author, crate::engine::now_ms(), body)?;
            let original_hash = engine.policy_report_original(&event)?.hash.clone();
            let epoch = engine.ready_epoch(space).ok_or_else(denied)?;
            engine.seal_outgoing_in_transaction(&event, epoch)?;
            let (_, envelope) = engine.sealed_copy(&event).ok_or_else(denied)?;
            let reference = Reference {
                space: event.space.clone(),
                source: source(&event.body).ok_or_else(denied)?.into(),
                original_hash,
                event_digest: content_hash_of(&event),
                wire: STANDARD.encode(
                    ClientFrame::Publish {
                        env: envelope.clone(),
                    }
                    .encode(),
                ),
            };
            let json = serde_json::to_string(&reference).map_err(|_| invalid())?;
            if json.len() > MAX_REFERENCE_BYTES
                || engine.store.meta(&key(&event.client_id))?.is_some()
            {
                return Err(invalid());
            }
            engine.store.set_meta(&key(&event.client_id), &json)?;
            if engine.owner_policy_pending_envelope(&event)?.as_ref() != Some(&envelope) {
                return Err(denied());
            }
            Ok(())
        })
    }

    fn owner_policy_reference_envelope(&self, event: &Event) -> R<Option<Envelope>> {
        let Some(raw) = self.store.meta(&key(&event.client_id))? else {
            return Ok(None);
        };
        if raw.len() > MAX_REFERENCE_BYTES {
            return Err(invalid());
        }
        let reference: Reference = serde_json::from_str(&raw).map_err(|_| invalid())?;
        let original = self.policy_report_original(event)?;
        if serde_json::to_string(&reference).map_err(|_| invalid())? != raw
            || event.space != reference.space
            || source(&event.body) != Some(reference.source.as_str())
            || original.hash != reference.original_hash
            || content_hash_of(event) != reference.event_digest
            || verify_author(event).is_err()
        {
            return Err(denied());
        }
        let bytes = STANDARD.decode(&reference.wire).map_err(|_| invalid())?;
        let frame = ClientFrame::decode(&bytes).map_err(|_| invalid())?;
        if STANDARD.encode(&bytes) != reference.wire || frame.encode() != bytes {
            return Err(denied());
        }
        let ClientFrame::Publish { env } = frame else {
            return Err(denied());
        };
        let account = self.account().ok_or_else(denied)?;
        if event.author != account.identity
            || event.device.as_deref() != Some(account.device.as_str())
            || event.cert.as_deref() != Some(account.cert.as_str())
            || env.client_id() != event.client_id
            || env.space() != event.space
            || env.author() != event.author
            || env.device() != event.device.as_deref()
            || env.cert != event.cert
            || env.at_ms() != event.at_ms
            || env.seen() != event.seen
            || env.sealed_kind() != Some(SealedKind::Application)
            || env.is_pruned()
            || env.invite.is_some()
            || env.recovery().is_some()
            || env.verify().is_err()
        {
            return Err(denied());
        }
        Ok(Some(env))
    }

    pub(crate) fn owner_policy_pending_envelope(&self, event: &Event) -> R<Option<Envelope>> {
        let Some(env) = self.owner_policy_reference_envelope(event)? else {
            return Ok(None);
        };
        let (epoch, stored) = self.sealed_copy(event).ok_or_else(denied)?;
        if stored != env
            || env
                .sealed_data()
                .and_then(|(_, bytes)| ::mls::application_epoch(bytes))
                != Some(epoch)
        {
            return Err(denied());
        }
        Ok(Some(env))
    }

    pub(crate) fn owner_policy_accepted_envelope(
        &self,
        space: &str,
        client_id: &str,
    ) -> R<Option<(Event, Envelope)>> {
        let reference = self.store.meta(&key(client_id))?;
        let account = self.account().ok_or_else(denied)?;
        let mut matching = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(event_log::SpaceLog::events)
            .filter(|event| event.client_id == client_id);
        let event = matching.next();
        if reference.is_none() {
            if event.is_some_and(|event| {
                source(&event.body).is_some()
                    && event.author == account.identity
                    && event.device.as_deref() == Some(account.device.as_str())
            }) {
                return Err(denied());
            }
            return Ok(None);
        }
        let event = event.ok_or_else(denied)?;
        let env = self
            .owner_policy_reference_envelope(event)?
            .ok_or_else(denied)?;
        if matching.next().is_some()
            || self.net.pending.contains_key(client_id)
            || self.store.outbox_get(client_id)?.is_some()
            || self
                .store
                .meta(&format!("mls.sealed:{client_id}"))?
                .is_some()
            || self
                .store
                .meta(&format!("mls.welcome:{client_id}"))?
                .is_some()
            || event_log::verify_event(event).is_err()
            || !crate::mls_opening::exact_own_echo(&self.store, event, account, &env)
        {
            return Err(denied());
        }
        Ok(Some((event.clone(), env)))
    }
}
