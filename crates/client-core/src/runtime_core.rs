//! Rust-only device core for retained cloud workspaces. Native facts are not
//! cloud permission: the runtime must authenticate custody, enrollment, source
//! head, leases, financial policy and continuity before using this facade.

use std::fmt;

use ::mls::Leaf;
use event_log::{agent_owner, Author, Signer, SpaceLog};
use grants::{evaluate_permission, Policy};
use protocol::Sequenced;
use serde::{Deserialize, Serialize};
use store::{Store, MAX_MEMORY_IMAGE_BYTES};
use types::*;
use zeroize::Zeroizing;

use crate::engine::{Engine, GrantState, State};
use crate::sync::{AccountMeta, Ingest, EVENT_FORMAT_META, EVENT_FORMAT_VALUE};

const PROFILE: &str = "owner-direct-text-reply/1";
const MAX_BATCH: usize = 64;
const MAX_EVENT_BYTES: usize = 65536;
const MAX_CONTEXT_BYTES: usize = 32768;

#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("unsupported or oversized native image")]
    Image,
    #[error("invalid certified agent device or owner")]
    Credential,
    #[error("native history is incomplete or invalid")]
    History,
    #[error("native reply permission is unavailable")]
    Permission,
    #[error("MLS group is not ready")]
    NotReady,
    #[error("native input exceeds the supported profile")]
    Bounds,
    #[error("native storage operation failed")]
    Storage,
}

type Result<T> = std::result::Result<T, NativeError>;

mod key_packages;
pub use key_packages::InitialKeyPackages;
mod publication;
mod receipt;
mod reference;
pub use publication::PreparedReply;
pub use receipt::{AcceptedReply, NativeReceiptPrefix, ReplyReceipt};

/// Inert plaintext image. A successful parse does not authenticate its origin.
/// The private cloud loader must first authenticate the entire custody capsule.
pub struct NativeImage(Zeroizing<Vec<u8>>);

impl fmt::Debug for NativeImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeImage").finish_non_exhaustive()
    }
}

impl NativeImage {
    pub fn decode_bounded(bytes: Vec<u8>) -> Result<Self> {
        let bytes = Zeroizing::new(bytes);
        Store::from_memory_image(&bytes).map_err(|_| NativeError::Image)?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Cryptographic credential only; this does not assert SQL enrollment.
pub struct UnlockedDevice {
    agent: Identity,
    owner: Identity,
    leaf: Leaf,
    secret: Zeroizing<[u8; 32]>,
}

impl fmt::Debug for UnlockedDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnlockedDevice").finish_non_exhaustive()
    }
}

impl UnlockedDevice {
    pub fn verify(
        agent: Identity,
        owner: Identity,
        certificate: String,
        device_secret: [u8; 32],
    ) -> Result<Self> {
        let secret = Zeroizing::new(device_secret);
        let device = Signer::from_secret(&secret).id();
        let leaf = Leaf {
            identity: agent.id.clone(),
            device,
            cert: certificate,
        };
        if agent.kind != IdentityKind::Agent
            || owner.kind != IdentityKind::Person
            || agent.owner.as_deref() != Some(owner.id.as_str())
            || leaf.device == agent.id
            || !agent_owner::profile_authorized(&agent)
            || !agent_owner::profile_authorized(&owner)
            || hex::decode(&leaf.device)
                .ok()
                .and_then(|key| Leaf::verified(&leaf.encode(), &key))
                .as_ref()
                != Some(&leaf)
        {
            return Err(NativeError::Credential);
        }
        Ok(Self {
            agent,
            owner,
            leaf,
            secret,
        })
    }
}

/// Immutable original action references. Deserialization is inert; refreshing
/// requires the same verified source event and exact original grant.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplyIntent {
    profile: String,
    agent: IdentityId,
    owner: IdentityId,
    space: SpaceId,
    trigger: String,
    trigger_digest: String,
    grant: GrantId,
    grant_digest: String,
}

impl ReplyIntent {
    pub fn trigger(&self) -> &str {
        &self.trigger
    }

    pub fn space(&self) -> &str {
        &self.space
    }

