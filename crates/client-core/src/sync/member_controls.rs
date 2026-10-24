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
        let can_remove = state == MemberActionState::Active
            && member != identity
            && super::spaces::may_remove_member(Some(actor_role), target_role)
            && super::spaces::may_remove_member(projected(identity), projected(member));
        // Only membership cuts participate, so unrelated chat messages do not
        // invalidate a review; remove/rejoin and role replacement always do.
        let cut = |who: &str| {
            self.logs
                .get(space)
                .and_then(|log| {
                    log.events().iter().rev().find(|event| match &event.body {
                        EventBody::SpaceCreated { .. } => event.author == who,
                        EventBody::MemberAdded { identity, .. }
                        | EventBody::MemberRemoved { identity } => identity == who,
                        _ => false,
                    })
                })
                .map(|event| event.hash.as_str())
        };
        let review = serde_json::to_vec(&(
            space,
            &room.title,
            identity,
            device,
            member,
            actor_role,
            target_role,
            cut(identity),
            cut(member),
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
