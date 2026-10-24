//! Last locally queued inquiry, committed atomically with its signed original.
//! This association recovers an ACK, never a quote, policy or financial grant.
use super::*;
use types::{owner_policy_context::OwnerPolicyInquiryPurpose, Event};

const PREFIX: &str = "runtime.owner-policy.inquiry/1/";
const MAX_BYTES: usize = 16_384;
const INITIALIZED: &str = "producer-index/1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    scope: OwnerPolicyScope,
    event: Event,
    order: u64,
}

fn key(
    scope: &OwnerPolicyScope,
    purpose: Option<OwnerPolicyInquiryPurpose>,
    request: &str,
    expected_hash: &str,
) -> R<String> {
    let bytes = serde_json::to_vec(&(
        "zoen-owner-policy-inquiry-index/1",
        scope,
        purpose,
        request,
        expected_hash,
    ))
    .map_err(|_| invalid())?;
    Ok(format!("{PREFIX}{}", event_log::sha256_hex(&bytes)))
}

fn target(
    event: &Event,
    scope: &OwnerPolicyScope,
    purpose: Option<OwnerPolicyInquiryPurpose>,
    request: &str,
    expected_hash: &str,
) -> bool {
    matches!(&event.body, EventBody::OwnerPolicyOptionsRequested { agent, signed }
        if event.space == scope.space_id && event.author == scope.expected_identity_id
        && event.device.as_deref() == Some(scope.expected_device_id.as_str())
        && agent == &scope.agent_id && signed.inquiry.purpose == purpose
        && signed.inquiry.request == request && signed.inquiry.content_hash == expected_hash)
}

fn decode_original(
    raw: &str,
    scope: &OwnerPolicyScope,
    purpose: Option<OwnerPolicyInquiryPurpose>,
    request: &str,
    expected_hash: &str,
) -> R<Original> {
    if raw.len() > MAX_BYTES {
        return Err(invalid());
    }
    let original: Original = serde_json::from_str(raw).map_err(|_| invalid())?;
    let event = &original.event;
    let EventBody::OwnerPolicyOptionsRequested { signed, .. } = &event.body else {
        return Err(denied());
    };
    if serde_json::to_string(&original).map_err(|_| invalid())? != raw
        || original.scope != *scope
        || original.order == 0
        || !target(event, scope, purpose, request, expected_hash)
        || event.seq != 0
        || event.sealed_wire.is_some()
        || signed.inquiry.owner != event.author
        || signed.inquiry.space != event.space
        || signed.inquiry.agent != scope.agent_id
        || event.device.as_deref() != Some(signed.device.as_str())
        || event.cert.as_deref() != Some(signed.cert.as_str())
        || event_log::verify_author(event).is_err()
        || !event_log::owner_policy_inquiry::verify(signed)
    {
        return Err(denied());
    }
    Ok(original)
}

