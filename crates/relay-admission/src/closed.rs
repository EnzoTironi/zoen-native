//! Private closed-group projection. This is not a complete LogStore or a session/directory authorizer.
//! The host supplies only trusted directory facts and commits the returned state atomically with the receipt.
use crate::invite::{authorize_invite_kind, authorize_inviter, invite_limits};
use crate::portable::{
    admit_verified, parse_u64, verify_publish, BoundaryError, FactSnapshot, InviteFacts, MlsFacts,
    ObservedHash, SessionContext, TargetFacts,
};
use crate::{code_hash, identity_key, mls_projection, Effect, MAX_MEMBERS};
use base64::{engine::general_purpose::STANDARD, Engine};
use protocol::Sequenced;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use types::{EventBody, Privacy, Role, SpaceKind};

pub mod reads;
pub mod relay;

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub removed_at: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mls: Option<MlsFacts>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<ProjectionHold>,
    #[serde(default = "zero", skip_serializing_if = "is_zero")]
    pub pruned: String,
}
fn zero() -> String {
    "0".into()
}
fn is_zero(value: &str) -> bool {
    value == "0"
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionHold {
    pub identity: String,
    pub device: String,
    pub seq: String,
    pub when_ms: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PruneRange {
    pub from: String,
    pub to_exclusive: String,
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
            removed_at: BTreeMap::new(),
            mls: None,
            holds: vec![],
            pruned: zero(),
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
            if !matches!(self.kind, Some(SpaceKind::Direct | SpaceKind::Group))
                || !matches!(self.privacy, Some(Privacy::Closed | Privacy::EndToEnd))
            {
                return Err(unsupported());
            }
            for (identity, removed_at) in &self.removed_at {
                if !identity_key(identity)
                    || self.members.contains_key(identity)
                    || parse_u64(removed_at)? > parse_u64(&head.seq)?
                {
                    return Err(internal("invalid removed member boundary"));
                }
            }
            self.tracking()?;
            if parse_u64(&self.pruned)? > parse_u64(&head.seq)? {
                return Err(internal("pruning cursor is beyond the Space head"));
            }
        } else if self.kind.is_some()
            || self.privacy.is_some()
            || !self.creator.is_empty()
            || !self.members.is_empty()
            || !self.removed_at.is_empty()
            || self.mls.is_some()
            || !self.holds.is_empty()
            || self.pruned != "0"
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
    fn tracking(&self) -> Result<(Option<(u64, String)>, mls_projection::Holds), BoundaryError> {
        if self.privacy != Some(Privacy::EndToEnd)
            && (self.mls.is_some() || !self.holds.is_empty() || self.pruned != "0")
        {
            return Err(internal("unencrypted projection has MLS retention state"));
        }
        let mls = self
            .mls
            .as_ref()
            .map(|value| {
                if !value.last_device.is_empty() && !identity_key(&value.last_device) {
                    return Err(internal("invalid last MLS device"));
                }
                Ok((parse_u64(&value.next_epoch)?, value.last_device.clone()))
            })
            .transpose()?;
        let mut holds = mls_projection::Holds::new();
        for value in &self.holds {
            if !self.members.contains_key(&value.identity)
                || !identity_key(&value.identity)
                || value.device != mls_projection::ADDED && !identity_key(&value.device)
            {
                return Err(internal("invalid retained member device"));
            }
            let seq = parse_u64(&value.seq)?;
            if self
                .head
                .as_ref()
                .is_none_or(|head| parse_u64(&head.seq).is_ok_and(|head| seq > head))
            {
                return Err(internal("member hold is beyond the Space head"));
            }
            let when = i64::try_from(parse_u64(&value.when_ms)?)
                .map_err(|_| internal("invalid hold timestamp"))?;
            if holds
                .insert((value.identity.clone(), value.device.clone()), (seq, when))
                .is_some()
            {
                return Err(internal("duplicate member hold"));
            }
        }
        Ok((mls, holds))
    }
    fn apply_tracking(&mut self, writes: &[mls_projection::Write]) -> Result<(), BoundaryError> {
        let (mut mls, mut holds) = self.tracking()?;
        mls_projection::apply(&mut mls, &mut holds, writes);
        self.mls = mls.map(|(epoch, device)| MlsFacts {
            next_epoch: epoch.to_string(),
            last_device: device,
        });
        self.holds = holds
            .into_iter()
            .map(|((identity, device), (seq, when))| ProjectionHold {
                identity,
                device,
                seq: seq.to_string(),
                when_ms: when.to_string(),
            })
            .collect();
        Ok(())
    }
    fn prune_range(&self, now: i64, ceiling: i64) -> Result<Option<PruneRange>, BoundaryError> {
        let (_, holds) = self.tracking()?;
        let from = parse_u64(&self.pruned)?;
        Ok(
            mls_projection::prune_floor(self.privacy, &self.members, &holds, now, ceiling)
                .filter(|floor| *floor > from)
                .map(|floor| PruneRange {
                    from: from.to_string(),
                    to_exclusive: floor.to_string(),
                }),
        )
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
    #[serde(default)]
    pub prune_ceiling_secs: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<PruneRange>,
}

fn internal(reason: &str) -> BoundaryError {
    BoundaryError::internal("invalid_projection", reason)
}
fn unsupported() -> BoundaryError {
    BoundaryError::internal("unsupported_projection", "only Direct and Group chats with Closed or EndToEnd privacy are supported by this private projection")
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
        mls: state.mls.clone(),
    };
    let accepted = admit_verified(verified, &facts)?;
    let when = i64::try_from(now).map_err(|_| internal("invalid transaction clock"))?;
    let (_, holds) = state.tracking()?;
    let writes = mls_projection::transition(
        &accepted.effect,
        state.privacy,
        accepted.event.env.author(),
        accepted.event.seq,
        when,
        &state.members,
        &holds,
    )
    .map_err(|reason| BoundaryError::no("admission", reason))?;
    // Apply retention against the pre-event membership, exactly as the native log.
    state.apply_tracking(&writes)?;
    let mut joined = None;
    let mut removed = None;
    let mut invite_update = None;
    match &accepted.effect {
        Effect::Create { kind, privacy }
            if matches!(kind, SpaceKind::Direct | SpaceKind::Group)
                && matches!(privacy, Privacy::Closed | Privacy::EndToEnd) =>
        {
            state.kind = Some(*kind);
            state.privacy = Some(*privacy);
            state.creator = accepted.event.env.author().into();
            state.members.insert(state.creator.clone(), Role::Owner);
        }
        Effect::Add {
            identity,
            role,
            by_invite,
        } => {
            state.members.insert(identity.clone(), *role);
            state.removed_at.remove(identity);
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
            state
                .removed_at
                .insert(identity.clone(), accepted.event.seq.to_string());
            removed = Some(identity.clone());
        }
        Effect::Encrypt => state.privacy = Some(Privacy::EndToEnd),
        Effect::Commit { .. }
        | Effect::Checkpoint { .. }
        | Effect::Hold { .. }
        | Effect::Nothing => {}
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
    let prune = state.prune_range(
        when,
        mls_projection::ceiling_ms(context.prune_ceiling_secs.as_deref()),
    )?;
    Ok(ClosedAcceptance {
        kind: "new",
        event: event_view(&accepted.event, &bytes),
        projection: state,
        audience,
        joined,
        invite_update,
        prune,
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
    if !protocol::valid_invite_code(&input.code) {
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

pub fn closed_prune_plan_json(
    projection: &str,
    now_ms: &str,
    ceiling_secs: &str,
) -> Result<String, BoundaryError> {
    let state: ClosedProjection = read_json(projection)?;
    state.validate(&state.space, false)?;
    let now =
        i64::try_from(parse_u64(now_ms)?).map_err(|_| internal("invalid compaction clock"))?;
    encode(&state.prune_range(now, mls_projection::ceiling_ms(Some(ceiling_secs)))?)
}

pub fn closed_pruned_json(
    projection: &str,
    from: &str,
    floor: &str,
    count: u32,
    last: &str,
) -> Result<String, BoundaryError> {
    let mut state: ClosedProjection = read_json(projection)?;
    state.validate(&state.space, false)?;
    let from = parse_u64(from)?;
    let floor = parse_u64(floor)?;
    if from != parse_u64(&state.pruned)?
        || state
            .head
            .as_ref()
            .is_none_or(|head| parse_u64(&head.seq).is_ok_and(|head| floor > head))
    {
        return Err(internal("compaction range does not match the projection"));
    }
    let last = (!last.is_empty()).then(|| parse_u64(last)).transpose()?;
    state.pruned = mls_projection::pruned_cursor(from, floor, count as usize, last)
        .map_err(internal)?
        .to_string();
    encode(&state)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    fn error(value: super::BoundaryError) -> JsValue {
        JsValue::from_str(
            &serde_json::to_string(&value).unwrap_or_else(|_| "projection failed".into()),
        )
    }
    #[wasm_bindgen(js_name = closedPruneEntry)]
    pub fn prune_entry(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        crate::mls_projection::pruned_bytes(bytes)
            .map(|value| value.unwrap_or_default())
            .map_err(|reason| error(super::internal(&reason)))
    }
    #[wasm_bindgen(js_name = closedPrunePlan)]
    pub fn prune_plan(
        projection: &str,
        now_ms: &str,
        ceiling_secs: &str,
    ) -> Result<String, JsValue> {
        super::closed_prune_plan_json(projection, now_ms, ceiling_secs).map_err(error)
    }
    #[wasm_bindgen(js_name = closedPruneState)]
    pub fn prune_state(
        projection: &str,
        from: &str,
        floor: &str,
        count: u32,
        last: &str,
    ) -> Result<String, JsValue> {
        super::closed_pruned_json(projection, from, floor, count, last).map_err(error)
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
