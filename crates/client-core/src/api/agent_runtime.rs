//! Server-side Agent identity and runtime commands; the same Client owns state.

use super::{
    invalid, unexpected, AccountDto, Arc, Client, CoreError, Op, Reply, SecretVault,
    VAULT_AGREEMENT, VAULT_DEVICE, VAULT_IDENTITY,
};

/// The Agent identity id a root secret signs as (server-side runtime).
pub fn agent_id_for_root(root: Vec<u8>) -> Result<String, CoreError> {
    let root: [u8; 32] = root
        .as_slice()
        .try_into()
        .map_err(|_| invalid("agent key"))?;
    Ok(event_log::Signer::from_secret(&root).id())
}

/// Server-side agent runtime only; deliberately outside the native FFI surface.
impl Client {
    /// Agent citizen account (server-side runtime). Not exported to native apps.
    #[allow(clippy::too_many_arguments)]
    pub fn create_agent_account(
        &self,
        name: String,
        handle: String,
        relay_url: String,
        root_secret: Vec<u8>,
        owner: String,
        proof_json: String,
        vault: Arc<dyn SecretVault>,
    ) -> Result<AccountDto, CoreError> {
        let root: [u8; 32] = root_secret
            .as_slice()
            .try_into()
            .map_err(|_| invalid("agent key"))?;
        let proof: types::AgentOwnerProof =
            serde_json::from_str(&proof_json).map_err(|_| invalid("agent proof"))?;
        {
            let mut e = self.lock();
            let (root, device, agreement) =
                e.create_agent_account(&name, &handle, &relay_url, root, &owner, proof)?;
            if !vault.save(VAULT_IDENTITY.into(), root.to_vec())
                || !vault.save(VAULT_DEVICE.into(), device.to_vec())
                || !vault.save(VAULT_AGREEMENT.into(), agreement.to_vec())
            {
                let _ = e.wipe();
                let _ = e.store.meta_delete("account");
                return Err(invalid("vault"));
            }
        }
        self.account_dto().ok_or_else(|| invalid("account"))
    }

    /// The Person this agent account answers to (ADR 0049), from its own profile.
    pub fn agent_owner(&self) -> Option<String> {
        let e = self.lock();
        let me = e.me_id().ok()?;
        e.store
            .identities()
            .ok()?
            .into_iter()
            .find(|(i, _)| i.id == me)
            .and_then(|(i, _)| i.owner)
    }

    /// Inspect already-known owner receipts without creating a DM or requiring
    /// the owner's directory profile. Absence and unreadable history differ.
    pub fn agent_owner_timeline(
        &self,
        owner: &str,
    ) -> Result<Option<Vec<crate::TimelineEntry>>, CoreError> {
        let e = self.lock();
        let me = e.me_id()?;
        let bound_owner = e
            .store
            .identities()?
            .into_iter()
            .find(|(identity, _)| identity.id == me)
            .and_then(|(identity, _)| identity.owner);
        if bound_owner.as_deref() != Some(owner) {
            return Err(CoreError::Forbidden {
                reason: "The agent owner changed".into(),
            });
        }
        e.direct_with(owner)
            .map(|space| e.timeline(&space))
            .transpose()
    }

    /// The relay's record for this agent runtime: status, definition and cursors.
    pub async fn agent_state(&self) -> Result<protocol::AgentStateRecord, CoreError> {
        let (owner, channel) = self.device_directory_owner()?;
        let reply = channel
            .clone()
            .send(Op::Agent(protocol::AgentRequest::State))
            .await
            .map_err(invalid)?;
        let Reply::AgentState(state) = reply else {
            return Err(unexpected());
        };
        if state.cursors.len() > protocol::complete_reads::CURSOR_ROWS
            || state.definition.len() > protocol::complete_reads::DEFINITION_BYTES
        {
            return Err(invalid(protocol::complete_reads::PROFILE_EXCEEDED));
        }
        let mut spaces = std::collections::BTreeSet::new();
        if state.cursors.iter().any(|(space, _)| {
            space.is_empty()
                || space.len() > protocol::complete_reads::KEY_BYTES
                || !spaces.insert(space.clone())
        }) {
            return Err(invalid(protocol::complete_reads::INVALID));
        }
        self.with_device_directory_completion(&owner, &channel, |_| Ok(state))
    }

    /// This agent runtime handled `space` through `seq` (inclusive).
    pub async fn agent_advance(&self, space: String, seq: u64) -> Result<(), CoreError> {
        self.agent_done(protocol::AgentRequest::Advance { space, seq })
            .await
    }

    /// A new definition version for `agent` (owner, or the agent's own first one).
    pub async fn agent_define(
        &self,
        agent: String,
        version: u32,
        kind: String,
        definition: Vec<u8>,
    ) -> Result<(), CoreError> {
        self.agent_done(protocol::AgentRequest::Define {
            agent,
            version,
            kind,
            definition,
        })
        .await
    }

    /// Owner: "pause", "resume" or "revoke" one of your agents.
    pub async fn agent_control(&self, agent: String, action: String) -> Result<(), CoreError> {
        let action = protocol::AgentAction::parse(&action).ok_or_else(|| invalid("action"))?;
        let action = match action {
            protocol::AgentAction::Pause => crate::AgentControl::Pause,
            protocol::AgentAction::Resume => crate::AgentControl::Resume,
            protocol::AgentAction::Revoke => crate::AgentControl::Revoke,
        };
        self.control_agent(agent, action).await
    }

    pub(super) async fn agent_done(&self, req: protocol::AgentRequest) -> Result<(), CoreError> {
        match self.request(Op::Agent(req)).await? {
            Reply::Done => Ok(()),
            _ => Err(unexpected()),
        }
    }

    /// Owner side of ADR 0049: this unlocked device signs the agent root, as JSON.
    pub fn authorize_agent_json(&self, agent_id: String) -> Result<String, CoreError> {
        let proof = self.lock().authorize_agent(&agent_id)?;
        serde_json::to_string(&proof).map_err(|_| invalid("proof"))
    }
}
