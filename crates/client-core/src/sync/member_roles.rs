//! Derived role reviews over the existing membership context and signed outbox.
use super::member_controls::{changed, role_name};
use super::{CoreError, Engine, Role, R};
use crate::{MemberActionState, MemberRoleOption, SpaceMemberContext};
use sha2::{Digest, Sha256};

pub(super) fn options(
    context: &SpaceMemberContext,
    actor: Role,
    target: Option<Role>,
    projected_actor: Option<Role>,
    projected_target: Option<Role>,
) -> R<Vec<MemberRoleOption>> {
    let available = match context.state {
        MemberActionState::Active => target.is_some(),
        MemberActionState::Removed => {
            target.is_none() && context.can_add && context.candidate.is_some()
        }
        _ => false,
    };
    if !available
        || context.member_id == context.identity_id
        || target == Some(Role::Owner)
        || projected_target != target
    {
        return Ok(Vec::new());
    }
    [Role::Member, Role::Reader, Role::Admin]
        .into_iter()
        .filter(|desired| {
            Some(*desired) != target
                && actor.can_assign(target, *desired)
                && projected_actor.is_some_and(|actor| actor.can_assign(projected_target, *desired))
        })
        .map(|desired| {
            let role = role_name(desired);
            let bytes = serde_json::to_vec(&(
                "zoen-member-role-review/1",
                &context.identity_id,
                &context.device_id,
                &context.space_id,
                &context.member_id,
                &context.revision,
                &role,
            ))
            .map_err(|_| CoreError::Storage {
                message: "could not review member role".into(),
            })?;
            Ok(MemberRoleOption {
                role,
                revision: format!("{:x}", Sha256::digest(bytes)),
            })
        })
        .collect()
}

impl Engine {
    /// Role and current account/device/member cuts are rechecked under one lock.
    /// Neither a queued projection nor an old exact response is a new assignment.
    pub(crate) fn set_member_role_reviewed(
        &mut self,
        space: &str,
        member: &str,
        identity: &str,
        device: &str,
        revision: &str,
        role: &str,
    ) -> R<SpaceMemberContext> {
        let current = self.space_member_context(space, member, identity, device)?;
        if !current
            .role_options
            .iter()
            .any(|choice| choice.role == role && choice.revision == revision)
        {
            return Err(changed());
        }
        let desired = match role {
            "member" => Role::Member,
            "reader" => Role::Reader,
            "admin" => Role::Admin,
            _ => return Err(changed()),
        };
        self.add_member(space, member, desired)?;
        self.space_member_context(space, member, identity, device)
    }
}
