//! Rust-only device core for retained cloud workspaces. Native facts are not
//! cloud permission: the runtime must authenticate custody, enrollment, source
//! head, leases, financial policy and continuity before using this facade.

use std::fmt;

use roda_grants::{evaluate_permission, Policy};
use roda_log::{agent_owner, Author, Signer, SpaceLog};
use roda_mls::Leaf;
use roda_proto::Sequenced;
use roda_store::{Store, MAX_MEMORY_IMAGE_BYTES};
use roda_types::*;
use serde::{Deserialize, Serialize};
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

impl DeviceCore {
    /// Installs public profiles and a certified device without local personas,
    /// root secrets, demo grants, Space membership or enrollment claims.
    pub fn bootstrap(device: UnlockedDevice, relay_url: &str) -> Result<Self> {
        if relay_url.is_empty() || relay_url.len() > 2048 {
            return Err(NativeError::Bounds);
        }
        let mut engine = Engine::open(":memory:").map_err(|_| NativeError::Storage)?;
        limit_pages(&engine.store)?;
        engine
            .store
            .put_identity(&device.agent, None)
            .and_then(|_| engine.store.put_identity(&device.owner, None))
            .and_then(|_| engine.store.set_meta("me", &device.agent.id))
            .map_err(|_| NativeError::Storage)?;
        let account = AccountMeta {
            identity: device.agent.id.clone(),
            device: device.leaf.device.clone(),
            cert: device.leaf.cert.clone(),
            relay_url: relay_url.into(),
            registered: false,
        };
        engine
            .save_linked_account(account)
            .map_err(|_| NativeError::Storage)?;
        engine.reload().map_err(|_| NativeError::Storage)?;
        Self::install(engine, device)
    }

    /// Called by the custody owner after authenticating a committed capsule.
    /// Rejects unsupported formats before Engine's legacy reset/migrations.
    pub fn restore(image: NativeImage, device: UnlockedDevice) -> Result<Self> {
        let store = Store::from_memory_image(image.as_bytes()).map_err(|_| NativeError::Image)?;
        validate_image(&store)?;
        let engine = Engine::from_store(store, ":memory:").map_err(|_| NativeError::Image)?;
        Self::install(engine, device)
    }

    fn install(mut engine: Engine, device: UnlockedDevice) -> Result<Self> {
        let account = engine.net.account.as_ref().ok_or(NativeError::Credential)?;
        if account.identity != device.agent.id
            || account.device != device.leaf.device
            || account.cert != device.leaf.cert
            || engine.me.as_deref() != Some(device.agent.id.as_str())
            || engine.identities.get(&device.agent.id) != Some(&device.agent)
            || engine.identities.get(&device.owner.id) != Some(&device.owner)
            || engine
                .state
                .spaces
                .values()
                .any(|space| space.integrity_error.is_some())
        {
            return Err(NativeError::Credential);
        }
        engine.net.author = Some(Author::certified(
            device.agent.id.clone(),
            Signer::from_secret(&device.secret),
            device.leaf.cert,
        ));
        engine.device().map_err(|_| NativeError::Credential)?;
        Ok(Self {
            engine,
            agent: device.agent.id,
            owner: device.owner.id,
            usable: true,
        })
    }

    pub fn image(&self) -> Result<NativeImage> {
        if !self.usable {
            return Err(NativeError::History);
        }
        let bytes = self
            .engine
            .store
            .memory_image()
            .map_err(|_| NativeError::Image)?;
        Ok(NativeImage(Zeroizing::new(bytes)))
    }

    /// Retains the matching private OpenMLS material before a host publishes
    /// these public packages. A host must activate this image before delivery.
    pub fn key_packages(&mut self, count: usize) -> Result<(Vec<Vec<u8>>, NativeImage)> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if !(1..=32).contains(&count) {
            return Err(NativeError::Bounds);
        }
        let result = (|| {
            let packages = self
                .engine
                .device()
                .map_err(|_| NativeError::Credential)?
                .key_packages(count, false)
                .map_err(|_| NativeError::Storage)?;
            Ok((packages, self.image()?))
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }

    /// Partial ingestion never permits a partial activation: on error the cloud
    /// actor must discard this workspace and reload its committed capsule.
    pub fn ingest(&mut self, batch: Vec<Sequenced>) -> Result<NativeImage> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if batch.len() > MAX_BATCH
            || batch
                .iter()
                .any(|entry| entry.env.content().len() > MAX_EVENT_BYTES)
        {
            return Err(NativeError::Bounds);
        }
        for entry in batch {
            match self.engine.ingest(entry) {
                Ingest::Applied | Ingest::Confirmed | Ingest::Duplicate => {}
                Ingest::Gap { .. } | Ingest::Invalid(_) => {
                    self.usable = false;
                    return Err(NativeError::History);
                }
            }
        }
        let image = self.image();
        if image.is_err() {
            self.usable = false;
        }
        image
    }

