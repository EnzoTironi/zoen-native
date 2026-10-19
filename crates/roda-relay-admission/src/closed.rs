//! Private closed-group projection. This is not a complete LogStore or a session/directory authorizer.
//! The host supplies only trusted directory facts and commits the returned state atomically with the receipt.
use crate::invite::{authorize_invite_kind, authorize_inviter, invite_limits};
use crate::portable::{
    admit_verified, parse_u64, verify_publish, BoundaryError, FactSnapshot, InviteFacts,
    ObservedHash, SessionContext, TargetFacts,
};
use crate::{code_hash, identity_key, Effect, MAX_MEMBERS};
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_proto::Sequenced;
use roda_types::{EventBody, Privacy, Role, SpaceKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_CLOSED_CONTEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedProjection {
    pub version: u8,
    pub space: String,
    pub head: Option<ObservedHash>,
    pub kind: Option<SpaceKind>,
    pub privacy: Option<Privacy>,
    pub creator: String,
    pub members: BTreeMap<String, Role>,
}

impl ClosedProjection {
    pub fn empty(space: &str) -> Self {
        Self {
            version: 1,
            space: space.into(),
            head: None,
            kind: None,
            privacy: None,
            creator: String::new(),
            members: BTreeMap::new(),
        }
    }
    fn validate(&self, space: &str, fresh: bool) -> Result<(), BoundaryError> {
        if self.version != 1 || self.space != space {
            return Err(internal("projection scope or version mismatch"));
        }
        if let Some(head) = &self.head {
            parse_u64(&head.seq)?;
            if head.hash.len() != 64 || !head.hash.bytes().all(|v| v.is_ascii_hexdigit()) {
                return Err(internal("invalid projection head"));
            }
            if self.kind != Some(SpaceKind::Group) || self.privacy != Some(Privacy::Closed) {
                return Err(unsupported());
            }
        } else if self.kind.is_some()
            || self.privacy.is_some()
            || !self.creator.is_empty()
            || !self.members.is_empty()
        {
            return Err(internal("a missing head has nonempty projection state"));
        }
        if fresh && self.members.keys().any(|who| !identity_key(who)) {
            return Err(BoundaryError::no(
                "admission",
                "stored membership contains an unsupported identity",
            ));
        }
        Ok(())
    }
    fn supported_roster(&self) -> bool {
        self.members.len() <= MAX_MEMBERS as usize
            && self.members.keys().all(|who| identity_key(who))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedInvite {
    pub code_hash: String,
    pub space: String,
    pub role: Role,
    pub created_by: String,
    pub expires_ms: String,
    pub max_uses: u32,
    pub uses: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryTarget {
    pub identity: String,
    pub registered: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedContext {
    pub session: SessionContext,
    pub projection: Option<ClosedProjection>,
    pub space_id: String,
    pub history: Vec<ObservedHash>,
    pub target: Option<DirectoryTarget>,
    pub invite: Option<ClosedInvite>,
    pub now_ms: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CanonicalEvent {
    pub seq: String,
    pub prev: String,
    pub hash: String,
    pub bytes_base64: String,
}

#[derive(Debug, Serialize)]
pub struct ClosedAcceptance {
    pub kind: &'static str,
    pub event: CanonicalEvent,
    pub projection: ClosedProjection,
    pub audience: Vec<String>,
    pub joined: Option<String>,
    pub invite_update: Option<ClosedInvite>,
}

fn internal(reason: &str) -> BoundaryError {
    BoundaryError::internal("invalid_projection", reason)
}
fn unsupported() -> BoundaryError {
    BoundaryError::internal("unsupported_projection", "only closed Groups are supported by this private projection; MLS, holds and pruning are not implemented")
}
fn event_view(event: &Sequenced, bytes: &[u8]) -> CanonicalEvent {
    CanonicalEvent {
        seq: event.seq.to_string(),
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        bytes_base64: STANDARD.encode(bytes),
    }
}
fn read_json<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, BoundaryError> {
    if input.len() > MAX_CLOSED_CONTEXT_BYTES {
        return Err(internal("projection context too large"));
    }
    serde_json::from_str(input).map_err(|_| internal("invalid trusted projection context"))
}
fn encode(value: &impl Serialize) -> Result<String, BoundaryError> {
    serde_json::to_string(value).map_err(|_| {
        BoundaryError::internal("encoding", "could not encode closed projection result")
    })
}

pub fn closed_publish(
    frame: &[u8],
    context: &ClosedContext,
) -> Result<ClosedAcceptance, BoundaryError> {
    let verified = verify_publish(frame, &context.session)?;
    let mut state = context
        .projection
        .clone()
        .unwrap_or_else(|| ClosedProjection::empty(&context.space_id));
    state.validate(verified.space(), true)?;
    if context.space_id != verified.space() {
        return Err(internal("requested Space does not match the frame"));
    }
    let env = &verified.env;
    let body = env.body();
    let now = parse_u64(&context.now_ms)?;
    let invite = match (&context.invite, env.invite.as_deref()) {
        (Some(invite), Some(code)) => {
            if invite.code_hash != code_hash(code) {
                return Err(internal("invite lookup belongs to another code"));
            }
            let expires = parse_u64(&invite.expires_ms)?;
            if invite.max_uses == 0 || invite.max_uses > 10_000 || invite.uses > invite.max_uses {
                return Err(internal("invalid invite use counters"));
            }
            (expires > now && invite.uses < invite.max_uses).then_some(InviteFacts {
                code_hash: invite.code_hash.clone(),
                space: invite.space.clone(),
                role: invite.role,
            })
        }
        (None, _) => None,
        _ => return Err(internal("invite record has no transport capability")),
    };
    let target = match &body {
        Some(EventBody::MemberAdded { identity, .. } | EventBody::MemberRemoved { identity }) => {
            let registered = match &context.target {
                Some(target) if target.identity == *identity => Some(target.registered),
                None if matches!(&body, Some(EventBody::MemberRemoved { .. })) => None,
                _ => {
                    return Err(internal(
                        "directory target lookup does not match the signed envelope",
                    ))
                }
            };
            Some(TargetFacts {
                identity: identity.clone(),
                role: state.members.get(identity).copied(),
                registered,
            })
        }
        _ => None,
    };
    let facts = FactSnapshot {
        space: state.space.clone(),
        author: env.author().into(),
        head: state.head.clone(),
        kind: state.kind,
        privacy: state.privacy,
        author_role: state.members.get(env.author()).copied(),
        member_count: state.members.len() as u32,
        target,
        history: context.history.clone(),
        invite,
        mls: None,
    };
    let accepted = admit_verified(verified, &facts)?;
    let mut joined = None;
    let mut removed = None;
    let mut invite_update = None;
    match &accepted.effect {
        Effect::Create {
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        } => {
            state.kind = Some(SpaceKind::Group);
            state.privacy = Some(Privacy::Closed);
            state.creator = accepted.event.env.author().into();
            state.members.insert(state.creator.clone(), Role::Owner);
        }
        Effect::Add {
            identity,
            role,
            by_invite,
        } => {
            state.members.insert(identity.clone(), *role);
            joined = Some(identity.clone());
            if *by_invite {
                let mut updated = context
                    .invite
                    .clone()
                    .ok_or_else(|| internal("accepted invite is missing"))?;
                updated.uses = updated
                    .uses
                    .checked_add(1)
                    .ok_or_else(|| internal("invite use overflow"))?;
                invite_update = Some(updated);
            }
        }
        Effect::Remove { identity } => {
            state.members.remove(identity);
            removed = Some(identity.clone());
        }
        Effect::Nothing => {}
        _ => return Err(unsupported()),
    }
    state.head = Some(ObservedHash {
        seq: accepted.event.seq.to_string(),
        hash: accepted.event.hash.clone(),
    });
    let mut audience: Vec<_> = state.members.keys().cloned().collect();
    audience.extend(removed);
    audience.sort();
    let bytes = accepted.event.encode();
    Ok(ClosedAcceptance {
        kind: "new",
        event: event_view(&accepted.event, &bytes),
        projection: state,
        audience,
        joined,
        invite_update,
    })
}

pub fn closed_publish_json(frame: &[u8], context: &str) -> Result<String, BoundaryError> {
    encode(&closed_publish(frame, &read_json(context)?)?)
}

/// Verify the incoming retry, then return the original durable receipt with CURRENT membership.
/// No new membership rule or invite consumption runs on this path.
pub fn closed_replay_json(
    frame: &[u8],
    session: &str,
    projection: &str,
    receipt: &[u8],
) -> Result<String, BoundaryError> {
    let session: SessionContext = read_json(session)?;
    let verified = verify_publish(frame, &session)?;
    let state: ClosedProjection = read_json(projection)?;
    state.validate(verified.space(), false)?;
    if receipt.len() > crate::portable::MAX_FRAME_BYTES {
        return Err(internal("stored receipt too large"));
    }
    let original = Sequenced::decode(receipt).map_err(|_| internal("invalid stored receipt"))?;
    if original.env.space() != verified.space()
        || original.env.author() != verified.author()
        || original.env.client_id() != verified.client_id()
    {
        return Err(internal("stored receipt belongs to another publication"));
    }
    original
        .env
        .verify()
        .map_err(|_| internal("stored receipt signature is invalid"))?;
    let head = state
        .head
        .as_ref()
        .ok_or_else(|| internal("stored receipt has no projection head"))?;
    if original.seq > parse_u64(&head.seq)? {
        return Err(internal("stored receipt is beyond the projection head"));
    }
    let supported = state.supported_roster();
    let audience: Vec<_> = if supported {
        state.members.keys().cloned().collect()
    } else {
        Vec::new()
    };
    let joined = if supported {
        match original.env.body() {
            Some(EventBody::MemberAdded { identity, .. })
                if state.members.contains_key(&identity) =>
            {
                Some(identity)
            }
            _ => None,
        }
    } else {
        None
    };
    encode(
        &serde_json::json!({ "kind":"duplicate", "event":event_view(&original, receipt), "audience":audience, "joined":joined }),
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MintInvite {
    pub who: String,
    pub code: String,
    pub role: Role,
    pub max_uses: u32,
    pub ttl_secs: String,
    pub now_ms: String,
    pub existing: Option<ClosedInvite>,
}

pub fn closed_invite_json(projection: &str, input: &str) -> Result<String, BoundaryError> {
    let state: ClosedProjection = read_json(projection)?;
    let input: MintInvite = read_json(input)?;
    state.validate(&state.space, true)?;
    if !roda_proto::valid_invite_code(&input.code) {
        return Err(BoundaryError::no("invalid_invite", "invalid invite code"));
    }
    authorize_inviter(state.members.get(&input.who).copied(), input.role)
        .map_err(|reason| BoundaryError::no("invite", reason))?;
    if state.head.is_none() {
        return Err(BoundaryError::no("invite", "unknown space"));
    }
    authorize_invite_kind(state.kind.ok_or_else(|| internal("Space kind missing"))?)
        .map_err(|reason| BoundaryError::no("invite", reason))?;
    let (max, ttl_ms) = invite_limits(input.max_uses, parse_u64(&input.ttl_secs)?);
    let now = parse_u64(&input.now_ms)?;
    let expires = now
        .checked_add(ttl_ms as u64)
        .ok_or_else(|| internal("invite expiry overflow"))?;
    if expires > i64::MAX as u64 {
        return Err(internal(
            "invite expiry is outside the native timestamp range",
        ));
    }
    let code_hash = code_hash(&input.code);
    if let Some(existing) = input.existing {
        if existing.code_hash != code_hash
            || existing.space != state.space
            || existing.role != input.role
            || existing.created_by != input.who
            || existing.max_uses != max as u32
        {
            return Err(BoundaryError::no("invite", "invite code collision"));
        }
        return encode(&existing);
    }
    encode(&ClosedInvite {
        code_hash,
        space: state.space,
        role: input.role,
        created_by: input.who,
        expires_ms: expires.to_string(),
        max_uses: max as u32,
        uses: 0,
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: super::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value).unwrap_or_else(|_| "projection failed".into()),
        )
    }
    #[wasm_bindgen(js_name = closedPublish)]
    pub fn publish(frame: &[u8], context: &str) -> Result<String, JsValue> {
        super::closed_publish_json(frame, context).map_err(error)
    }
    #[wasm_bindgen(js_name = closedReplay)]
    pub fn replay(
        frame: &[u8],
        session: &str,
        projection: &str,
        receipt: &[u8],
    ) -> Result<String, JsValue> {
        super::closed_replay_json(frame, session, projection, receipt).map_err(error)
    }
    #[wasm_bindgen(js_name = closedInvite)]
    pub fn invite(projection: &str, input: &str) -> Result<String, JsValue> {
        super::closed_invite_json(projection, input).map_err(error)
    }
}
