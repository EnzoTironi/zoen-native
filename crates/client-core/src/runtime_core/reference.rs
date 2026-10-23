use base64::{engine::general_purpose::STANDARD, Engine as _};
use event_log::{content::SealedKind, content_hash_of, verify_author};
use protocol::{ClientFrame, Envelope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use types::*;

use super::publication::MAX_PUBLISH_BYTES;
use super::{
    direct_writers, DeviceCore, NativeError, ReplyIntent, Result, State, MAX_EVENT_BYTES, PROFILE,
};

pub(super) const MAX_REFERENCE_BYTES: usize = 135_168;
const MAX_REF_FIELD: usize = 256;

pub(super) fn reference_key(client_id: &str) -> String {
    format!("runtime.native-reply.v1/{client_id}")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReplyReference {
    version: u8,
    pub(super) original: ReplyIntent,
    pub(super) client_id: String,
    pub(super) message: String,
    pub(super) inner_digest: String,
    wire: String,
}

fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_REF_FIELD
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

impl ReplyReference {
    pub(super) fn digests(&self) -> Result<(String, String)> {
        let json = serde_json::to_vec(self).map_err(|_| NativeError::Storage)?;
        let wire = STANDARD
            .decode(&self.wire)
            .map_err(|_| NativeError::History)?;
        Ok((
            hex::encode(Sha256::digest(json)),
            hex::encode(Sha256::digest(wire)),
        ))
    }

    pub(super) fn envelope(&self, core: &DeviceCore) -> Result<Envelope> {
        if self.version != 1
            || self.original.profile != PROFILE
            || self.original.agent != core.agent
            || self.original.owner != core.owner
            || !digest(&self.original.trigger)
            || !digest(&self.original.trigger_digest)
            || !digest(&self.original.grant_digest)
            || !digest(&self.inner_digest)
        {
            return Err(NativeError::History);
        }
        if ![
            self.original.profile.as_str(),
            &self.original.agent,
            &self.original.owner,
            &self.original.space,
            &self.original.grant,
            &self.client_id,
            &self.message,
        ]
        .into_iter()
        .all(bounded)
            || self.wire.len() > MAX_PUBLISH_BYTES.div_ceil(3) * 4
        {
            return Err(NativeError::Bounds);
        }
        let bytes = STANDARD
            .decode(&self.wire)
            .map_err(|_| NativeError::History)?;
        if bytes.len() > MAX_PUBLISH_BYTES || STANDARD.encode(&bytes) != self.wire {
            return Err(NativeError::History);
        }
        let frame = ClientFrame::decode(&bytes).map_err(|_| NativeError::History)?;
        if frame.encode() != bytes {
            return Err(NativeError::History);
        }
        let ClientFrame::Publish { env } = frame else {
            return Err(NativeError::History);
        };
        let account = core
            .engine
            .net
            .account
            .as_ref()
            .ok_or(NativeError::Credential)?;
        if env.client_id() != self.client_id
            || env.space() != self.original.space
            || env.author() != core.agent
            || env.device() != Some(account.device.as_str())
            || env.cert.as_deref() != Some(account.cert.as_str())
            || env.is_pruned()
            || env.sealed_kind() != Some(SealedKind::Application)
            || env.invite.is_some()
            || env.recovery().is_some()
            || env.content().len() > MAX_EVENT_BYTES
            || env.verify().is_err()
        {
            return Err(NativeError::History);
        }
        core.verify_original_history(&self.original)?;
        Ok(env)
    }

    pub(super) fn matches_inner(&self, event: &Event, envelope: &Envelope) -> bool {
        event.client_id == self.client_id
            && event.space == self.original.space
            && event.author == self.original.agent
            && event.device.as_deref() == envelope.device()
            && event.cert == envelope.cert
            && event.at_ms == envelope.at_ms()
            && event.seen == envelope.seen()
            && event.content.len() <= MAX_EVENT_BYTES
            && content_hash_of(event) == self.inner_digest
            && verify_author(event).is_ok()
            && matches!(&event.body, EventBody::MessagePosted { message, attaches: None, reply: None, .. } if message == &self.message)
    }
}

impl DeviceCore {
    pub(super) fn retain_reply_reference(
        &self,
        original: &ReplyIntent,
        event: &Event,
        envelope: &Envelope,
    ) -> Result<(String, String)> {
        let EventBody::MessagePosted { message, .. } = &event.body else {
            return Err(NativeError::History);
        };
        let reference = ReplyReference {
            version: 1,
            original: original.clone(),
            client_id: event.client_id.clone(),
            message: message.clone(),
            inner_digest: content_hash_of(event),
            wire: STANDARD.encode(
                ClientFrame::Publish {
                    env: envelope.clone(),
                }
                .encode(),
            ),
        };
        let expected = reference.envelope(self)?;
        if !reference.matches_inner(event, &expected) || expected != *envelope {
            return Err(NativeError::History);
        }
        let key = reference_key(&reference.client_id);
        if self
            .engine
            .store
            .meta(&key)
            .map_err(|_| NativeError::Storage)?
            .is_some()
        {
            return Err(NativeError::History);
        }
        let json = serde_json::to_string(&reference).map_err(|_| NativeError::Storage)?;
        if json.len() > MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        self.engine
            .store
            .set_meta(&key, &json)
            .map_err(|_| NativeError::Storage)?;
        reference.digests()
    }

    pub(super) fn reply_reference(&self, client_id: &str) -> Result<Option<ReplyReference>> {
        if !bounded(client_id) {
            return Err(NativeError::Bounds);
        }
        let Some(json) = self
            .engine
            .store
            .meta(&reference_key(client_id))
            .map_err(|_| NativeError::Storage)?
        else {
            return Ok(None);
        };
        if json.len() > MAX_REFERENCE_BYTES {
            return Err(NativeError::Bounds);
        }
        let reference: ReplyReference =
            serde_json::from_str(&json).map_err(|_| NativeError::History)?;
        if reference.client_id != client_id
            || serde_json::to_string(&reference).map_err(|_| NativeError::History)? != json
        {
            return Err(NativeError::History);
        }
        reference.envelope(self)?;
        Ok(Some(reference))
    }

    fn verify_original_history(&self, original: &ReplyIntent) -> Result<()> {
        let log = self
            .engine
            .logs
            .get(&original.space)
            .ok_or(NativeError::History)?;
        let mut ordered = State::default();
        let mut grant_event = None;
        let mut plans = crate::pages::PlanReplay::new(&self.engine.store, &self.engine.logs);
        for (end, event) in log.events().iter().enumerate() {
            let context = plans
                .before(&ordered, &self.engine.identities, event, end)
                .map_err(|_| NativeError::History)?;
            let accepted = match ordered.apply_checked(event, &self.engine.identities, &context) {
                crate::pages::Projection::Applied => true,
                crate::pages::Projection::Refused => false,
                crate::pages::Projection::Waiting => return Err(NativeError::History),
            };
            if accepted
                && matches!(&event.body, EventBody::GrantIssued { grant } if grant.id == original.grant)
            {
                grant_event = Some(event);
            }
            if event.hash == original.trigger {
                let grant = ordered
                    .latest_trust(&original.space, &self.owner, &self.agent)
                    .ok_or(NativeError::History)?;
                let issued = grant_event.ok_or(NativeError::History)?;
                if !accepted
                    || event.author != self.owner
                    || content_hash_of(event) != original.trigger_digest
                    || !matches!(
                        event.body,
                        EventBody::MessagePosted {
                            attaches: None,
                            reply: None,
                            ..
                        }
                    )
                    || crate::mls_opening::opened_leaf(&self.engine.store, event)
                        .is_none_or(|leaf| leaf.identity != self.owner)
                    || content_hash_of(issued) != original.grant_digest
                    || crate::mls_opening::opened_leaf(&self.engine.store, issued)
                        .is_none_or(|leaf| leaf.identity != self.owner)
                    || grant.grant.id != original.grant
                    || grant
                        .grant
                        .expires_at_ms
                        .is_some_and(|at| at <= event.at_ms)
                    || !direct_writers(&ordered, &original.space, &self.owner, &self.agent)
                {
                    return Err(NativeError::History);
                }
                return Ok(());
            }
        }
        Err(NativeError::History)
    }
}
