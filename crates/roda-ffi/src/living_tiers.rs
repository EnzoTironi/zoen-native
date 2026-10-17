//! Tier policy and encrypted partitions for the default-off living Space experiment.
//! Kept separate from billing: Paid is a reserved, refused source in v1.
use crate::engine::{now_ms, Engine, R};
use crate::CoreError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use roda_log::sha256_hex;
use roda_types::{new_id, EventBody, ItemKind, Privacy, Role, SpaceKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) const TIER_META: &str = "\u{001e}zoen.space.tiers:";
pub(crate) const TIER_CLAIM: &str = "\u{001e}zoen.space.tier-claim:";
const MAX_TIERS: usize = 64;
const MAX_INVITES: usize = 512;
const MAX_INVITE_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

fn invalid(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}
fn forbidden() -> CoreError {
    CoreError::Forbidden {
        reason: "You don't have access to this Space.".into(),
    }
}
fn string<'a>(v: &'a Value, key: &str) -> R<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .ok_or_else(|| invalid("Some information is missing."))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntitlementSource {
    Manual,
    InviteLink,
    Paid,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Resource {
    id: String,
    kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Tier {
    id: String,
    name: String,
    space: String,
    groups: Vec<String>,
    resources: Vec<Resource>,
    members: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Entitlement {
    identity_id: String,
    tier_id: String,
    source: EntitlementSource,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Invite {
    id: String,
    tier_id: String,
    issuer: String,
    digest: String,
    expires_at_ms: i64,
    revoked: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TierState {
    tiers: Vec<Tier>,
    entitlements: Vec<Entitlement>,
    invites: Vec<Invite>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Token {
    space: String,
    tier_id: String,
    invite_id: String,
    secret: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claim {
    token: String,
}

// Deltas preserve unrelated offline admin edits. A stale complete snapshot must
// never silently drop another admin's grant while that person still has keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TierDelta {
    tiers: Vec<Tier>,
    grants: Vec<Entitlement>,
    revokes: Vec<Revocation>,
    invites: Vec<Invite>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Revocation {
    identity_id: String,
    tier_id: String,
}
impl TierDelta {
    fn between(before: &TierState, after: &TierState) -> Self {
        let tiers = after
            .tiers
            .iter()
            .filter(|tier| {
                !before.tiers.iter().any(|old| {
                    old.id == tier.id
                        && old.name == tier.name
                        && old.space == tier.space
                        && old.groups == tier.groups
                        && old.resources == tier.resources
                })
            })
            .map(|tier| {
                let mut value = tier.clone();
                value.members.clear();
                value
            })
            .collect();
        let grants = after
            .entitlements
            .iter()
            .filter(|grant| !before.entitlements.contains(grant))
            .cloned()
            .collect();
        let revokes = before
            .entitlements
            .iter()
            .filter(|old| {
                !after
                    .entitlements
                    .iter()
                    .any(|new| new.tier_id == old.tier_id && new.identity_id == old.identity_id)
            })
            .map(|old| Revocation {
                identity_id: old.identity_id.clone(),
                tier_id: old.tier_id.clone(),
            })
            .collect();
        let invites = after
            .invites
            .iter()
            .filter(|invite| !before.invites.contains(invite))
            .cloned()
            .collect();
        Self {
            tiers,
            grants,
            revokes,
            invites,
        }
    }
    fn apply(&self, state: &mut TierState) -> bool {
        if self.tiers.len() > MAX_TIERS
            || self.invites.len() > MAX_INVITES
            || self.grants.len() > 10000
            || self.revokes.len() > 10000
        {
            return false;
        }
        let mut next = state.clone();
        for incoming in &self.tiers {
            if !incoming.members.is_empty() {
                return false;
            }
            if let Some(existing) = next.tiers.iter_mut().find(|tier| tier.id == incoming.id) {
                if existing.space != incoming.space {
                    return false;
                }
                existing.name = incoming.name.clone();
                for group in &incoming.groups {
                    if !existing.groups.contains(group) {
                        existing.groups.push(group.clone());
                    }
                }
                for resource in &incoming.resources {
                    if !existing.resources.iter().any(|old| old.id == resource.id) {
                        existing.resources.push(resource.clone());
                    }
                }
            } else {
                next.tiers.push(incoming.clone());
            }
        }
        for revoked in &self.revokes {
            next.entitlements.retain(|grant| {
                !(grant.identity_id == revoked.identity_id && grant.tier_id == revoked.tier_id)
            });
        }
        for grant in &self.grants {
            if grant.source == EntitlementSource::Paid
                || !next.tiers.iter().any(|tier| tier.id == grant.tier_id)
            {
                return false;
            }
            next.entitlements.retain(|old| {
                !(old.identity_id == grant.identity_id && old.tier_id == grant.tier_id)
            });
            next.entitlements.push(grant.clone());
        }
        for incoming in &self.invites {
            if let Some(existing) = next
                .invites
                .iter_mut()
                .find(|invite| invite.id == incoming.id)
            {
                if existing.tier_id != incoming.tier_id
                    || existing.issuer != incoming.issuer
                    || existing.digest != incoming.digest
                    || existing.expires_at_ms != incoming.expires_at_ms
                {
                    return false;
                }
                existing.revoked |= incoming.revoked;
            } else {
                next.invites.push(incoming.clone());
            }
        }
        for tier in &mut next.tiers {
            tier.members = next
                .entitlements
                .iter()
                .filter(|grant| grant.tier_id == tier.id)
                .map(|grant| grant.identity_id.clone())
                .collect();
        }
        if !next.validate() {
            return false;
        }
        *state = next;
        true
    }
}

impl TierState {
    fn validate(&self) -> bool {
        self.tiers.len() <= MAX_TIERS
            && self.invites.len() <= MAX_INVITES
            && self.entitlements.iter().all(|e| {
                e.source != EntitlementSource::Paid
                    && self
                        .tiers
                        .iter()
                        .any(|t| t.id == e.tier_id && t.members.contains(&e.identity_id))
            })
            && self.tiers.iter().all(|t| {
                !t.name.trim().is_empty()
                    && t.name.chars().count() <= 60
                    && t.groups.len() <= 64
                    && t.resources.len() <= 1024
                    && t.members.len() <= 10000
                    && t.resources
                        .iter()
                        .all(|r| matches!(r.kind.as_str(), "page" | "event"))
            })
    }
    fn invitation(&self, token: &Token, now: i64) -> R<&Invite> {
        let invite = self
            .invites
            .iter()
            .find(|i| i.id == token.invite_id && i.tier_id == token.tier_id)
            .ok_or_else(forbidden)?;
        if invite.revoked
            || invite.expires_at_ms <= now
            || token.secret.len() != 43
            || invite.digest != token_digest(token)
        {
            return Err(forbidden());
        }
        if !self.tiers.iter().any(|t| t.id == token.tier_id) {
            return Err(forbidden());
        }
        Ok(invite)
    }
}
fn token_digest(token: &Token) -> String {
    sha256_hex(
        format!(
            "zoen-tier-invite\0{}\0{}\0{}\0{}",
            token.space, token.tier_id, token.invite_id, token.secret
        )
        .as_bytes(),
    )
}
fn decode_token(raw: &str) -> R<Token> {
    let raw = if let Some(query) = raw.strip_prefix("zoen://tier?") {
        query
            .split('&')
            .find_map(|field| field.strip_prefix("token="))
            .ok_or_else(|| invalid("This invitation is not valid."))?
    } else {
        raw
    };
    if raw.len() > 4096 {
        return Err(invalid("This invitation is not valid."));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| invalid("This invitation is not valid."))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("This invitation is not valid."))
}

impl Engine {
    fn tier_admin(&self, space: &str, who: &str) -> bool {
        self.state.spaces.get(space).is_some_and(|s| {
            s.members
                .iter()
                .any(|(m, r)| m == who && matches!(r, Role::Owner | Role::Admin))
        })
    }
    fn tier_state(&self, space: &str) -> R<TierState> {
        self.space_state(space)?;
        let mut result = TierState::default();
        let mut roles = BTreeMap::new();
        {
            for event in &self.living_events(space) {
                match &event.body {
                    EventBody::SpaceCreated { .. } => {
                        roles.insert(event.author.clone(), Role::Owner);
                    }
                    EventBody::MemberAdded { identity, role }
                        if roles
                            .get(&event.author)
                            .is_some_and(|r| matches!(r, Role::Owner | Role::Admin)) =>
                    {
                        roles.insert(identity.clone(), *role);
                    }
                    EventBody::MemberRemoved { identity }
                        if roles
                            .get(&event.author)
                            .is_some_and(|r| matches!(r, Role::Owner | Role::Admin)) =>
                    {
                        roles.remove(identity);
                    }
                    EventBody::MessagePosted { text, .. }
                        if roles
                            .get(&event.author)
                            .is_some_and(|r| matches!(r, Role::Owner | Role::Admin)) =>
                    {
                        if let Some(raw) = text.strip_prefix(TIER_META) {
                            if raw.len() <= 2_000_000 {
                                if let Ok(delta) = serde_json::from_str::<TierDelta>(raw) {
                                    delta.apply(&mut result);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(result)
    }
    fn tier_store(&mut self, space: &str, state: &TierState) -> R<()> {
        if !state.validate() {
            return Err(invalid("This membership update isn't valid."));
        }
        let me = self.me_id()?;
        if !self.tier_admin(space, &me) {
            return Err(forbidden());
        }
        let delta = TierDelta::between(&self.tier_state(space)?, state);
        let encoded =
            serde_json::to_string(&delta).map_err(|_| invalid("Couldn't save memberships."))?;
        if encoded.len() > 2_000_000 {
            return Err(invalid("This membership update is too large."));
        }
        let text = format!("{TIER_META}{encoded}");
        self.append(
            space,
            &me,
            EventBody::MessagePosted {
                message: new_id("msg"),
                text,
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )?;
        Ok(())
    }
    fn tier_claims(&self, space: &str, state: &TierState) -> Vec<(String, Token)> {
        let Ok(me) = self.me_id() else {
            return vec![];
        };
        if !self.tier_admin(space, &me) {
            return vec![];
        }
        let mut claims = BTreeMap::new();
        for id in self.state.spaces.keys() {
            if !self.state.spaces.get(id).is_some_and(|s| {
                s.kind == SpaceKind::Direct
                    && s.privacy == Privacy::EndToEnd
                    && s.members.iter().any(|(m, _)| m == &me)
            }) {
                continue;
            }
            for event in &self.living_events(id) {
                if let EventBody::MessagePosted { text, .. } = &event.body {
                    let Some(raw) = text.strip_prefix(TIER_CLAIM) else {
                        continue;
                    };
                    if raw.len() > 8192 {
                        continue;
                    }
                    let Ok(claim) = serde_json::from_str::<Claim>(raw) else {
                        continue;
                    };
                    let Ok(token) = decode_token(&claim.token) else {
                        continue;
                    };
                    if token.space != space || state.invitation(&token, now_ms()).is_err() {
                        continue;
                    }
                    if state
                        .entitlements
                        .iter()
                        .any(|e| e.identity_id == event.author && e.tier_id == token.tier_id)
                    {
                        continue;
                    }
                    claims.insert((event.author.clone(), token.invite_id.clone()), token);
                }
            }
        }
        claims
            .into_iter()
            .map(|((identity, _), token)| (identity, token))
            .collect()
    }
    pub(crate) fn living_tier_home(&self, space: &str) -> R<Value> {
        let state = self.tier_state(space)?;
        let me = self.me_id()?;
        let admin = self.tier_admin(space, &me);
        let claims: Vec<_> = self.tier_claims(space, &state).into_iter().map(|(identity,t)| json!({"identity_id":identity,"invite_id":t.invite_id,"tier_id":t.tier_id})).collect();
        let tiers: Vec<_> = state.tiers.iter().map(|t| {
            let access = self.is_member(space, &me) && self.is_member(&t.space, &me)
                && (admin || state.entitlements.iter().any(|e| e.tier_id == t.id && e.identity_id == me && e.source != EntitlementSource::Paid));
            json!({"id":t.id,"name":t.name,"space":if access { Some(&t.space) } else {None},"groups":if access { t.groups.clone() } else {vec![]},"resources":if access { serde_json::to_value(&t.resources).unwrap_or(json!([])) } else {json!([])},"members":t.members})
        }).collect();
        let invitations: Vec<_> = if admin {
            state.invites.iter().map(|invite| json!({"id":invite.id,"tier_id":invite.tier_id,"expires_at_ms":invite.expires_at_ms,"revoked":invite.revoked})).collect()
        } else {
            vec![]
        };
        Ok(
            json!({"tiers":tiers,"entitlements":state.entitlements,"tier_claims":claims,"invitations":invitations}),
        )
    }
    pub(crate) fn living_tier_hidden(&self, child: &str) -> bool {
        self.state
            .spaces
            .iter()
            .filter(|(_, s)| s.kind == SpaceKind::Community)
            .any(|(root, _)| {
                self.tier_state(root).is_ok_and(|s| {
                    s.tiers
                        .iter()
                        .any(|t| t.space == child || t.groups.iter().any(|g| g == child))
                })
            })
    }
    /// Used by every item/page/file read and write entry point, including search.
    pub(crate) fn living_tier_guard_resource(&self, item: &str) -> R<()> {
        let space = &self.item_state(item)?.space;
        // An absent account is an anonymous principal, not an error for ordinary
        // imported resources. Empty can never match a protected signed identity.
        self.living_tier_guard_space(space, self.me.as_deref().unwrap_or_default())
    }
    pub(crate) fn living_tier_guard_space(&self, space: &str, who: &str) -> R<()> {
        for (root, community) in &self.state.spaces {
            if community.kind != SpaceKind::Community {
                continue;
            }
            let state = self.tier_state(root)?;
            if let Some(tier) = state
                .tiers
                .iter()
                .find(|t| t.space == space || t.groups.iter().any(|g| g == space))
            {
                if !self.is_member(root, who) || !self.is_member(space, who) {
                    return Err(forbidden());
                }
                if self.tier_admin(root, who)
                    || state.entitlements.iter().any(|e| {
                        e.tier_id == tier.id
                            && e.identity_id == who
                            && e.source != EntitlementSource::Paid
                    })
                {
                    return Ok(());
                }
                return Err(forbidden());
            }
        }
        Ok(())
    }
    /// Cascades a community departure before its membership event is queued.
    /// Child removals can fail; the caller must propagate that failure, not publish
    /// a root departure that falsely promises future key revocation.
    pub(crate) fn living_tier_remove_from_all(&mut self, root: &str, who: &str) -> R<()> {
        if !self
            .state
            .spaces
            .get(root)
            .is_some_and(|s| s.kind == SpaceKind::Community)
        {
            return Ok(());
        }
        let mut state = self.tier_state(root)?;
        if state.tiers.is_empty() {
            return Ok(());
        }
        let me = self.me_id()?;
        let admin = self.tier_admin(root, &me);
        if !admin && me != who {
            return Err(forbidden());
        }
        for tier in &mut state.tiers {
            for group in std::iter::once(tier.space.clone()).chain(tier.groups.clone()) {
                if self.is_member(&group, who) {
                    if who == me {
                        self.leave_space(&group)?;
                    } else {
                        self.remove_member(&group, who)?;
                    }
                }
            }
            tier.members.retain(|m| m != who);
        }
        state.entitlements.retain(|e| e.identity_id != who);
        // A departing member cannot write an administrator snapshot. Root and
        // child membership checks already invalidate their stale entitlement.
        if admin {
            self.tier_store(root, &state)
        } else {
            Ok(())
        }
    }
    pub(crate) fn living_tier_demote_admin(&mut self, root: &str, who: &str) -> R<()> {
        if !self
            .state
            .spaces
            .get(root)
            .is_some_and(|s| s.kind == SpaceKind::Community)
        {
            return Ok(());
        }
        let state = self.tier_state(root)?;
        for tier in state.tiers {
            let entitled = state
                .entitlements
                .iter()
                .any(|e| e.identity_id == who && e.tier_id == tier.id)
                && tier.members.iter().any(|id| id == who);
            for group in std::iter::once(tier.space).chain(tier.groups) {
                if self.is_member(&group, who) {
                    if entitled {
                        // Preserve independently granted access, never the inherited role.
                        self.add_member(&group, who, Role::Member)?;
                    } else {
                        // remove_member emits the MLS removal commit as well as the event.
                        self.remove_member(&group, who)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn living_tier_promote_admin(&mut self, root: &str, who: &str) -> R<()> {
        if !self
            .state
            .spaces
            .get(root)
            .is_some_and(|s| s.kind == SpaceKind::Community)
        {
            return Ok(());
        }
        let state = self.tier_state(root)?;
        if state.tiers.is_empty() {
            return Ok(());
        }
        if !self.tier_admin(root, &self.me_id()?) || !self.tier_admin(root, who) {
            return Err(forbidden());
        }
        for tier in state.tiers {
            for group in std::iter::once(tier.space).chain(tier.groups) {
                let already_admin = self
                    .space_state(&group)?
                    .members
                    .iter()
                    .any(|(id, role)| id == who && matches!(role, Role::Owner | Role::Admin));
                if !already_admin {
                    self.add_member(&group, who, Role::Admin)?;
                }
            }
        }
        Ok(())
    }
    fn tier_grant(
        &mut self,
        root: &str,
        state: &mut TierState,
        tier_id: &str,
        who: &str,
        source: EntitlementSource,
    ) -> R<()> {
        if source == EntitlementSource::Paid {
            return Err(forbidden());
        }
        if !self.is_member(root, who) {
            return Err(invalid("Approve this person's Space membership first."));
        }
        let tier = state
            .tiers
            .iter_mut()
            .find(|t| t.id == tier_id)
            .ok_or_else(forbidden)?;
        let groups: Vec<_> = std::iter::once(tier.space.clone())
            .chain(tier.groups.clone())
            .collect();
        for group in groups {
            if self.space_state(&group)?.privacy != Privacy::EndToEnd {
                return Err(forbidden());
            }
            if !self.is_member(&group, who) {
                self.add_member(&group, who, Role::Member)?;
            }
        }
        if !tier.members.iter().any(|m| m == who) {
            tier.members.push(who.into());
        }
        state
            .entitlements
            .retain(|e| !(e.tier_id == tier_id && e.identity_id == who));
        state.entitlements.push(Entitlement {
            identity_id: who.into(),
            tier_id: tier_id.into(),
            source,
        });
        Ok(())
    }
    pub(crate) fn living_tier_command(&mut self, space: &str, command: &Value) -> R<Value> {
        let me = self.me_id()?;
        let action = string(command, "action")?;
        let mut state = self.tier_state(space)?;
        if action == "tier_redeem" {
            let raw = string(command, "token")?;
            let token = decode_token(raw)?;
            if token.space != space {
                return Err(forbidden());
            }
            let invite = state.invitation(&token, now_ms())?;
            if !self.tier_admin(space, &invite.issuer) {
                return Err(forbidden());
            }
            let dm = self.start_direct(&invite.issuer)?;
            let claim = serde_json::to_string(&Claim { token: raw.into() })
                .map_err(|_| invalid("Couldn't request membership."))?;
            self.append(
                &dm,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg"),
                    text: format!("{TIER_CLAIM}{claim}"),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                },
            )?;
            return Ok(json!({"pending":true}));
        }
        if !self.tier_admin(space, &me) {
            return Err(forbidden());
        }
        match action {
            "tier_create" => {
                let name = string(command, "name")?.trim();
                if name.is_empty()
                    || name.chars().count() > 60
                    || state.tiers.len() >= MAX_TIERS
                    || state
                        .tiers
                        .iter()
                        .any(|t| t.name.to_lowercase() == name.to_lowercase())
                {
                    return Err(invalid("Choose a different membership name."));
                }
                let partition =
                    self.create_synced_space(name, SpaceKind::Group, Privacy::EndToEnd, &[])?;
                let admins: Vec<_> = self
                    .space_state(space)?
                    .members
                    .iter()
                    .filter(|(who, role)| who != &me && matches!(role, Role::Owner | Role::Admin))
                    .map(|(who, _)| who.clone())
                    .collect();
                for admin in admins {
                    self.add_member(&partition, &admin, Role::Admin)?;
                }
                state.tiers.push(Tier {
                    id: new_id("tier"),
                    name: name.into(),
                    space: partition,
                    groups: vec![],
                    resources: vec![],
                    members: vec![],
                });
            }
            "tier_assign" => self.tier_grant(
                space,
                &mut state,
                string(command, "tier_id")?,
                string(command, "identity_id")?,
                EntitlementSource::Manual,
            )?,
            "tier_revoke" => {
                let id = string(command, "tier_id")?;
                let who = string(command, "identity_id")?;
                if self.tier_admin(space, who) {
                    return Err(invalid("Administrators keep access to manage this Space."));
                }
                let tier = state
                    .tiers
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(forbidden)?;
                for group in std::iter::once(tier.space.clone()).chain(tier.groups.clone()) {
                    if self.is_member(&group, who) {
                        self.remove_member(&group, who)?;
                    }
                }
                tier.members.retain(|m| m != who);
                state
                    .entitlements
                    .retain(|e| !(e.tier_id == id && e.identity_id == who));
            }
            "tier_invite" => {
                let tier_id = string(command, "tier_id")?;
                if !state.tiers.iter().any(|t| t.id == tier_id)
                    || state.invites.len() >= MAX_INVITES
                {
                    return Err(forbidden());
                }
                let expiry = command
                    .get("expires_at_ms")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| invalid("Choose an expiry date."))?;
                let now = now_ms();
                if expiry <= now || expiry.saturating_sub(now) > MAX_INVITE_AGE_MS {
                    return Err(invalid("Invitations can last up to 30 days."));
                }
                let mut random = [0u8; 32];
                getrandom::getrandom(&mut random)
                    .map_err(|_| invalid("Couldn't create an invitation."))?;
                let token = Token {
                    space: space.into(),
                    tier_id: tier_id.into(),
                    invite_id: new_id("invite"),
                    secret: URL_SAFE_NO_PAD.encode(random),
                };
                state.invites.push(Invite {
                    id: token.invite_id.clone(),
                    tier_id: tier_id.into(),
                    issuer: me,
                    digest: token_digest(&token),
                    expires_at_ms: expiry,
                    revoked: false,
                });
                self.tier_store(space, &state)?;
                let raw = URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&token)
                        .map_err(|_| invalid("Couldn't create an invitation."))?,
                );
                return Ok(
                    json!({"invite_url":format!("zoen://tier?space={space}&token={raw}"),"invite_id":token.invite_id,"expires_at_ms":expiry}),
                );
            }
            "tier_invite_revoke" => {
                let id = string(command, "invite_id")?;
                state
                    .invites
                    .iter_mut()
                    .find(|i| i.id == id)
                    .ok_or_else(forbidden)?
                    .revoked = true;
            }
            "tier_claim_approve" => {
                let identity = string(command, "identity_id")?;
                let invite_id = string(command, "invite_id")?;
                let token = self
                    .tier_claims(space, &state)
                    .into_iter()
                    .find(|(who, t)| who == identity && t.invite_id == invite_id)
                    .map(|(_, t)| t)
                    .ok_or_else(forbidden)?;
                let invite = state.invitation(&token, now_ms())?;
                if !self.tier_admin(space, &invite.issuer) {
                    return Err(forbidden());
                }
                self.tier_grant(
                    space,
                    &mut state,
                    &token.tier_id,
                    identity,
                    EntitlementSource::InviteLink,
                )?;
            }
            "tier_group_create" => {
                let id = string(command, "tier_id")?;
                let title = string(command, "title")?.trim();
                if title.is_empty() || title.chars().count() > 100 {
                    return Err(invalid("Choose a shorter conversation name."));
                }
                let tier = state
                    .tiers
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(forbidden)?;
                if tier.groups.len() >= 64 {
                    return Err(invalid("This membership has enough conversations already."));
                }
                let group = self.create_synced_space(
                    title,
                    SpaceKind::Group,
                    Privacy::EndToEnd,
                    &tier.members,
                )?;
                let admins: Vec<_> = self
                    .space_state(space)?
                    .members
                    .iter()
                    .filter(|(who, role)| who != &me && matches!(role, Role::Owner | Role::Admin))
                    .map(|(who, _)| who.clone())
                    .collect();
                for admin in admins {
                    self.add_member(&group, &admin, Role::Admin)?;
                }
                tier.groups.push(group);
            }
            "tier_bind" => {
                let id = string(command, "tier_id")?;
                let item_id = string(command, "item_id")?;
                let kind = string(command, "kind")?;
                let tier = state
                    .tiers
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(forbidden)?;
                let item = self.item_state(item_id)?;
                if item.space != tier.space
                    || self.space_state(&item.space)?.privacy != Privacy::EndToEnd
                {
                    return Err(invalid(
                        "Create this content inside the membership's private Space first.",
                    ));
                }
                if !matches!(
                    (kind, item.kind),
                    ("page", ItemKind::Page) | ("event", ItemKind::App)
                ) {
                    return Err(invalid("Choose a page or an event."));
                }
                if !tier.resources.iter().any(|r| r.id == item_id) {
                    tier.resources.push(Resource {
                        id: item_id.into(),
                        kind: kind.into(),
                    });
                }
            }
            _ => return Err(invalid("This action is not available.")),
        }
        self.tier_store(space, &state)?;
        self.living_tier_home(space)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> (TierState, Token) {
        let token = Token {
            space: "community".into(),
            tier_id: "pro".into(),
            invite_id: "invite".into(),
            secret: URL_SAFE_NO_PAD.encode([7u8; 32]),
        };
        let state = TierState {
            tiers: vec![Tier {
                id: "pro".into(),
                name: "Pro".into(),
                space: "encrypted".into(),
                groups: vec![],
                resources: vec![],
                members: vec![],
            }],
            entitlements: vec![],
            invites: vec![Invite {
                id: "invite".into(),
                tier_id: "pro".into(),
                issuer: "admin".into(),
                digest: token_digest(&token),
                expires_at_ms: 1000,
                revoked: false,
            }],
        };
        (state, token)
    }
    #[test]
    fn invitation_is_scoped_expiring_and_revocable() {
        let (mut state, mut token) = state();
        assert!(state.invitation(&token, 999).is_ok());
        assert!(state.invitation(&token, 1000).is_err());
        token.space = "another-community".into();
        assert!(state.invitation(&token, 999).is_err());
        token.space = "community".into();
        token.tier_id = "free".into();
        assert!(state.invitation(&token, 999).is_err());
        token.tier_id = "pro".into();
        state.invites[0].revoked = true;
        assert!(state.invitation(&token, 999).is_err());
    }
    #[test]
    fn paid_source_never_grants_v1_access() {
        let (mut state, _) = state();
        state.tiers[0].members.push("person".into());
        state.entitlements.push(Entitlement {
            identity_id: "person".into(),
            tier_id: "pro".into(),
            source: EntitlementSource::Paid,
        });
        assert!(!state.validate());
        state.entitlements[0].source = EntitlementSource::Manual;
        assert!(state.validate());
    }
    #[test]
    fn tier_policy_checks_actual_partition_membership_and_assignment() {
        let mut engine = Engine::open(":memory:").unwrap();
        crate::seed::seed(&mut engine).unwrap();
        let owner = engine.me_id().unwrap();
        let member = engine
            .signers
            .keys()
            .find(|id| *id != &owner)
            .unwrap()
            .clone();
        let root = new_id("sp");
        let partition = new_id("sp");
        engine
            .append(
                &root,
                &owner,
                EventBody::SpaceCreated {
                    title: "Community".into(),
                    kind: SpaceKind::Community,
                    privacy: Privacy::Closed,
                },
            )
            .unwrap();
        engine
            .append(
                &root,
                &owner,
                EventBody::MemberAdded {
                    identity: member.clone(),
                    role: Role::Member,
                },
            )
            .unwrap();
        engine
            .append(
                &partition,
                &owner,
                EventBody::SpaceCreated {
                    title: "Private".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            )
            .unwrap();
        let mut state = TierState {
            tiers: vec![Tier {
                id: "pro".into(),
                name: "Pro".into(),
                space: partition.clone(),
                groups: vec![],
                resources: vec![],
                members: vec![member.clone()],
            }],
            entitlements: vec![Entitlement {
                identity_id: member.clone(),
                tier_id: "pro".into(),
                source: EntitlementSource::Manual,
            }],
            invites: vec![],
        };
        engine.tier_store(&root, &state).unwrap();
        assert!(
            engine.living_tier_guard_space(&partition, &member).is_err(),
            "a badge without group membership cannot grant access"
        );
        engine
            .append(
                &partition,
                &owner,
                EventBody::MemberAdded {
                    identity: member.clone(),
                    role: Role::Member,
                },
            )
            .unwrap();
        assert!(engine.living_tier_guard_space(&partition, &member).is_ok());
        engine.add_member(&root, &member, Role::Admin).unwrap();
        engine.add_member(&root, &member, Role::Member).unwrap();
        assert!(
            engine
                .space_state(&partition)
                .unwrap()
                .members
                .iter()
                .any(|(id, role)| id == &member && *role == Role::Member),
            "demotion retains independent entitlement but removes child admin role"
        );
        state.entitlements.clear();
        state.tiers[0].members.clear();
        engine.tier_store(&root, &state).unwrap();
        assert!(
            engine.living_tier_guard_space(&partition, &member).is_err(),
            "cached membership alone does not grant an entitlement"
        );
        assert!(
            engine.living_tier_guard_space(&partition, &owner).is_ok(),
            "owners retain management access"
        );
        engine.add_member(&root, &member, Role::Admin).unwrap();
        engine.add_member(&root, &member, Role::Member).unwrap();
        assert!(
            !engine.is_member(&partition, &member),
            "demotion revokes inherited tier MLS membership without an entitlement"
        );
        engine.me = Some(member.clone());
        assert!(engine
            .living_tier_command(
                &root,
                &json!({"action":"tier_assign","tier_id":"pro","identity_id":owner})
            )
            .is_err());
        engine.living_tier_remove_from_all(&root, &member).unwrap();
        assert!(
            !engine.is_member(&partition, &member),
            "leaving removes stale child membership even after entitlement revocation"
        );
    }
    #[test]
    fn offline_admin_deltas_preserve_unrelated_grants_and_revocation() {
        let (base, _) = state();
        let mut first = base.clone();
        let mut second = base.clone();
        first.tiers[0].members.push("one".into());
        second.tiers[0].members.push("two".into());
        first.entitlements.push(Entitlement {
            identity_id: "one".into(),
            tier_id: "pro".into(),
            source: EntitlementSource::Manual,
        });
        second.entitlements.push(Entitlement {
            identity_id: "two".into(),
            tier_id: "pro".into(),
            source: EntitlementSource::Manual,
        });
        let first_delta = TierDelta::between(&base, &first);
        let second_delta = TierDelta::between(&base, &second);
        let mut replay = base.clone();
        assert!(first_delta.apply(&mut replay));
        assert!(second_delta.apply(&mut replay));
        assert_eq!(replay.entitlements.len(), 2);
        let mut revoked = first.clone();
        revoked.entitlements.clear();
        revoked.tiers[0].members.clear();
        assert!(TierDelta::between(&first, &revoked).apply(&mut replay));
        assert_eq!(replay.entitlements.len(), 1);
        assert_eq!(replay.entitlements[0].identity_id, "two");
        let mut revoked_invite = base.clone();
        revoked_invite.invites[0].revoked = true;
        assert!(TierDelta::between(&base, &revoked_invite).apply(&mut replay));
        let stale = TierDelta {
            tiers: vec![],
            grants: vec![],
            revokes: vec![],
            invites: base.invites.clone(),
        };
        assert!(stale.apply(&mut replay));
        assert!(
            replay.invites[0].revoked,
            "an old invitation cannot undo revocation"
        );
    }
    #[test]
    fn malformed_or_tampered_invitation_is_refused() {
        assert!(decode_token("not an invitation").is_err());
        let (state, mut token) = state();
        token.secret = URL_SAFE_NO_PAD.encode([8u8; 32]);
        assert!(state.invitation(&token, 900).is_err());
    }
}
