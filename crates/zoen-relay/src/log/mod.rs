//! The log seam (ADR 0011): per-Space logs, heads, idempotency keys, membership and
//! invites, behind one trait. FoundationDB implements it ([`fdb::FdbLog`]); the directory
//! (identities, handles, devices) stays in Postgres.

pub mod admission;
pub mod fdb;

use roda_proto::{Envelope, InviteCreated, Sequenced};
use roda_types::Role;

/// Why an envelope was refused. `permanent` = retrying the same envelope can't succeed.
#[derive(Debug)]
pub struct Reject {
    pub reason: String,
    pub permanent: bool,
}

impl Reject {
    pub fn no(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            permanent: true,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            reason: "log store unavailable".into(),
            permanent: false,
        }
    }
}

pub enum Sequencing {
    New {
        ev: Sequenced,
        /// Members after the append, plus whoever it removed (they learn they're out).
        audience: Vec<String>,
        joined: Option<String>,
    },
    /// Same `client_id` again: the stored copy, so the device can stop retrying.
    Duplicate { ev: Sequenced },
}

/// A redeemable invite, as the preview shows it.
pub struct InviteInfo {
    pub space: String,
    pub role: Role,
    pub inviter: String,
    pub title: String,
    pub members: u32,
}

#[derive(Debug)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

/// Every call names its Space or identity, so cells and regions route above the trait.
#[async_trait::async_trait]
pub trait LogStore: Send + Sync {
    /// Admits and sequences one envelope in a single transaction. `target_known` says
    /// whether the identity a `MemberAdded` names is in the directory.
    async fn append(&self, env: &Envelope, target_known: bool) -> Result<Sequencing, Reject>;
    /// Events of `space` from `from` on, in order, at most `limit`.
    async fn read(
        &self,
        space: &str,
        from: u64,
        limit: usize,
    ) -> Result<Vec<Sequenced>, StoreError>;
    async fn role(&self, space: &str, who: &str) -> Result<Option<Role>, StoreError>;
    async fn members(&self, space: &str) -> Result<Vec<(String, Role)>, StoreError>;
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
    async fn preview_invite(&self, code: &str) -> Result<InviteInfo, String>;
}
