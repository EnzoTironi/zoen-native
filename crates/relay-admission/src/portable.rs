//! Portable verification and sequencing boundary. Facts come from the trusted
//! storage projection after session authorization, never from the public client.
//! This module does not establish directory authorization or commit any effect.

use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::chain_hash;
use protocol::{ClientFrame, Envelope, Sequenced};
use serde::{Deserialize, Serialize};
use types::{EventBody, Privacy, Role, SpaceKind, GENESIS_PREV};

use crate::{admit, identity_key, Effect, Facts, Reject};

/// Matches the current native WebSocket framing and envelope admission bounds.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = 90 * 1024;
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedHash {
    pub seq: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetFacts {
    pub identity: String,
    pub role: Option<Role>,
    pub registered: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InviteFacts {
    pub code_hash: String,
    pub space: String,
    pub role: Role,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MlsFacts {
    pub next_epoch: String,
    pub last_device: String,
}

/// All values must have been read for this exact Space/author at `head`.
/// Target facts and history hashes carry their own lookup keys to prevent
/// accidentally admitting one request with a different request's projection.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactSnapshot {
    pub space: String,
    pub author: String,
    pub head: Option<ObservedHash>,
    pub kind: Option<SpaceKind>,
    pub privacy: Option<Privacy>,
    pub author_role: Option<Role>,
    pub member_count: u32,
    pub target: Option<TargetFacts>,
    #[serde(default)]
    pub history: Vec<ObservedHash>,
    pub invite: Option<InviteFacts>,
    pub mls: Option<MlsFacts>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedContext {
    pub authenticated_identity: String,
    pub authenticated_device: String,
    pub facts: FactSnapshot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionContext {
    pub authenticated_identity: String,
    pub authenticated_device: String,
}

/// An envelope whose signature and authenticated session binding were verified.
/// Private fields prevent callers from constructing a verified token directly.
#[derive(Debug)]
pub struct VerifiedPublish {
    pub(crate) env: Envelope,
}

impl VerifiedPublish {
    pub fn space(&self) -> &str {
        self.env.space()
    }
    pub fn author(&self) -> &str {
        self.env.author()
    }
    pub fn client_id(&self) -> &str {
        self.env.client_id()
    }
}

#[derive(Debug, Serialize)]
pub struct BoundaryError {
    pub code: &'static str,
    pub reason: String,
    pub permanent: bool,
}

impl BoundaryError {
    pub(crate) fn no(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            permanent: true,
        }
    }

    /// A trusted context/storage/encoding failure says nothing permanent about
    /// the signed client envelope. A host may reload facts and retry it.
    pub(crate) fn internal(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            permanent: false,
        }
    }
}

impl From<Reject> for BoundaryError {
    fn from(value: Reject) -> Self {
        Self {
            code: "admission",
            reason: value.reason,
            permanent: value.permanent,
        }
    }
}

#[derive(Debug)]
pub struct Admission {
    pub expected_head: Option<ObservedHash>,
    pub event: Sequenced,
    pub effect: Effect,
}

/// UInt64 JSON values cross the Worker boundary as exact canonical decimal text.
pub fn parse_u64(value: &str) -> Result<u64, BoundaryError> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err(BoundaryError::internal(
            "invalid_integer",
            "invalid UInt64 decimal string",
        ));
    }
    value
        .parse()
        .map_err(|_| BoundaryError::internal("invalid_integer", "UInt64 overflow"))
}

fn observed(value: &ObservedHash) -> Result<(u64, String), BoundaryError> {
    if value.hash.len() != 64 || !value.hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(BoundaryError::internal(
            "invalid_facts",
            "invalid observed chain hash",
        ));
    }
    Ok((parse_u64(&value.seq)?, value.hash.clone()))
}

