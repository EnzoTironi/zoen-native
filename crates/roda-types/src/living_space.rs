//! Opt-in living Space domain experiment. Not wired to production event dispatch or FFI.
//!
//! A Community remains the parent. Home references an existing Page item; conversations
//! use separate EndToEnd Group spaces (and thus separate MLS groups, whose IDs are space
//! IDs). Joining the parent does not grant membership or keys in any child group.
use crate::{IdentityId, ItemId, Privacy, Role, SpaceId, SpaceKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LivingSpaceExperiment {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LivingSpaceError {
    Disabled,
    NotCommunity,
    Forbidden,
    InvalidReference,
    AlreadyMember,
    NoPendingRequest,
    InvalidChild,
}

/// UI-safe projection: deliberately contains no encrypted group identifiers or roster.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct LivingSpaceHome {
    pub space: SpaceId,
    pub page: Option<ItemId>,
    pub announcement_items: Vec<ItemId>,
    pub can_manage: bool,
}

/// Local domain state only. A trusted event adapter must provide authenticated actors,
/// persist accepted mutations, and validate referenced items against the parent log.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LivingSpace {
    space: SpaceId,
    members: BTreeMap<IdentityId, Role>,
    home: Option<ItemId>,
    announcements: Vec<ItemId>,
    child_groups: BTreeSet<SpaceId>,
    pending: BTreeSet<IdentityId>,
}

impl LivingSpaceExperiment {
    pub fn create(
        self,
        space: SpaceId,
        kind: SpaceKind,
        owner: IdentityId,
    ) -> Result<LivingSpace, LivingSpaceError> {
        if !self.enabled {
            return Err(LivingSpaceError::Disabled);
        }
        if kind != SpaceKind::Community {
            return Err(LivingSpaceError::NotCommunity);
        }
        if space.trim().is_empty() || owner.trim().is_empty() {
            return Err(LivingSpaceError::InvalidReference);
        }
        Ok(LivingSpace {
            space,
            members: BTreeMap::from([(owner, Role::Owner)]),
            home: None,
            announcements: Vec::new(),
            child_groups: BTreeSet::new(),
            pending: BTreeSet::new(),
        })
    }
}

impl LivingSpace {
    /// Replace cached roles with the authenticated parent log projection on every command.
    pub fn reconcile_members(&mut self, members: impl IntoIterator<Item = (IdentityId, Role)>) {
        self.members = members.into_iter().collect();
    }

    pub fn is_announcement(&self, item: &str) -> bool {
        self.announcements.iter().any(|id| id == item)
    }

    pub fn is_child(&self, space: &str) -> bool {
        self.child_groups.contains(space)
    }

    fn require_admin(&self, actor: &str) -> Result<(), LivingSpaceError> {
        match self.members.get(actor) {
            Some(Role::Owner | Role::Admin) => Ok(()),
            _ => Err(LivingSpaceError::Forbidden),
        }
    }

    /// Call only after verifying `page` is a Page item in this Community's log.
    pub fn set_home(&mut self, actor: &str, page: ItemId) -> Result<(), LivingSpaceError> {
        self.require_admin(actor)?;
        if page.trim().is_empty() {
            return Err(LivingSpaceError::InvalidReference);
        }
        self.home = Some(page);
        Ok(())
    }

    /// Publishing and editing announcements must both pass this same admin gate.
    /// The adapter must verify the item belongs to this Community before calling.
    pub fn publish_announcement(
        &mut self,
        actor: &str,
        item: ItemId,
    ) -> Result<(), LivingSpaceError> {
        self.require_admin(actor)?;
        if item.trim().is_empty() {
            return Err(LivingSpaceError::InvalidReference);
        }
        if !self.announcements.contains(&item) {
            self.announcements.push(item);
        }
        Ok(())
    }

    pub fn request_join(&mut self, applicant: IdentityId) -> Result<(), LivingSpaceError> {
        if applicant.trim().is_empty() {
            return Err(LivingSpaceError::InvalidReference);
        }
        if self.members.contains_key(&applicant) {
            return Err(LivingSpaceError::AlreadyMember);
        }
        self.pending.insert(applicant);
        Ok(())
    }

    pub fn pending_requests(&self, actor: &str) -> Result<Vec<IdentityId>, LivingSpaceError> {
        self.require_admin(actor)?;
        Ok(self.pending.iter().cloned().collect())
    }

    /// Approval grants only parent Member status, never Admin or child MLS membership.
    pub fn resolve_join(
        &mut self,
        actor: &str,
        applicant: &str,
        approve: bool,
    ) -> Result<(), LivingSpaceError> {
        self.require_admin(actor)?;
        if !self.pending.remove(applicant) {
            return Err(LivingSpaceError::NoPendingRequest);
        }
        if approve {
            self.members.insert(applicant.to_owned(), Role::Member);
        }
        Ok(())
    }

    /// Owner-only role assignment; the owner cannot demote itself or create another owner.
    pub fn set_role(
        &mut self,
        actor: &str,
        member: &str,
        role: Role,
    ) -> Result<(), LivingSpaceError> {
        if self.members.get(actor) != Some(&Role::Owner)
            || self.members.get(member) == Some(&Role::Owner)
            || !self.members.contains_key(member)
            || role == Role::Owner
        {
            return Err(LivingSpaceError::Forbidden);
        }
        self.members.insert(member.to_owned(), role);
        Ok(())
    }

