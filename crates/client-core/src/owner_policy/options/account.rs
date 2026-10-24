//! Explicit private account management, independent of any image Request.
use super::*;
use types::owner_policy_context::OwnerPolicyInquiryPurpose;

impl Engine {
    pub(crate) fn start_owner_policy_direct(
        &mut self,
        identity: &str,
        device: &str,
        agent: &str,
    ) -> R<String> {
        self.native_surface_owner(identity, device)?;
        let account = self.account().ok_or_else(denied)?;
        let owner = self.identities.get(identity).ok_or_else(denied)?;
        let target = self.identities.get(agent).ok_or_else(denied)?;
        if !account.registered
            || owner.kind != IdentityKind::Person
            || target.kind != IdentityKind::Agent
            || target.owner.as_deref() != Some(identity)
            || !event_log::agent_owner::profile_authorized(target)
        {
            return Err(denied());
        }
        let space = self.start_direct(agent)?;
        let actual = self.space_state(&space)?;
        if actual.kind != SpaceKind::Direct
            || actual.privacy != Privacy::EndToEnd
            || actual.members.len() != 2
            || !self.is_member(&space, identity)
            || !self.is_member(&space, agent)
        {
            return Err(denied());
        }
        Ok(space)
    }

    pub(crate) fn request_account_policy_options(
        &mut self,
        scope: &OwnerPolicyScope,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.owner_policy_private_conversation(scope, false)?;
        self.queue_owner_policy_inquiry(
            scope,
            OwnerPolicyInquiry {
                id: types::new_id("account_policy_inquiry"),
                owner: scope.expected_identity_id.clone(),
                agent: scope.agent_id.clone(),
                space: scope.space_id.clone(),
                request: String::new(),
                content_hash: String::new(),
                price_digest: String::new(),
                purpose: Some(OwnerPolicyInquiryPurpose::AccountBudget),
                at_ms: crate::engine::now_ms(),
            },
        )
    }

    pub(crate) fn find_account_policy_options(
        &self,
        scope: &OwnerPolicyScope,
    ) -> R<Option<OwnerPolicyOptionsStatus>> {
        self.find_policy_inquiry(
            scope,
            Some(OwnerPolicyInquiryPurpose::AccountBudget),
            "",
            "",
        )
    }
}
impl Client {
    pub fn start_owner_policy_direct(
        &self,
        identity: String,
        device: String,
        agent: String,
    ) -> R<String> {
        self.lock()
            .start_owner_policy_direct(&identity, &device, &agent)
    }

    pub fn request_account_policy_options(
        &self,
        scope: OwnerPolicyScope,
    ) -> R<OwnerPolicyOptionsStatus> {
        self.lock().request_account_policy_options(&scope)
    }
    pub fn find_account_policy_options(
        &self,
        scope: OwnerPolicyScope,
    ) -> R<Option<OwnerPolicyOptionsStatus>> {
        self.lock().find_account_policy_options(&scope)
    }
}
