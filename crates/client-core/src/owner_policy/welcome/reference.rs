//! Existing link.asked metadata associates one signed outbox original with its
//! canonical confirmation. It is not a second membership or financial journal.
use super::*;

const MAX_BYTES: usize = 16_384;

pub(super) fn marker(space: &str, device: &str) -> String {
    format!("link.asked:{space}:{device}")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct JoinReference {
    original: Event,
    source: String,
    query: Seen,
    pub(super) inquiry_digest: String,
    pub(super) stale_leaf: Option<Vec<u8>>,
    accepted: Option<Seen>,
}

impl JoinReference {
    pub(super) fn new(
        original: Event,
        query: &Event,
        digest: &str,
        stale_leaf: Option<Vec<u8>>,
    ) -> Self {
        Self {
            original,
            source: query.client_id.clone(),
            query: Seen {
                seq: query.seq,
                hash: query.hash.clone(),
            },
            inquiry_digest: digest.into(),
            stale_leaf,
            accepted: None,
        }
    }

    pub(super) fn decode(raw: &str) -> R<Self> {
        if raw.len() > MAX_BYTES {
            return Err(invalid());
        }
        let value: Self = serde_json::from_str(raw).map_err(|_| invalid())?;
        if value.encode()? != raw {
            return Err(invalid());
        }
        Ok(value)
    }

    pub(super) fn encode(&self) -> R<String> {
        let raw = serde_json::to_string(self).map_err(|_| invalid())?;
        if raw.len() > MAX_BYTES {
            return Err(invalid());
        }
        Ok(raw)
    }
}

impl Engine {
    pub(super) fn validate_policy_welcome_reference(
        &self,
        space: &str,
        target: &str,
        reference: &JoinReference,
    ) -> R<Option<Seen>> {
        let original = &reference.original;
        let account = self.account().ok_or_else(denied)?;
        if original.seq != 0
            || original.space != space
            || original.author != account.identity
            || original.device.as_deref() != Some(account.device.as_str())
            || original.cert.as_deref() != Some(account.cert.as_str())
            || original.sealed_wire.is_some()
            || !matches!(&original.body, EventBody::DeviceJoining { device } if device == target)
            || event_log::verify_author(original).is_err()
        {
            return Err(denied());
        }
        let log = self.logs.get(space).ok_or_else(denied)?;
        let at = |seq| {
            usize::try_from(seq)
                .ok()
                .and_then(|seq| log.events().get(seq))
        };
        let query = at(reference.query.seq).ok_or_else(denied)?;
        let EventBody::OwnerPolicyOptionsRequested { agent, signed } = &query.body else {
            return Err(denied());
        };
        if query.hash != reference.query.hash
            || query.client_id != reference.source
            || query.author != signed.inquiry.owner
            || agent != &account.identity
            || signed.inquiry.agent != account.identity
            || signed.inquiry.space != space
            || query.sealed_wire.is_none()
            || query.device.as_deref() != Some(signed.device.as_str())
            || query.cert.as_deref() != Some(signed.cert.as_str())
            || event_log::verify_event(query).is_err()
            || !event_log::owner_policy_inquiry::verify(signed)
            || event_log::owner_policy_inquiry::digest(&signed.inquiry).as_deref()
                != Some(reference.inquiry_digest.as_str())
        {
            return Err(denied());
        }
        let seen = original.seen.as_ref().ok_or_else(denied)?;
        if seen.seq < query.seq || at(seen.seq).is_none_or(|event| event.hash != seen.hash) {
            return Err(denied());
        }
        let pending = self.store.outbox_get(&original.client_id)?;
        match &reference.accepted {
            None => {
                let pending = pending.ok_or_else(denied)?;
                if event_log::content_hash_of(&pending) != event_log::content_hash_of(original)
                    || pending.sig != original.sig
                    || event_log::verify_author(&pending).is_err()
                {
                    return Err(denied());
                }
                Ok(None)
            }
            Some(position) => {
                let accepted = at(position.seq).ok_or_else(denied)?;
                if pending.is_some()
                    || accepted.hash != position.hash
                    || accepted.client_id != original.client_id
                    || accepted.sig != original.sig
                    || event_log::content_hash_of(accepted) != event_log::content_hash_of(original)
                    || event_log::verify_event(accepted).is_err()
                {
                    return Err(denied());
                }
                Ok(Some(position.clone()))
            }
        }
    }

    /// Within the existing confirmation transaction, before removing the exact
    /// outbox original. Any association failure rolls back event+outbox together.
    pub(crate) fn confirm_owner_policy_welcome(&self, event: &Event) -> R<()> {
        let EventBody::DeviceJoining { device } = &event.body else {
            return Ok(());
        };
        let key = marker(&event.space, device);
        let Some(raw) = self.store.meta(&key)? else {
            return Ok(());
        };
        if raw == "1" {
            return Ok(()); // Existing unrelated sibling intent; no new proof inferred.
        }
        let mut reference = JoinReference::decode(&raw)?;
        if event.client_id != reference.original.client_id {
            return Ok(());
        }
        if self
            .validate_policy_welcome_reference(&event.space, device, &reference)?
            .is_some()
            || event.sig != reference.original.sig
            || event_log::content_hash_of(event) != event_log::content_hash_of(&reference.original)
            || event_log::verify_event(event).is_err()
        {
            return Err(denied());
        }
        reference.accepted = Some(Seen {
            seq: event.seq,
            hash: event.hash.clone(),
        });
        self.store.set_meta(&key, &reference.encode()?)?;
        Ok(())
    }
}