fn facts_for(env: &Envelope, input: &FactSnapshot) -> Result<Facts, BoundaryError> {
    if input.space != env.space() || input.author != env.author() {
        return Err(BoundaryError::internal(
            "facts_scope",
            "facts belong to another Space or author",
        ));
    }
    if input.history.len() > 2 {
        return Err(BoundaryError::internal(
            "invalid_facts",
            "only cited history hashes belong in admission facts",
        ));
    }
    let head = input.head.as_ref().map(observed).transpose()?;
    if head.is_some() && (input.kind.is_none() || input.privacy.is_none()) {
        return Err(BoundaryError::internal(
            "invalid_facts",
            "existing Space metadata is missing",
        ));
    }
    let history = input
        .history
        .iter()
        .map(observed)
        .collect::<Result<Vec<_>, _>>()?;
    if history.len() == 2 && history[0].0 == history[1].0 {
        return Err(BoundaryError::internal(
            "invalid_facts",
            "duplicate history lookup",
        ));
    }
    let lookup = |seq| {
        history
            .iter()
            .find(|(at, _)| *at == seq)
            .map(|(_, hash)| hash.clone())
    };
    let body = env.body();
    let target_identity = match &body {
        Some(EventBody::MemberAdded { identity, .. } | EventBody::MemberRemoved { identity }) => {
            Some(identity)
        }
        _ => None,
    };
    let (target_role, target_known) = match (target_identity, &input.target) {
        (Some(identity), Some(target)) if identity == &target.identity => {
            let known = if matches!(&body, Some(EventBody::MemberAdded { .. })) {
                target.registered.ok_or_else(|| {
                    BoundaryError::internal("invalid_facts", "target directory lookup is missing")
                })?
            } else {
                true
            };
            (target.role, known)
        }
        (None, None) => (None, true),
        _ => {
            return Err(BoundaryError::internal(
                "facts_scope",
                "target facts do not match the envelope",
            ))
        }
    };
    let invite = match (&input.invite, env.invite.as_deref()) {
        (Some(facts), Some(code)) if facts.code_hash == crate::code_hash(code) => {
            Some((facts.space.clone(), facts.role))
        }
        (None, _) => None,
        _ => {
            return Err(BoundaryError::internal(
                "facts_scope",
                "invite facts do not match the transport capability",
            ))
        }
    };
    let mls = input
        .mls
        .as_ref()
        .map(|facts| {
            if !facts.last_device.is_empty() && !identity_key(&facts.last_device) {
                return Err(BoundaryError::internal(
                    "invalid_facts",
                    "invalid previous MLS device",
                ));
            }
            Ok((parse_u64(&facts.next_epoch)?, facts.last_device.clone()))
        })
        .transpose()?;
    Ok(Facts {
        head,
        kind: input.kind,
        privacy: input.privacy,
        author_role: input.author_role,
        target_role,
        member_count: input.member_count,
        seen_hash: env.seen().and_then(|seen| lookup(seen.seq)),
        invite,
        target_known,
        upto_hash: match body {
            Some(EventBody::Checkpoint { upto, .. }) => lookup(upto.seq),
            _ => None,
        },
        mls,
    })
}

/// Session authorization and rate limits happen before this call. Dedupe follows
/// this validation, before membership admission. Directory revocation is not
/// established by a valid certificate/signature alone.
pub fn verify_publish(
    frame: &[u8],
    context: &SessionContext,
) -> Result<VerifiedPublish, BoundaryError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(BoundaryError::no("too_large", "frame too large"));
    }
    let ClientFrame::Publish { env } = ClientFrame::decode(frame)
        .map_err(|_| BoundaryError::no("invalid_frame", "invalid client frame"))?
    else {
        return Err(BoundaryError::no(
            "invalid_frame",
            "only Publish frames enter admission",
        ));
    };
    if env.stored_len() > MAX_ENVELOPE_BYTES {
        return Err(BoundaryError::no("too_large", "too large"));
    }
    if !env.valid_invite() {
        return Err(BoundaryError::no("invalid_invite", "invalid invite code"));
    }
    // Preserve the native Session rule: root-signed legacy/agent envelopes do not
    // assert a device, but an asserted device must equal the authenticated one.
    if !identity_key(&context.authenticated_identity)
        || !identity_key(&context.authenticated_device)
        || env.author() != context.authenticated_identity
        || env
            .device()
            .is_some_and(|device| device != context.authenticated_device)
    {
        return Err(BoundaryError::no(
            "wrong_author",
            "you can only publish as yourself, from this device",
        ));
    }
    env.verify()
        .map_err(|error| BoundaryError::no("bad_signature", format!("signature: {error}")))?;
    Ok(VerifiedPublish { env })
}

pub fn admit_verified(
    verified: VerifiedPublish,
    snapshot: &FactSnapshot,
) -> Result<Admission, BoundaryError> {
    let env = verified.env;
    let facts = facts_for(&env, snapshot)?;
    let effect = admit(&env, &facts)?;
    let (seq, prev) = match &facts.head {
        Some((seq, hash)) => (
            seq.checked_add(1).ok_or_else(|| {
                BoundaryError::no("sequence_exhausted", "Space sequence exhausted")
            })?,
            hash.clone(),
        ),
        None => (0, GENESIS_PREV.into()),
    };
    let hash = chain_hash(env.space(), seq, &prev, &env.wire_hash());
    let mut stored = env;
    stored.invite = None;
    Ok(Admission {
        expected_head: snapshot.head.clone(),
        event: Sequenced {
            seq,
            prev,
            hash,
            env: stored,
        },
        effect,
    })
}

