//! The log seam (ADR 0011): per-Space logs, heads, idempotency keys, membership and
//! invites, behind one trait. FoundationDB implements it ([`fdb::FdbLog`]); the directory
//! (identities, handles, devices) stays in Postgres.

pub mod admission;
pub mod fdb;
pub mod sequencer;

use protocol::{Envelope, InviteCreated, Sequenced};
use types::Role;

pub use relay_admission::Reject;

#[derive(Debug)]
pub enum Sequencing {
    New {
        ev: Sequenced,
        /// Members after the append, plus whoever it removed (they learn they're out).
        audience: Vec<String>,
        joined: Option<String>,
    },
    /// Same `client_id` again: the stored copy, so the device can stop retrying.
    Duplicate {
        ev: Sequenced,
        /// Current members at the replay transaction, so late acknowledgments
        /// can restore live delivery without disclosing to departed identities.
        audience: Vec<String>,
        joined: Option<String>,
    },
}

/// A redeemable invite, as the preview shows it.
pub struct InviteInfo {
    pub space: String,
    pub role: Role,
    pub inviter: String,
    pub title: String,
    pub members: u32,
    /// Agent identity id when this invite opens a chat with an agent.
    pub agent: Option<String>,
}

#[derive(Debug)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

/// One history page and its membership boundary from the SAME store snapshot.
/// A current member has no removal ceiling; a former member may receive their
/// removal itself. None from read_visible means this reader has no history grant.
pub struct VisiblePage {
    pub events: Vec<Sequenced>,
    pub through: Option<u64>,
}

/// Every call names its Space or identity, so cells and regions route above the trait.
#[async_trait::async_trait]
pub trait LogStore: Send + Sync {
    /// Read-only transport proof. An unsupported store cannot claim a runtime
    /// deployment. The expected witness is read by the host from its own SQL.
    async fn runtime_cell(
        &self,
        _namespace: &str,
        _witness: &str,
    ) -> Result<Option<String>, StoreError> {
        Ok(None)
    }
    /// Whether this node can renew its cell authority. Test stores default to ready.
    async fn ready(&self) -> bool {
        true
    }
    /// Admits and sequences one envelope in a single transaction. `target_known` says
    /// whether the identity a `MemberAdded` names is in the directory.
    async fn append(&self, env: &Envelope, target_known: bool) -> Result<Sequencing, Reject>;
    /// A managed generation must survive the actual queued commit. Unsupported
    /// stores refuse rather than dropping the guard and calling ordinary append.
    async fn append_managed(
        &self,
        _env: &Envelope,
        _target_known: bool,
        _generation: relay_admission::managed_images::GenerationGuard,
    ) -> Result<Sequencing, Reject> {
        Err(Reject::retry("managed publication unavailable"))
    }
    /// Events of `space` from `from` on, in order, at most `limit`.
    async fn read(
        &self,
        space: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<Sequenced>, StoreError>;
    /// Atomically admit this reader and scan the bounded history page. Separate
    /// role/removed_at/read calls are insufficient during concurrent removal.
    async fn read_visible(
        &self,
        space: &str,
        who: &str,
        from: u64,
        limit: usize,
    ) -> Result<Option<VisiblePage>, StoreError>;
    async fn role(&self, space: &str, who: &str) -> Result<Option<Role>, StoreError>;
    /// Where `who` was removed from `space`, if they were and haven't been added back.
    async fn removed_at(&self, space: &str, who: &str) -> Result<Option<u64>, StoreError>;
    async fn members(&self, space: &str) -> Result<Vec<(String, Role)>, StoreError>;
    /// One bounded transaction over current DM metadata and exactly two writers.
    /// Transport admission only; the relay cannot inspect encrypted call consent.
    async fn call_pair(&self, _space: &str, _who: &str, _peer: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
    async fn spaces_of(&self, who: &str) -> Result<Vec<String>, StoreError>;
    /// Identities that share at least one Space with `who` (presence audience).
    async fn co_members(&self, who: &str) -> Result<Vec<String>, StoreError>;
    async fn create_invite(
        &self,
        who: &str,
        space: &str,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
    ) -> Result<InviteCreated, String>;
    /// The agent's one share link: returns its live code, or mints one when there is none
    /// (or it expired, ran out of uses, or was revoked).
    async fn create_agent_invite(
        &self,
        who: &str,
        agent: &str,
        max_uses: u32,
        ttl_secs: u64,
    ) -> Result<InviteCreated, String>;
    async fn preview_invite(&self, code: &str) -> Result<InviteInfo, String>;
    /// Spends one use of an agent link in one transaction; refused past `max_uses`.
    /// Each person counts once: `who` is stored with the invite in the same transaction.
    async fn redeem_agent_invite(&self, code: &str, who: &str) -> Result<InviteInfo, String>;
    /// Takes the agent's live link down. Ownership is checked by the caller.
    async fn revoke_agent_invite(&self, agent: &str) -> Result<(), String>;
}
