//! Explicit owner-wide spending policy, sent through the existing encrypted DM.
//! A queued submission never proves installation in the Runtime authority.
mod control;
mod image_context;
mod options;
mod publication;
use crate::{
    engine::{Engine, R},
    Client, CoreError,
};
pub use control::OwnerPolicySubmission;
pub use options::{OwnerPolicyContext, OwnerPolicyInquiryControl, OwnerPolicyOptionsStatus};
use serde::{Deserialize, Serialize};
use types::{owner_budget::OwnerPeriodPolicy, EventBody, IdentityKind, Privacy, SpaceKind};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct OwnerPolicyScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub agent_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct OwnerPolicyReview {
    pub scope: OwnerPolicyScope,
    pub canonical_policy_json: String,
    pub policy_digest: String,
    pub confirmation_digest: String,
    pub currency: String,
    pub scale: u8,
    pub year: i32,
    pub month: u8,
    pub version: u32,
    pub limit_units: String,
    pub max_attempt_units: String,
    pub expires_at_ms: String,
    pub enabled: bool,
    pub allowed_profiles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct OwnerPolicyStatus {
    pub source: String,
    pub policy_digest: String,
    /// pending, installed or refused, from the original authenticated control.
    pub state: String,
}

fn denied() -> CoreError {
    CoreError::Forbidden {
        reason: "Owner policy review or conversation access changed.".into(),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid explicit owner spending policy.".into(),
    }
}
fn decode(json: &str) -> R<OwnerPeriodPolicy> {
    if json.len() > 4096 {
        return Err(invalid());
    }
    let policy: OwnerPeriodPolicy = serde_json::from_str(json).map_err(|_| invalid())?;
    if !policy.valid_shape() || policy.expires_at_ms <= crate::engine::now_ms() {
        return Err(invalid());
    }
    Ok(policy)
}

impl Engine {
    fn owner_policy_conversation(&self, scope: &OwnerPolicyScope) -> R<()> {
        self.owner_policy_private_conversation(scope, true)
    }
    fn owner_policy_private_conversation(&self, scope: &OwnerPolicyScope, ready: bool) -> R<()> {
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        let account = self.account().ok_or_else(denied)?;
        let owner = self
            .identities
            .get(&scope.expected_identity_id)
            .ok_or_else(denied)?;
        let agent = self.identities.get(&scope.agent_id).ok_or_else(denied)?;
        let space = self.space_state(&scope.space_id)?;
        if !account.registered
            || owner.kind != IdentityKind::Person
            || agent.kind != IdentityKind::Agent
            || agent.owner.as_deref() != Some(owner.id.as_str())
            || !event_log::agent_owner::profile_authorized(agent)
            || space.kind != SpaceKind::Direct
            || space.privacy != Privacy::EndToEnd
            || space.integrity_error.is_some()
            || space.members.len() != 2
            || !self.is_member(&scope.space_id, &owner.id)
            || !self.is_member(&scope.space_id, &agent.id)
            || !self.state.writer(&scope.space_id, &owner.id)
            || (ready && self.ready_epoch(&scope.space_id).is_none())
        {
            return Err(denied());
        }
        Ok(())
    }

    pub(crate) fn review_owner_policy(
        &self,
        scope: &OwnerPolicyScope,
        json: &str,
    ) -> R<OwnerPolicyReview> {
        self.owner_policy_conversation(scope)?;
        let policy = decode(json)?;
        if policy.owner != scope.expected_identity_id {
            return Err(denied());
        }
        let canonical = serde_json::to_string(&policy).map_err(|_| invalid())?;
        let policy_digest = event_log::owner_budget::digest(&policy).ok_or_else(invalid)?;
        let bytes = serde_json::to_vec(&("zoen-owner-policy-review/1", scope, &policy_digest))
            .map_err(|_| invalid())?;
        Ok(OwnerPolicyReview {
            scope: scope.clone(),
            canonical_policy_json: canonical,
            policy_digest,
            confirmation_digest: event_log::sha256_hex(&bytes),
            currency: policy.currency,
            scale: policy.scale,
            year: policy.year,
            month: policy.month,
            version: policy.version,
            limit_units: policy.limit_units.to_string(),
            max_attempt_units: policy.max_attempt_units.to_string(),
            expires_at_ms: policy.expires_at_ms.to_string(),
            enabled: policy.enabled,
            allowed_profiles: policy.allowed_profiles,
        })
    }

    pub(crate) fn find_owner_policy_submission(
        &self,
        scope: &OwnerPolicyScope,
        policy_digest: &str,
    ) -> R<Option<OwnerPolicyStatus>> {
        self.owner_policy_conversation(scope)?;
        if !types::owner_budget::digest_id(policy_digest) {
            return Err(invalid());
        }
        let pending = self.store.outbox()?;
        let original = pending.iter().map(|entry| &entry.event)
            .chain(self.logs.get(&scope.space_id).into_iter().flat_map(|log| log.events()))
            .find(|event| event.space == scope.space_id && event.author == scope.expected_identity_id
                && matches!(&event.body, EventBody::OwnerPolicySubmitted { agent, signed }
                    if agent == &scope.agent_id && signed.policy.owner == scope.expected_identity_id
                    && event.device.as_deref() == Some(signed.device.as_str())
                    && event.cert.as_deref() == Some(signed.cert.as_str())
                    && event_log::owner_budget::verify(signed) && event_log::verify_author(event).is_ok()
                    && event_log::owner_budget::digest(&signed.policy).as_deref() == Some(policy_digest)));
        original
            .map(|event| self.owner_policy_status(scope, &event.client_id))
            .transpose()
    }

    pub(crate) fn submit_owner_policy(
        &mut self,
        scope: &OwnerPolicyScope,
        json: &str,
        confirmed: &str,
    ) -> R<OwnerPolicyStatus> {
        let review = self.review_owner_policy(scope, json)?;
        if review.confirmation_digest != confirmed {
            return Err(denied());
        }
        // Pending outbox entries and confirmed log entries share one original ID.
        let pending = self.store.outbox()?;
        if let Some(event) = pending.iter().map(|entry| &entry.event)
            .chain(self.logs.get(&scope.space_id).into_iter().flat_map(|log| log.events()))
            .find(|event| {
                event.space == scope.space_id && matches!(&event.body, EventBody::OwnerPolicySubmitted { agent, signed }
                    if agent == &scope.agent_id && event.author == scope.expected_identity_id
                    && event.device.as_deref() == Some(scope.expected_device_id.as_str())
                    && event_log::owner_budget::verify(signed)
                    && event_log::verify_author(event).is_ok()
                    && event_log::owner_budget::digest(&signed.policy).as_deref() == Some(review.policy_digest.as_str()))
            }) {
            return self.owner_policy_status(scope, &event.client_id);
        }
        let policy = decode(&review.canonical_policy_json)?;
        let author = self
            .net
            .account_author(&scope.expected_identity_id)
            .ok_or_else(denied)?;
        let signed = event_log::owner_budget::sign(&author, policy).ok_or_else(denied)?;
        let event = self.append_synced(
            &scope.space_id,
            &author,
            crate::engine::now_ms(),
            EventBody::OwnerPolicySubmitted {
                agent: scope.agent_id.clone(),
                signed,
            },
        )?;
        Ok(OwnerPolicyStatus {
            source: event.client_id,
            policy_digest: review.policy_digest,
            state: "pending".into(),
        })
    }
}

impl Client {
    /// Read-only reconciliation after a lost submission acknowledgement.
    pub fn find_owner_policy_submission(
        &self,
        scope: OwnerPolicyScope,
        policy_digest: String,
    ) -> R<Option<OwnerPolicyStatus>> {
        self.lock()
            .find_owner_policy_submission(&scope, &policy_digest)
    }
    pub fn review_owner_policy(
        &self,
        scope: OwnerPolicyScope,
        policy_json: String,
    ) -> R<OwnerPolicyReview> {
        self.lock().review_owner_policy(&scope, &policy_json)
    }
    pub fn submit_owner_policy(
        &self,
        scope: OwnerPolicyScope,
        policy_json: String,
        confirmation_digest: String,
    ) -> R<OwnerPolicyStatus> {
        self.lock()
            .submit_owner_policy(&scope, &policy_json, &confirmation_digest)
    }
    pub fn owner_policy_status(
        &self,
        scope: OwnerPolicyScope,
        source: String,
    ) -> R<OwnerPolicyStatus> {
        self.lock().owner_policy_status(&scope, &source)
    }
}