    pub fn reply(&self, space: &str, trigger: &str, at_ms: i64) -> Result<ReplyFacts> {
        self.extract(space, trigger, at_ms)
    }

    /// Applied native position only. This is not a current relay head or grant.
    pub fn retained_head(&self, space: &str) -> Result<Option<Seen>> {
        if !self.usable {
            return Err(NativeError::History);
        }
        Ok(self.engine.logs.get(space).and_then(SpaceLog::head))
    }

    pub fn discover_replies(&mut self, space: &str, at_ms: i64) -> Result<ReplyDiscovery> {
        if !self.usable || space.is_empty() || space.len() > 256 || at_ms < 0 {
            return Err(NativeError::History);
        }
        let key = format!("runtime.reply-scan/1/{space}");
        let cursor = self
            .engine
            .store
            .meta(&key)
            .map_err(|_| NativeError::Storage)?;
        let cursor: Option<Seen> = cursor
            .map(|value| {
                if value.len() > 256 {
                    return Err(NativeError::History);
                }
                serde_json::from_str(&value).map_err(|_| NativeError::History)
            })
            .transpose()?;
        let log = self.engine.logs.get(space).ok_or(NativeError::History)?;
        let from = match &cursor {
            Some(cursor) => {
                let index = usize::try_from(cursor.seq).map_err(|_| NativeError::History)?;
                if log
                    .events()
                    .get(index)
                    .is_none_or(|event| event.seq != cursor.seq || event.hash != cursor.hash)
                {
                    return Err(NativeError::History);
                }
                index.checked_add(1).ok_or(NativeError::History)?
            }
            None => 0,
        };
        let mut replies = Vec::new();
        let mut examined = 0u16;
        let mut through = cursor;
        for event in log.events().iter().skip(from).take(MAX_BATCH) {
            if event.author == self.owner
                && matches!(
                    &event.body,
                    EventBody::MessagePosted {
                        attaches: None,
                        reply: None,
                        ..
                    }
                )
            {
                match self.extract(space, &event.hash, at_ms) {
                    Ok(facts) => replies.push(facts),
                    Err(NativeError::Permission) => {}
                    Err(error) => return Err(error),
                }
            }
            examined += 1;
            through = Some(Seen {
                seq: event.seq,
                hash: event.hash.clone(),
            });
            if replies.len() == 16 {
                break;
            }
        }
        let complete = through.as_ref() == log.head().as_ref();
        if examined > 0 {
            let value = serde_json::to_string(&through.ok_or(NativeError::History)?)
                .map_err(|_| NativeError::Storage)?;
            if self.engine.store.set_meta(&key, &value).is_err() {
                self.usable = false;
                return Err(NativeError::Storage);
            }
        }
        Ok(ReplyDiscovery {
            replies,
            examined,
            complete,
            changed: examined > 0,
        })
    }

    pub fn refresh(&self, original: &ReplyIntent, at_ms: i64) -> Result<ReplyFacts> {
        let current = self.extract(&original.space, &original.trigger, at_ms)?;
        if &current.intent != original {
            return Err(NativeError::Permission);
        }
        Ok(current)
    }

