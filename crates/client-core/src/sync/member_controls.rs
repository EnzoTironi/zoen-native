//! Review and removal use the existing log/outbox, never a second membership store.
use super::*;
use crate::{MemberActionState, SpaceMemberContext};
use sha2::{Digest, Sha256};

fn changed() -> CoreError {
    CoreError::Forbidden {
        reason: t(
            "o participante ou o acesso à conversa mudou",
            "the member or conversation access changed",
        ),
    }
}
fn role_name(role: Role) -> String {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Member => "member",
        Role::Reader => "reader",
    }
    .into()
}
impl Engine {
    /// Confirmed membership plus pending intent are reported separately. The
    /// optimistic disappearance of a person is never a successful removal receipt.
    pub(crate) fn space_member_context(
        &self,
        space: &str,
        member: &str,
        identity: &str,
        device: &str,
    ) -> R<SpaceMemberContext> {
        self.native_surface_owner(identity, device)?;
        if space.is_empty() || space.len() > 256 || member.is_empty() || member.len() > 256 {
            return Err(changed());
        }
        let room = self.space_state(space)?;
        if !matches!(room.kind, SpaceKind::Group | SpaceKind::Community)
            || room.integrity_error.is_some()
            || !self.is_member(space, identity)
        {
            return Err(changed());
        }
        let confirmed = self.receipt_members(space)?;
        let role = |who: &str| {
            confirmed
                .iter()
                .find(|(id, _)| id == who)
                .map(|(_, role)| *role)
        };
        let actor_role = role(identity).ok_or_else(changed)?;
        let target_role = role(member);
        let projected = |who: &str| {
            room.members
                .iter()
                .find(|(id, _)| id == who)
                .map(|(_, role)| *role)
        };
        // Only membership cuts participate, so unrelated chat messages do not
        // invalidate a review; remove/rejoin and role replacement always do.
        let cut = |who: &str| {
            self.logs.get(space).and_then(|log| {
                log.events().iter().rev().find(|event| match &event.body {
                    EventBody::SpaceCreated { .. } => event.author == who,
                    EventBody::MemberAdded { identity, .. }
                    | EventBody::MemberRemoved { identity } => identity == who,
                    _ => false,
                })
            })
        };
        let actor_cut = cut(identity);
        let member_cut = cut(member);
        let pending = self.store.outbox()?;
        let mut updating = false;
        let mut removing = false;
        for row in pending
            .iter()
            .filter(|row| !row.failed && row.event.space == space)
        {
            // A cleanup-delayed confirmed row is not still pending.
            if self.logs.get(space).is_some_and(|log| {
                log.events()
                    .iter()
                    .any(|event| event.client_id == row.event.client_id)
            }) {
                continue;
            }
            match &row.event.body {
                EventBody::SpaceCreated { .. } | EventBody::MemberAdded { .. } => updating = true,
                EventBody::MemberRemoved { identity: target } => {
                    updating = true;
                    removing |= target == member;
                }
                _ => {}
            }
        }
        let state = if removing && target_role.is_some() {
            MemberActionState::Removing
        } else if updating {
            MemberActionState::Updating
        } else if target_role.is_some() {
            MemberActionState::Active
        } else {
            MemberActionState::Removed
        };
        let person =
            (target_role.is_some() || projected(member).is_some()).then(|| self.persona(member));
        // Preserve AAC633: confirmed roles plus the real roster, for humans
        // and Readers too. Owner proof and write authority are separate facts.
        let delivery_ready = target_role.is_some()
            && (!self.is_e2e(space)
                || self
                    .mls_status(space)
                    .is_some_and(|(_, _, members)| members.iter().any(|id| id == member)));
        let can_manage_members = matches!(actor_role, Role::Owner | Role::Admin)
            && matches!(projected(identity), Some(Role::Owner | Role::Admin));
        let candidate = (target_role.is_none() && self.identities.contains_key(member))
            .then(|| self.persona(member));
        let can_add = can_manage_members
            && state == MemberActionState::Removed
            && projected(member).is_none()
            && member != identity
            && candidate.is_some();
        let addition_failure = if state == MemberActionState::Removed {
            pending.iter().rev().find(|row| {
                let event = &row.event;
                if !row.failed || event.space != space || event.author != identity
                    || event.device.as_deref() != Some(device)
                    || !matches!(&event.body, EventBody::MemberAdded { identity: target, role: Role::Member } if target == member)
                { return false; }
                let (Some(log), Some(seen), Some(actor)) =
                    (self.logs.get(space), event.seen.as_ref(), actor_cut)
                else { return false; };
                event_log::verify_author(event).is_ok()
                    && !log.events().iter().any(|e| e.client_id == event.client_id)
                    && log.events().iter().any(|e| e.seq == seen.seq && e.hash == seen.hash)
                    && actor.seq <= seen.seq
                    && member_cut.is_none_or(|target| target.seq <= seen.seq)
            }).map(|row| row.event.client_id.as_str())
        } else {
            None
        };
        let can_remove = state == MemberActionState::Active
            && member != identity
            && super::spaces::may_remove_member(Some(actor_role), target_role)
            && super::spaces::may_remove_member(projected(identity), projected(member));
        // Failed outbox rows are existing durable dispositions, not new authority.
        // Only this exact native writer, target and still-applicable signed causal
        // membership cut can surface a refusal. Rejoins/role ABA retire old failures.
        let failure = if state == MemberActionState::Active {
            pending.iter().rev().find(|row| {
                let event = &row.event;
                if !row.failed || event.space != space || event.author != identity
                    || event.device.as_deref() != Some(device)
                    || !matches!(&event.body, EventBody::MemberRemoved { identity: target } if target == member)
                { return false; }
                let (Some(log), Some(seen), Some(actor), Some(target)) =
                    (self.logs.get(space), event.seen.as_ref(), actor_cut, member_cut)
                else { return false; };
                event_log::verify_author(event).is_ok()
                    && !log.events().iter().any(|e| e.client_id == event.client_id)
                    && log.events().iter().any(|e| e.seq == seen.seq && e.hash == seen.hash)
                    && actor.seq <= seen.seq && target.seq <= seen.seq
            }).map(|row| row.event.client_id.as_str())
        } else {
            None
        };
        let owned_agent = self.verified_owned_agent(member, identity);
        let review = serde_json::to_vec(&(
            space,
            &room.title,
            identity,
            device,
            member,
            actor_role,
            target_role,
            actor_cut.map(|event| &event.hash),
            member_cut.map(|event| &event.hash),
            failure,
            addition_failure,
            delivery_ready,
            owned_agent,
            candidate
                .as_ref()
                .map(|p| (&p.name, &p.handle, &p.kind, &p.owner_id)),
            state,
            person.as_ref().map(|person| (&person.name, &person.handle)),
        ))
        .map_err(|_| CoreError::Storage {
            message: t(
                "não foi possível verificar o participante",
                "could not verify the member",
            ),
        })?;
        Ok(SpaceMemberContext {
            identity_id: identity.into(),
            device_id: device.into(),
            space_id: space.into(),
            space_title: room.title.clone(),
            member_id: member.into(),
            member: person,
            role: target_role.map(role_name),
            state,
            can_remove,
            can_manage_members,
            can_add,
            owned_agent,
            delivery_ready,
            candidate,
            addition_failed: addition_failure.is_some(),
            removal_failed: failure.is_some(),
            revision: format!("{:x}", Sha256::digest(review)),
        })
    }