    pub fn agent(&self) -> &str {
        &self.agent
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeFrontier {
    seq: u64,
    hash: String,
    epoch: u64,
}

impl NativeFrontier {
    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// Derived native facts, with no public constructor or Deserialize. These do
/// not mint a runtime dispatch capability or prove a current relay frontier.
pub struct ReplyFacts {
    intent: ReplyIntent,
    frontier: NativeFrontier,
    expires_at_ms: Option<i64>,
    text: Zeroizing<String>,
}

/// A bounded scan of authenticated native history. The cursor is retained
/// inside the complete device image, so activation can bind it to run creation.
pub struct ReplyDiscovery {
    pub replies: Vec<ReplyFacts>,
    pub examined: u16,
    pub complete: bool,
    pub changed: bool,
}

impl fmt::Debug for ReplyFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplyFacts")
            .field("intent", &self.intent)
            .field("frontier", &self.frontier)
            .finish_non_exhaustive()
    }
}

impl ReplyFacts {
    pub fn intent(&self) -> &ReplyIntent {
        &self.intent
    }

    pub fn frontier(&self) -> &NativeFrontier {
        &self.frontier
    }

    pub fn expires_at_ms(&self) -> Option<i64> {
        self.expires_at_ms
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

pub struct DeviceCore {
    engine: Engine,
    agent: IdentityId,
    owner: IdentityId,
    usable: bool,
}

fn direct_writers(state: &State, space: &str, owner: &str, agent: &str) -> bool {
    state.spaces.get(space).is_some_and(|s| {
        s.kind == SpaceKind::Direct
            && s.privacy == Privacy::EndToEnd
            && s.members.len() == 2
            && state.writer(space, owner)
            && state.writer(space, agent)
    })
}

fn limit_pages(store: &Store) -> Result<()> {
    let size: u64 = store
        .conn()
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .map_err(|_| NativeError::Image)?;
    store
        .conn()
        .pragma_update(None, "max_page_count", MAX_MEMORY_IMAGE_BYTES as u64 / size)
        .map_err(|_| NativeError::Image)
}

type Schema = Vec<(String, String, String, Option<String>)>;

fn schema(store: &Store) -> Result<Schema> {
    let mut query = store
        .conn()
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
        .map_err(|_| NativeError::Image)?;
    let rows = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|_| NativeError::Image)?;
    rows.collect::<std::result::Result<_, _>>()
        .map_err(|_| NativeError::Image)
}

fn provider_migrations(store: &Store) -> Result<Vec<(i64, String, String)>> {
    let mut query = store
        .conn()
        .prepare(
            "SELECT version,name,checksum FROM openmls_sqlite_storage_migrations ORDER BY version",
        )
        .map_err(|_| NativeError::Image)?;
    let rows = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|_| NativeError::Image)?;
    rows.collect::<std::result::Result<_, _>>()
        .map_err(|_| NativeError::Image)
}

mod image_schema;

fn validate_image(store: &Store) -> Result<()> {
    let trusted = Engine::open(":memory:").map_err(|_| NativeError::Storage)?;
    let version: i64 = store
        .conn()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| NativeError::Image)?;
    if version != 2
        || !image_schema::supported(store, &trusted.store)?
        || provider_migrations(store)? != provider_migrations(&trusted.store)?
        || store
            .meta(EVENT_FORMAT_META)
            .map_err(|_| NativeError::Image)?
            .as_deref()
            != Some(EVENT_FORMAT_VALUE)
    {
        return Err(NativeError::Image);
    }
    for (_, name, _, _) in schema(&trusted.store)? {
        // Only names from the trusted, exact schema enter this SQL identifier.
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(NativeError::Image);
        }
        let mut columns = store
            .conn()
            .prepare(&format!("PRAGMA table_info(\"{name}\")"))
            .map_err(|_| NativeError::Image)?;
        let columns = columns
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|_| NativeError::Image)?;
        let columns = columns
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| NativeError::Image)?;
        if columns.iter().any(|column| column == "provider_version") {
            let invalid: bool = store
                .conn()
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM \"{name}\" WHERE provider_version <> 1)"),
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| NativeError::Image)?;
            if invalid {
                return Err(NativeError::Image);
            }
        }
    }
    let secret: bool = store.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM identities WHERE secret IS NOT NULL) OR EXISTS(SELECT 1 FROM meta WHERE key LIKE 'zoen.%')",
        [], |r| r.get(0)).map_err(|_| NativeError::Image)?;
    if secret
        || store
            .identities()
            .map_err(|_| NativeError::Image)?
            .iter()
            .any(|(profile, _)| !agent_owner::profile_authorized(profile))
    {
        return Err(NativeError::Image);
    }
    // Only the exact previous schema reaches this narrow, inert upgrade. The
    // authenticated custody image and all event/provider/credential checks above
    // remain required before installing any Engine state.
    store
        .upgrade_outbox_order()
        .map_err(|_| NativeError::Image)?;
    if schema(store)? != schema(&trusted.store)? {
        return Err(NativeError::Image);
    }
    limit_pages(store)
}

#[cfg(test)]
mod tests;

mod device_image;
mod device_reply;