impl Engine {
    fn inquiry_index_head(
        &self,
        index: &str,
        scope: &OwnerPolicyScope,
        purpose: Option<OwnerPolicyInquiryPurpose>,
        request: &str,
        expected_hash: &str,
    ) -> R<Option<Original>> {
        let head = self.store.meta(index)?;
        let marker = self.store.meta(&format!("{index}:initialized"))?;
        // Only the current signed producer is retained. Two rows are enough to
        // reject extra/corrupt records without scanning any inquiry history.
        let mut statement = self
            .store
            .conn()
            .prepare("SELECT key,value FROM meta WHERE key GLOB ?1 LIMIT 2")
            .map_err(store::StoreError::from)?;
        let mut rows = statement
            .query([format!("{index}/*")])
            .map_err(store::StoreError::from)?;
        let current = rows
            .next()
            .map_err(store::StoreError::from)?
            .map(|row| {
                Ok::<_, store::StoreError>((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .transpose()?;
        if rows.next().map_err(store::StoreError::from)?.is_some() {
            return Err(denied());
        }
        match (marker, head, current) {
            (None, None, None) => Ok(None),
            (Some(marker), Some(head), Some((record_key, raw)))
                if marker == INITIALIZED && head == raw =>
            {
                let original = decode_original(&head, scope, purpose, request, expected_hash)?;
                if record_key != format!("{index}/{}", original.event.client_id) {
                    return Err(denied());
                }
                Ok(Some(original))
            }
            _ => Err(denied()),
        }
    }

    pub(super) fn retain_owner_policy_inquiry(
        &mut self,
        scope: &OwnerPolicyScope,
        event: &Event,
    ) -> R<()> {
        let EventBody::OwnerPolicyOptionsRequested { agent, signed } = &event.body else {
            return Err(invalid());
        };
        if agent != &scope.agent_id
            || signed.inquiry.owner != scope.expected_identity_id
            || signed.inquiry.space != scope.space_id
            || event.device.as_deref() != Some(scope.expected_device_id.as_str())
            || event.device.as_deref() != Some(signed.device.as_str())
            || event.cert.as_deref() != Some(signed.cert.as_str())
            || event_log::verify_author(event).is_err()
            || !event_log::owner_policy_inquiry::verify(signed)
        {
            return Err(denied());
        }
        let index = key(
            scope,
            signed.inquiry.purpose,
            &signed.inquiry.request,
            &signed.inquiry.content_hash,
        )?;
        let previous = self.inquiry_index_head(
            &index,
            scope,
            signed.inquiry.purpose,
            &signed.inquiry.request,
            &signed.inquiry.content_hash,
        )?;
        let order = previous
            .as_ref()
            .map_or(Some(1), |value| value.order.checked_add(1))
            .ok_or_else(invalid)?;
        let raw = serde_json::to_string(&Original {
            scope: scope.clone(),
            event: event.clone(),
            order,
        })
        .map_err(|_| invalid())?;
        if raw.len() > MAX_BYTES {
            return Err(invalid());
        }
        let producer = format!("{index}/{}", event.client_id);
        if self.store.meta(&producer)?.is_some() {
            return Err(denied());
        }
        if let Some(previous) = previous {
            self.store
                .conn()
                .execute(
                    "DELETE FROM meta WHERE key=?1",
                    [format!("{index}/{}", previous.event.client_id)],
                )
                .map_err(store::StoreError::from)?;
        }
        self.store.set_meta(&producer, &raw)?;
        self.store.set_meta(&index, &raw)?;
        self.store
            .set_meta(&format!("{index}:initialized"), INITIALIZED)?;
        Ok(())
    }

    pub(super) fn find_policy_inquiry(
        &self,
        scope: &OwnerPolicyScope,
        purpose: Option<OwnerPolicyInquiryPurpose>,
        request: &str,
        expected_hash: &str,
    ) -> R<Option<OwnerPolicyOptionsStatus>> {
        self.owner_policy_conversation(scope)?;
        let index = key(scope, purpose, request, expected_hash)?;
        let Some(original) =
            self.inquiry_index_head(&index, scope, purpose, request, expected_hash)?
        else {
            // Legacy originals have no local producer association. Never guess
            // from their clock/ID; an ambiguous send remains held by the caller.
            // Explicit known-source reads remain available for those originals.
            return Ok(None);
        };
        let event = &original.event;
        let signed = self.owner_policy_query_record(scope, &event.client_id)?;
        if !matches!(&event.body, EventBody::OwnerPolicyOptionsRequested { signed: retained, .. }
            if retained == &signed)
        {
            return Err(denied());
        }
        let pending = self.store.outbox_get(&event.client_id)?;
        let current = pending
            .as_ref()
            .or_else(|| {
                self.logs.get(&scope.space_id).and_then(|log| {
                    log.events()
                        .iter()
                        .find(|value| value.client_id == event.client_id)
                })
            })
            .ok_or_else(denied)?;
        if current.space != event.space
            || current.author != event.author
            || current.device != event.device
            || current.cert != event.cert
            || current.at_ms != event.at_ms
            || current.seen != event.seen
            || current.body != event.body
            || event_log::verify_author(current).is_err()
        {
            return Err(denied());
        }
        if pending.is_some() {
            if self
                .store
                .meta(&format!("mls.sealed:{}", current.client_id))?
                .is_some()
            {
                let (_, envelope) = self.sealed_copy(current).ok_or_else(denied)?;
                if envelope.verify().is_err()
                    || envelope.client_id() != current.client_id
                    || envelope.space() != current.space
                    || envelope.author() != current.author
                    || envelope.device() != current.device.as_deref()
                    || envelope.cert != current.cert
                    || envelope.at_ms() != current.at_ms
                    || envelope.seen() != current.seen
                {
                    return Err(denied());
                }
            }
        } else if current.seq == 0
            || current.sealed_wire.is_none()
            || event_log::verify_event(current).is_err()
        {
            return Err(denied());
        }
        self.owner_policy_options(scope, &event.client_id).map(Some)
    }
}