    /// A single Engine guard spans current device/permission/review checks and
    /// the normal signed MemberRemoved enqueue. No automatic mutation retry.
    pub(crate) fn remove_member_reviewed(
        &mut self,
        space: &str,
        member: &str,
        identity: &str,
        device: &str,
        revision: &str,
    ) -> R<SpaceMemberContext> {
        let current = self.space_member_context(space, member, identity, device)?;
        if !current.can_remove || current.revision != revision {
            return Err(changed());
        }
        self.remove_member(space, member)?;
        self.space_member_context(space, member, identity, device)
    }
}

impl Engine {
    /// Current native actor and membership cut remain guarded through the normal enqueue.
    pub(crate) fn add_member_reviewed(
        &mut self,
        space: &str,
        member: &str,
        identity: &str,
        device: &str,
        revision: &str,
    ) -> R<SpaceMemberContext> {
        let current = self.space_member_context(space, member, identity, device)?;
        if !current.can_add || current.revision != revision {
            return Err(changed());
        }
        self.add_member(space, member, Role::Member)?;
        self.space_member_context(space, member, identity, device)
    }
    pub(crate) fn member_candidates(
        &self,
        space: &str,
        query: &str,
        identity: &str,
        device: &str,
    ) -> R<Vec<SpaceMemberContext>> {
        let actor = self.space_member_context(space, identity, identity, device)?;
        if !actor.can_manage_members {
            return Err(changed());
        }
        let query = query.trim().trim_start_matches('@').to_lowercase();
        self.personas()
            .iter()
            .filter(|p| {
                p.id != identity
                    && (query.is_empty()
                        || p.name.to_lowercase().contains(&query)
                        || p.handle.to_lowercase().contains(&query))
            })
            .take(100)
            .map(|p| self.space_member_context(space, &p.id, identity, device))
            .collect()
    }
}