    /// Internal mapping only. The adapter must resolve kind/privacy from trusted state;
    /// this registers an already-created MLS group, it does not create or share keys.
    pub fn register_child_group(
        &mut self,
        actor: &str,
        child: SpaceId,
        kind: SpaceKind,
        privacy: Privacy,
    ) -> Result<(), LivingSpaceError> {
        self.require_admin(actor)?;
        if child.trim().is_empty()
            || child == self.space
            || kind != SpaceKind::Group
            || privacy != Privacy::EndToEnd
        {
            return Err(LivingSpaceError::InvalidChild);
        }
        self.child_groups.insert(child);
        Ok(())
    }

    pub fn home(&self, actor: &str) -> Result<LivingSpaceHome, LivingSpaceError> {
        if !self.members.contains_key(actor) {
            return Err(LivingSpaceError::Forbidden);
        }
        Ok(LivingSpaceHome {
            space: self.space.clone(),
            page: self.home.clone(),
            announcement_items: self.announcements.clone(),
            can_manage: self.require_admin(actor).is_ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn space() -> LivingSpace {
        LivingSpaceExperiment { enabled: true }
            .create("community".into(), SpaceKind::Community, "owner".into())
            .unwrap()
    }
    fn join(s: &mut LivingSpace, who: &str) {
        s.request_join(who.into()).unwrap();
        s.resolve_join("owner", who, true).unwrap();
    }
    #[test]
    fn default_off_and_community_only() {
        assert_eq!(
            LivingSpaceExperiment::default()
                .create("s".into(), SpaceKind::Community, "o".into())
                .unwrap_err(),
            LivingSpaceError::Disabled
        );
        assert_eq!(
            LivingSpaceExperiment { enabled: true }
                .create("s".into(), SpaceKind::Group, "o".into())
                .unwrap_err(),
            LivingSpaceError::NotCommunity
        );
    }
    #[test]
    fn approval_is_admin_only_and_parent_only() {
        let mut s = space();
        join(&mut s, "member");
        s.request_join("new".into()).unwrap();
        assert_eq!(
            s.resolve_join("member", "new", true),
            Err(LivingSpaceError::Forbidden)
        );
        assert_eq!(
            s.pending_requests("member"),
            Err(LivingSpaceError::Forbidden)
        );
        assert_eq!(s.home("new"), Err(LivingSpaceError::Forbidden));
        s.resolve_join("owner", "new", true).unwrap();
        assert!(!s.home("new").unwrap().can_manage);
        assert!(s.child_groups.is_empty());
        assert_eq!(
            s.resolve_join("owner", "new", true),
            Err(LivingSpaceError::NoPendingRequest)
        );
    }
    #[test]
    fn home_and_announcements_require_admin_even_for_reader() {
        let mut s = space();
        join(&mut s, "member");
        for role in [Role::Member, Role::Reader] {
            s.set_role("owner", "member", role).unwrap();
            assert_eq!(
                s.set_home("member", "page".into()),
                Err(LivingSpaceError::Forbidden)
            );
            assert_eq!(
                s.publish_announcement("member", "item".into()),
                Err(LivingSpaceError::Forbidden)
            );
        }
        s.set_role("owner", "member", Role::Admin).unwrap();
        s.set_home("member", "page".into()).unwrap();
        s.publish_announcement("member", "item".into()).unwrap();
        s.publish_announcement("member", "item".into()).unwrap();
        assert_eq!(s.home("member").unwrap().announcement_items, vec!["item"]);
    }
    #[test]
    fn encrypted_children_do_not_leak_into_home() {
        let mut s = space();
        let before = s.home("owner").unwrap();
        assert_eq!(
            s.register_child_group("owner", "child".into(), SpaceKind::Group, Privacy::Closed),
            Err(LivingSpaceError::InvalidChild)
        );
        assert_eq!(
            s.register_child_group(
                "owner",
                "community".into(),
                SpaceKind::Group,
                Privacy::EndToEnd
            ),
            Err(LivingSpaceError::InvalidChild)
        );
        s.register_child_group("owner", "child".into(), SpaceKind::Group, Privacy::EndToEnd)
            .unwrap();
        assert_eq!(s.home("owner").unwrap(), before);
        join(&mut s, "member");
        assert_eq!(
            s.register_child_group(
                "member",
                "other".into(),
                SpaceKind::Group,
                Privacy::EndToEnd
            ),
            Err(LivingSpaceError::Forbidden)
        );
    }
    #[test]
    fn rejection_and_role_escalation_are_safe() {
        let mut s = space();
        s.request_join("no".into()).unwrap();
        s.resolve_join("owner", "no", false).unwrap();
        assert_eq!(s.home("no"), Err(LivingSpaceError::Forbidden));
        join(&mut s, "member");
        assert_eq!(
            s.set_role("member", "member", Role::Admin),
            Err(LivingSpaceError::Forbidden)
        );
        assert_eq!(
            s.set_role("owner", "owner", Role::Member),
            Err(LivingSpaceError::Forbidden)
        );
        assert_eq!(
            s.set_role("owner", "member", Role::Owner),
            Err(LivingSpaceError::Forbidden)
        );
    }
}
