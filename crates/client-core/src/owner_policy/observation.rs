//! Rust-only evidence from an authenticated current control, never authority.
use super::{
    denied, Client, CoreError, Engine, IdentityKind, OwnerPolicyInquiryControl, Serialize, R,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerGroupState {
    Credential,
    Missing,
    Pending,
    Ready,
    Storage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerDirectoryState {
    Missing,
    Complete,
    Invalid,
    Storage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerConnectionState {
    Unobserved,
    Offline,
    Connecting,
    Online,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WorkerGroupObservation {
    /// Hash of this public device ID only; no raw identity, device or conversation.
    pub device_fingerprint: String,
    pub group: WorkerGroupState,
    pub directory: WorkerDirectoryState,
    pub space_synced: bool,
    pub socket: WorkerConnectionState,
    pub socket_synced: Option<bool>,
    pub roster_lists_self: bool,
    pub own_leaf: Option<bool>,
    pub self_active: Option<bool>,
    pub active_devices: Option<u32>,
    pub revoked_devices: Option<u32>,
    pub other_active_devices: Option<u32>,
    pub other_active_leaves: Option<u32>,
    pub asked_other_devices: Option<u32>,
    pub dirty: bool,
    pub claiming: bool,
    pub retry_scheduled: bool,
}
impl Engine {
    pub(crate) fn owner_policy_worker_observation(
        &self,
        query: &OwnerPolicyInquiryControl,
    ) -> R<WorkerGroupObservation> {
        self.owner_policy_worker_observation_at(query, crate::engine::now_ms())
    }

    pub(crate) fn owner_policy_worker_observation_at(
        &self,
        query: &OwnerPolicyInquiryControl,
        at_ms: i64,
    ) -> R<WorkerGroupObservation> {
        // Recheck expiration, current principal, membership and original signed
        // bytes after IPC as well. A stale query cannot solicit diagnostics.
        let current = self.owner_policy_authenticated_original(query.space(), query.source())?;
        let signed = current.signed();
        let inquiry = &signed.inquiry;
        // This bounded diagnostic is specifically for the account-management
        // control. It cannot replace image-reference or group-readiness checks.
        if current.digest() != query.digest()
            || signed != query.signed()
            || inquiry.purpose
                != Some(types::owner_policy_context::OwnerPolicyInquiryPurpose::AccountBudget)
            || !inquiry.fresh_at(at_ms)
            || self
                .identities
                .get(&inquiry.owner)
                .is_none_or(|owner| owner.kind != IdentityKind::Person)
            || !self.state.writer(current.space(), &inquiry.owner)
        {
            return Err(denied());
        }
        self.linked_group_observation(current.space())
    }
}
impl Client {
    /// Not an FFI command or RPC. Returned facts cannot approve or publish work.
    pub fn owner_policy_worker_observation(
        &self,
        query: &OwnerPolicyInquiryControl,
    ) -> R<WorkerGroupObservation> {
        // Existing global order: Net then Engine. Stop/account transitions
        // cannot replace this channel between the scope check and snapshot.
        let net = self.net.lock().map_err(|_| CoreError::Storage {
            message: "Owner-control connection observation unavailable.".into(),
        })?;
        let mut observed = self.lock().owner_policy_worker_observation(query)?;
        let status = net.as_ref().map(crate::net::Net::status);
        observed.socket = match status.as_ref().map(|status| &status.state) {
            None | Some(crate::net::ConnState::Offline) => WorkerConnectionState::Offline,
            Some(crate::net::ConnState::Connecting) => WorkerConnectionState::Connecting,
            Some(crate::net::ConnState::Online) => WorkerConnectionState::Online,
        };
        observed.socket_synced = Some(status.is_some_and(|status| status.synced));
        Ok(observed)
    }
}