    fn extract(&self, space: &str, trigger: &str, at_ms: i64) -> Result<ReplyFacts> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if at_ms < 0 {
            return Err(NativeError::Permission);
        }
        let log = self.engine.logs.get(space).ok_or(NativeError::History)?;
        let mut ordered = State::default();
        let mut selected: Option<(GrantState, &Event)> = None;
        let mut grant_events = std::collections::HashMap::new();
        let mut original = None;
        let mut readable = false;
        let mut text = Zeroizing::new(String::new());
        for event in log.events() {
            let opened = crate::mls_opening::opened_leaf(&self.engine.store, event);
            let own_echo = self.engine.net.account.as_ref().is_some_and(|account| {
                crate::mls_opening::own_echo(&self.engine.store, event, account)
            });
            let historical_writers = direct_writers(&ordered, space, &self.owner, &self.agent);
            let accepted = ordered.apply_checked(event, &self.engine.identities);
            if let EventBody::GrantIssued { grant } = &event.body {
                if accepted {
                    grant_events.insert(grant.id.as_str(), event);
                }
                if accepted
                    && grant.grantor == self.owner
                    && grant.grantee.as_deref() == Some(self.agent.as_str())
                    && grant.scope == GrantScope::Space(space.into())
                    && matches!(grant.capability, Capability::Trust(_))
                {
                    // A new baseline needs an actual owner opening. Old opaque
                    // history supplies neither permission nor model context.
                    readable = opened
                        .as_ref()
                        .is_some_and(|from| from.identity == self.owner);
                    text.clear();
                }
            }
            let supported = match &event.body {
                EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::MemberRemoved { .. }
                | EventBody::SpaceEncrypted
                | EventBody::Checkpoint { .. }
                | EventBody::DeviceJoining { .. } => true,
                EventBody::Sealed { kind } => kind == "Commit" || kind == "Welcome",
                EventBody::GrantIssued { .. } | EventBody::GrantRevoked { .. } => opened.is_some(),
                EventBody::MessagePosted {
                    attaches, reply, ..
                } => attaches.is_none() && reply.is_none() && (opened.is_some() || own_echo),
                _ => false,
            };
            if !supported {
                readable = false;
            }
            // Select by ordered issuance, never a wall-clock event timestamp.
            let active = latest_trust(&ordered, space, &self.owner, &self.agent);
            selected = active.and_then(|grant| {
                grant_events
                    .get(grant.grant.id.as_str())
                    .map(|event| (grant.clone(), *event))
            });
            if let EventBody::MessagePosted { text: message, .. } = &event.body {
                if readable
                    && historical_writers
                    && original.is_none()
                    && (own_echo
                        || opened.as_ref().is_some_and(|from| {
                            from.identity == self.owner || from.identity == self.agent
                        }))
                {
                    if text
                        .len()
                        .checked_add(message.len())
                        .and_then(|n| n.checked_add(1))
                        .is_none_or(|n| n > MAX_CONTEXT_BYTES)
                    {
                        return Err(NativeError::Bounds);
                    }
                    text.push_str(message);
                    text.push('\n');
                }
                if event.hash == trigger {
                    let (grant, grant_event) = selected.as_ref().ok_or(NativeError::Permission)?;
                    if !readable
                        || !historical_writers
                        || event.author != self.owner
                        || opened
                            .as_ref()
                            .is_none_or(|from| from.identity != self.owner)
                        || grant
                            .grant
                            .expires_at_ms
                            .is_some_and(|expiry| expiry <= event.at_ms)
                    {
                        return Err(NativeError::Permission);
                    }
                    original = Some(ReplyIntent {
                        profile: PROFILE.into(),
                        agent: self.agent.clone(),
                        owner: self.owner.clone(),
                        space: space.into(),
                        trigger: event.hash.clone(),
                        trigger_digest: roda_log::content_hash_of(event),
                        grant: grant.grant.id.clone(),
                        grant_digest: roda_log::content_hash_of(grant_event),
                    });
                }
            }
        }
        let intent = original.ok_or(NativeError::Permission)?;
        let (grant, grant_event) = selected.ok_or(NativeError::Permission)?;
        let Capability::Trust(level) = grant.grant.capability else {
            return Err(NativeError::Permission);
        };
        if !readable
            || !direct_writers(&ordered, space, &self.owner, &self.agent)
            || intent.grant != grant.grant.id
            || intent.grant_digest != roda_log::content_hash_of(grant_event)
            || grant
                .grant
                .expires_at_ms
                .is_some_and(|expiry| expiry <= at_ms)
            || !evaluate_permission(level, &ActionClass::Reply, &Policy::default()).acts()
        {
            return Err(NativeError::Permission);
        }
        if self
            .engine
            .state
            .spaces
            .get(space)
            .is_some_and(|space| space.integrity_error.is_some())
        {
            return Err(NativeError::NotReady);
        }
        let epoch = self
            .engine
            .ready_epoch_in(space, &ordered)
            .ok_or(NativeError::NotReady)?;
        let last = log.events().last().ok_or(NativeError::History)?;
        Ok(ReplyFacts {
            intent,
            frontier: NativeFrontier {
                seq: last.seq,
                hash: last.hash.clone(),
                epoch,
            },
            expires_at_ms: grant.grant.expires_at_ms,
            text,
        })
    }
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

fn latest_trust<'a>(
    state: &'a State,
    space: &str,
    owner: &str,
    agent: &str,
) -> Option<&'a GrantState> {
    // A revoked newest grant cannot revive an earlier, more permissive grant.
    state
        .grants
        .iter()
        .rev()
        .find(|g| {
            g.grant.grantor == owner
                && g.grant.grantee.as_deref() == Some(agent)
                && g.grant.scope == GrantScope::Space(space.into())
                && matches!(g.grant.capability, Capability::Trust(_))
        })
        .filter(|g| !g.revoked)
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

fn validate_image(store: &Store) -> Result<()> {
    let trusted = Engine::open(":memory:").map_err(|_| NativeError::Storage)?;
    let version: i64 = store
        .conn()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| NativeError::Image)?;
    if version != 2
        || schema(store)? != schema(&trusted.store)?
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
    limit_pages(store)
}

#[cfg(test)]
mod tests;