/// Convenience for a fresh mutation after the atomic storage transaction has
/// established that no dedupe receipt exists. Replays use verify_publish then
/// the original receipt, not admission against changed membership.
pub fn admit_publish(frame: &[u8], context: &TrustedContext) -> Result<Admission, BoundaryError> {
    let session = SessionContext {
        authenticated_identity: context.authenticated_identity.clone(),
        authenticated_device: context.authenticated_device.clone(),
    };
    admit_verified(verify_publish(frame, &session)?, &context.facts)
}

pub fn verify_publish_json(frame: &[u8], context: &str) -> Result<String, BoundaryError> {
    if context.len() > MAX_CONTEXT_BYTES {
        return Err(BoundaryError::internal(
            "too_large",
            "session context too large",
        ));
    }
    let context = serde_json::from_str(context)
        .map_err(|_| BoundaryError::internal("invalid_session", "invalid trusted session"))?;
    let verified = verify_publish(frame, &context)?;
    let env = &verified.env;
    let body = env.body();
    let target_identity = match &body {
        Some(EventBody::MemberAdded { identity, .. } | EventBody::MemberRemoved { identity }) => {
            Some(identity.clone())
        }
        _ => None,
    };
    let upto_seq = match &body {
        Some(EventBody::Checkpoint { upto, .. }) => Some(upto.seq.to_string()),
        _ => None,
    };
    serde_json::to_string(&serde_json::json!({
        "space_id": verified.space(), "author": verified.author(), "client_id": verified.client_id(),
        "lookup": { "target_identity": target_identity, "seen_seq": env.seen().map(|seen| seen.seq.to_string()),
            "upto_seq": upto_seq, "invite_hash": env.invite.as_deref().map(crate::code_hash) },
    })).map_err(|_| BoundaryError::internal("encoding", "could not encode verified publication"))
}

pub fn admit_publish_json(frame: &[u8], context: &str) -> Result<String, BoundaryError> {
    if context.len() > MAX_CONTEXT_BYTES {
        return Err(BoundaryError::internal(
            "too_large",
            "admission context too large",
        ));
    }
    let context = serde_json::from_str(context)
        .map_err(|_| BoundaryError::internal("invalid_facts", "invalid trusted context"))?;
    let accepted = admit_publish(frame, &context)?;
    let bytes_base64 = STANDARD.encode(accepted.event.encode());
    let effect = match &accepted.effect {
        Effect::Create { kind, privacy } => {
            serde_json::json!({"type":"create","kind":kind,"privacy":privacy})
        }
        Effect::Add {
            identity,
            role,
            by_invite,
        } => {
            serde_json::json!({"type":"add","identity":identity,"role":role,"by_invite":by_invite})
        }
        Effect::Remove { identity } => serde_json::json!({"type":"remove","identity":identity}),
        Effect::Encrypt => serde_json::json!({"type":"encrypt"}),
        Effect::Commit { epoch, device } => {
            serde_json::json!({"type":"commit","epoch":epoch.to_string(),"device":device})
        }
        Effect::Checkpoint { device, upto } => {
            serde_json::json!({"type":"checkpoint","device":device,"upto":upto.to_string()})
        }
        Effect::Hold { device } => serde_json::json!({"type":"hold","device":device}),
        Effect::Nothing => serde_json::json!({"type":"nothing"}),
    };
    serde_json::to_string(&serde_json::json!({
        "space_id": accepted.event.env.space(),
        "author": accepted.event.env.author(),
        "client_id": accepted.event.env.client_id(),
        "expected_head": accepted.expected_head,
        "event": {"seq":accepted.event.seq.to_string(),"prev":accepted.event.prev,"hash":accepted.event.hash,"bytes_base64":bytes_base64},
        "effect": effect,
    })).map_err(|_| BoundaryError::internal("encoding", "could not encode admission result"))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(js_name = admitPublish)]
    pub fn admit_publish(frame: &[u8], trusted_context: &str) -> Result<String, JsValue> {
        super::admit_publish_json(frame, trusted_context).map_err(|error| {
            JsValue::from_str(
                &serde_json::to_string(&error).unwrap_or_else(|_| "admission failed".into()),
            )
        })
    }

    #[wasm_bindgen(js_name = verifyPublish)]
    pub fn verify_publish(frame: &[u8], trusted_session: &str) -> Result<String, JsValue> {
        super::verify_publish_json(frame, trusted_session).map_err(|error| {
            JsValue::from_str(
                &serde_json::to_string(&error).unwrap_or_else(|_| "verification failed".into()),
            )
        })
    }
}
