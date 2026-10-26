//! The existing scoped picker can read verified owned candidates in any writer
//! conversation. It never makes a Direct conversation addable or adds a member.
use super::*;
use crate::{MemberActionState, SpaceMemberContext};
impl Engine {
    pub(crate) fn verified_owned_agent(&self, id: &str, owner: &str) -> bool {
        self.identities.get(id).is_some_and(|profile| {
            profile.kind == types::IdentityKind::Agent
                && profile.owner.as_deref() == Some(owner)
                && event_log::agent_owner::profile_authorized(profile)
        })
    }
    pub(crate) fn member_delivery_ready(&self, space: &str, id: &str) -> bool {
        self.receipt_members(space)
            .is_ok_and(|members| members.iter().any(|(who, _)| who == id))
            && (!self.is_e2e(space)
                || self
                    .mls_status(space)
                    .is_some_and(|(_, _, readers)| readers.contains(&id.to_owned())))
    }
    pub(crate) fn member_candidates_mode(
        &self,
        space: &str,
        query: &str,
        identity: &str,
        device: &str,
        owned_only: bool,
    ) -> R<Vec<SpaceMemberContext>> {
        if !owned_only {
            return self.member_candidates(space, query, identity, device);
        }
        self.native_surface_owner(identity, device)?;
        if !self.state.writer(space, identity) || self.checked_integrity_error(space)?.is_some() {
            return Err(changed());
        }
        let room = self.space_state(space)?;
        let query = query.trim().trim_start_matches('@').to_lowercase();
        let ready: std::collections::HashSet<_> = self
            .mls_status(space)
            .map(|(_, _, ids)| ids.into_iter().collect())
            .unwrap_or_default();
        let mut candidates: Vec<_> = self
            .identity_order
            .iter()
            .filter(|id| self.verified_owned_agent(id, identity))
            .collect();
        // Preserve ANY-ready admission before the display bound. Hints are never
        // authority and do not turn an absent candidate into a member.
        candidates.sort_by_key(|id| {
            (
                !(self.state.writer(space, id)
                    && (!self.is_e2e(space) || ready.contains(id.as_str()))),
                !self.is_member(space, id),
            )
        });
        candidates
            .into_iter()
            .filter(|id| {
                let p = self.persona(id);
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.handle.to_lowercase().contains(&query)
            })
            .take(100)
            .map(|id| {
                if matches!(room.kind, SpaceKind::Group | SpaceKind::Community) {
                    return self.space_member_context(space, id, identity, device);
                }
                let members = self.receipt_members(space)?;
                let role = members
                    .iter()
                    .find(|(who, _)| who == id)
                    .map(|(_, role)| *role);
                let person = self.persona(id);
                Ok(SpaceMemberContext {
                    identity_id: identity.into(),
                    device_id: device.into(),
                    space_id: space.into(),
                    space_title: room.title.clone(),
                    member_id: id.clone(),
                    member: role.map(|_| person.clone()),
                    role: role.map(|r| {
                        match r {
                            types::Role::Owner => "owner",
                            types::Role::Admin => "admin",
                            types::Role::Member => "member",
                            types::Role::Reader => "reader",
                        }
                        .into()
                    }),
                    state: if role.is_some() {
                        MemberActionState::Active
                    } else {
                        MemberActionState::Removed
                    },
                    can_remove: false,
                    can_manage_members: false,
                    can_add: false,
                    owned_agent: true,
                    delivery_ready: self.member_delivery_ready(space, id),
                    candidate: role.is_none().then_some(person),
                    addition_failed: false,
                    removal_failed: false,
                    role_options: Vec::new(),
                    pending_role: None,
                    role_change_failed: false,
                    revision: event_log::content_hash(&(
                        space,
                        identity,
                        device,
                        id,
                        role,
                        self.logs.get(space).and_then(event_log::SpaceLog::head),
                    ))?,
                })
            })
            .collect()
    }
}
fn changed() -> CoreError {
    CoreError::Forbidden {
        reason: crate::i18n::t("O acesso à conversa mudou.", "Conversation access changed."),
    }
}
