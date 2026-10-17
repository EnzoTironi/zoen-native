//! # End-to-end Spaces on this device (ADR 0026)
//!
//! An end-to-end Space is relay-ordered like any other, but the relay holds ciphertext:
//!
//! - **Writing:** an event that isn't membership or a checkpoint is signed as usual and
//!   queued in the clear on the device (shown as *Sending*). It is sealed when it goes
//!   out, as an MLS application message inside an outer envelope the device signs too, at
//!   the group's current epoch, and only once the group is ready: this device has it, has
//!   no commit in flight, and owes no commit adding someone it has packages for. So the
//!   first message of a new chat reaches the person it was written to, and a message
//!   queued while the group moved on is sealed again for the new epoch. The sealed copy is
//!   kept, so a relaunch in the same epoch resends the same bytes.
//! - **Upgrading:** a relay-readable chat becomes end-to-end with `SpaceEncrypted`. The
//!   author's device starts the group when the relay confirms it, and reconciles everyone
//!   in. History before it stays as it was; nothing after it is readable to the relay.
//! - **Reading:** sealed entries are opened in log order, in one SQLite transaction with
//!   the log append, so group state and log never disagree. The device log keeps the inner
//!   signed event of each message it opened (bound to the chain by the outer hash), and
//!   the outer envelope of everything it keeps sealed: handshakes, and ciphertext from
//!   before it joined. Either way the chain and every signature re-verify from disk.
//! - **Membership:** the log decides who's in; MLS follows. An owner or admin's device
//!   reconciles: whoever the log lists that the group lacks is added with claimed key
//!   packages, whoever the group holds that the log doesn't list is removed, in one
//!   commit. Members apply a commit only if it leaves no unlisted reader.
//! - **Checkpoints:** after its commit lands, after joining, and every 256 sealed entries,
//!   a device posts its epoch digest. A member whose digest for the same epoch differs
//!   was shown another group: the Space is marked as forked. A checkpoint whose
//!   `upto.hash` is not this device's chain hash at `upto.seq` marks that same fork.
//!   A checkpoint still ahead of the chain waits under `mls.pending_prefix`,
//!   one claim per signing device. A signed `seen` that names a held position
//!   with a different hash marks the same fork and is not appended.
//!   A member event that is already the next sequence, whose signed `seen`
//!   is not a held position, marks that same fork and is not fetched again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use roda_log::content::{InnerEvent, Sealed, SealedKind};
use roda_log::{event_from_content, SpaceLog};
use roda_mls::{leaf_name, sealed::state_key, Device, Leaf, MlsError, Opened, SUITE_ID};
use roda_proto::{
    Envelope, KeyPackageClaimClock, KeyPackageRecord, Sequenced, KEY_PACKAGE_CLAIM_EXPIRED,
};
use roda_types::*;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::i18n::t;
use crate::net::tracing_like;

fn claim_operation(clock: KeyPackageClaimClock) -> R<String> {
    // Event ULIDs intentionally clamp to this device's wall clock. Consumptive
    // claim receipts instead use an exact relay sample and independent entropy.
    let mut random = [0u8; 16];
    getrandom::getrandom(&mut random).map_err(storage)?;
    let mut value = ((clock.milliseconds() as u128) << 80)
        | (u128::from_be_bytes(random) & ((1u128 << 80) - 1));
    let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut encoded = [0u8; 26];
    for c in encoded.iter_mut().rev() {
        *c = alphabet[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(encoded.to_vec()).map_err(storage)
}
use crate::sync::Ingest;
use crate::CoreError;

type R<T> = Result<T, CoreError>;

/// Single-use key packages a device keeps on the relay, beside its last-resort one.
const KEY_PACKAGES: usize = 32;

/// Defense ceiling on pending prefix claims in one Space. A valid roster stays
/// under this, because each member device holds one claim. Reaching it refuses
/// the new device and drops nothing already stored.
const PENDING_PREFIX_CEILING: usize = 4096;
const META_PUBLISHED: &str = "mls.key_packages";
/// Log entries between a device's checkpoints in a Space (ADR 0026). Pruning waits on the
/// slowest member's checkpoint, so readers post them too. `ZOEN_CHECKPOINT_EVERY` changes it
/// (journeys use a small one to reach pruning in a few messages).
fn checkpoint_every() -> u64 {
    static EVERY: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *EVERY.get_or_init(|| {
        std::env::var("ZOEN_CHECKPOINT_EVERY")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(256)
    })
}
/// How often a device in use checkpoints even with nothing new (at its next sync), so the
/// relay's pruning ceiling (30 days) only ever passes devices that stopped coming back.
/// `ZOEN_CHECKPOINT_REFRESH_SECS` changes it.
fn checkpoint_refresh_ms() -> i64 {
    static REFRESH: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *REFRESH.get_or_init(|| {
        std::env::var("ZOEN_CHECKPOINT_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(24 * 3600)
            * 1000
    })
}
/// How long to wait before claiming again for someone who had no key packages.
const CLAIM_RETRY: Duration = Duration::from_secs(5);
/// How long each admin waits behind the one before it (by identity order) before
/// committing a change, so a group's admins rarely race for the same epoch.
const ADMIN_STAGGER: Duration = Duration::from_millis(400);

/// Key packages on their way to the relay: single-use ones, and a last-resort one with the
/// first set.
pub type KeyPackageBatch = (Vec<Vec<u8>>, Option<Vec<u8>>);

/// MLS bookkeeping beside the network state.
#[derive(Default)]
pub struct MlsNet {
    /// Key packages generated and on their way to the relay.
    publishing: Option<KeyPackageBatch>,
    /// Single-use packages the relay says this device should add (it's running low).
    top_up: usize,
    /// Who each end-to-end Space's confirmed log lists, and as what.
    rosters: HashMap<SpaceId, BTreeMap<IdentityId, Role>>,
    /// Spaces whose group may owe a commit (membership changed, a commit landed or was
    /// refused, a claim came back).
    dirty: HashSet<SpaceId>,
    claiming: HashSet<SpaceId>,
    retry_at: HashMap<SpaceId, Instant>,
    checkpoint_due: HashSet<SpaceId>,
    /// Spaces where the relay said a message was sealed at an epoch the group has left, with
    /// the epoch this device was at: nothing is sealed there until a commit moves it on.
    behind: HashMap<SpaceId, u64>,
    /// Where this device's last checkpoint in each Space reached (`upto.seq`), from the
    /// device database; it survives restarts so a reader still checkpoints.
    checkpointed_at: HashMap<SpaceId, u64>,
    /// Spaces where someone listed had no key packages: messages stop waiting for them.
    stuck: HashSet<SpaceId>,
    /// When this device's turn to commit comes, behind the admins before it.
    turn_at: HashMap<SpaceId, Instant>,
    /// Spaces where this device already waited its turn.
    waited: HashSet<SpaceId>,
    /// Spaces this device must ask to rejoin (its group fell behind pruned history), once
    /// it has caught up with the log.
    rejoin: HashSet<SpaceId>,
    pub(crate) recovery_retry_at: HashMap<SpaceId, Instant>,
    recovery_dirty: Mutex<HashSet<SpaceId>>,
}

impl MlsNet {
    pub(crate) fn wake_recovery(&self, space: &str) {
        self.recovery_dirty
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(space.into());
    }

    pub(crate) fn wake_recovery_many(&self, spaces: impl IntoIterator<Item = SpaceId>) {
        self.recovery_dirty
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .extend(spaces);
    }

    pub(crate) fn has_recovery_work(&self) -> bool {
        !self
            .recovery_dirty
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_empty()
    }

    pub(crate) fn take_recovery_work(&self) -> Vec<SpaceId> {
        self.recovery_dirty
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain()
            .collect()
    }

    fn mark_group_dirty(&mut self, space: SpaceId) {
        self.wake_recovery(&space);
        self.dirty.insert(space);
    }
}

/// Events the relay reads even in an end-to-end Space: what it orders and authorizes by.
fn stays_clear(body: &EventBody) -> bool {
    matches!(
        body,
        EventBody::SpaceCreated { .. }
            | EventBody::MemberAdded { .. }
            | EventBody::MemberRemoved { .. }
            | EventBody::ProfileKeyShared { .. }
            | EventBody::SpaceEncrypted
            | EventBody::Checkpoint { .. }
            | EventBody::DeviceJoining { .. }
    )
}

fn sealed_meta(client_id: &str) -> String {
    format!("mls.sealed:{client_id}")
}

/// Who a queued Welcome of ours is for, until it lands.
fn welcome_meta(client_id: &str) -> String {
    format!("mls.welcome:{client_id}")
}

/// People in our group whose Welcome the relay refused: they hold (or will hold, once the
/// commit lands) a leaf they can never use, so the group takes it out and adds them again.
fn rewelcome_meta(space: &str) -> String {
    format!("mls.rewelcome:{space}")
}

fn claim_meta(space: &str) -> String {
    format!("mls.claim:{space}")
}

/// Persisted before a destructive relay request, then with its exact reply before MLS
/// staging. Removing it shares the transaction that stages the commit and Welcome.
#[derive(Serialize, Deserialize)]
struct ClaimOperation {
    operation_id: String,
    device: String,
    epoch: u64,
    intent: Owed,
    targets: Vec<IdentityId>,
    records: Option<Vec<KeyPackageRecord>>,
}

fn identity_set(v: Option<String>) -> BTreeSet<IdentityId> {
    v.map(|v| {
        v.split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn join_set(set: &BTreeSet<IdentityId>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(",")
}

fn checkpointed_meta(space: &str) -> String {
    format!("mls.checkpointed:{space}")
}

/// When this device last checkpointed in a Space (ms).
fn checkpointed_ms_meta(space: &str) -> String {
    format!("mls.checkpointed_ms:{space}")
}

/// Devices that need a fresh leaf in a Space's group (`DeviceJoining`): leaf name -> the
/// encryption key (hex) of the leaf they had when the event landed, "" if none. Done once
/// the group holds that device with another key.
fn joins_meta(space: &str) -> String {
    format!("mls.joins:{space}")
}

use roda_mls::reconciliation::{self, Owed};

impl MlsNet {
    pub(crate) fn mark_dirty(&mut self, spaces: Vec<SpaceId>) {
        self.wake_recovery_many(spaces.iter().cloned());
        self.dirty.extend(spaces);
    }
}

fn split_leaf(name: &str) -> Option<(&str, &str)> {
    name.split_once('/')
}

fn digest_meta(space: &str, epoch: u64) -> String {
    format!("mls.digest:{space}:{epoch}")
}

fn fork_meta(space: &str) -> String {
    format!("mls.fork:{space}")
}

fn pending_prefix_meta(space: &str) -> String {
    format!("mls.pending_prefix:{space}")
}

fn pending_refusal_meta(space: &str) -> String {
    format!("mls.pending_prefix_refused:{space}")
}

fn fork_reason() -> String {
    t(
        "Membros veem grupos diferentes nesta conversa (possível bifurcação do servidor).",
        "Members see different groups in this chat (the server may have forked it).",
    )
}

#[cfg(test)]
thread_local! {
    static CRASH_AFTER_FORK_META: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn crash_after_fork_meta() -> bool {
    CRASH_AFTER_FORK_META.with(std::cell::Cell::get)
}

#[cfg(test)]
fn set_crash_after_fork_meta(crash: bool) {
    CRASH_AFTER_FORK_META.with(|flag| flag.set(crash));
}

/// How a signed `seen` sits against the log this device holds.
pub(crate) enum SeenAgainst {
    /// `seen.seq` is not a position in the log.
    Ahead,
    /// The position is held and its hash is not `seen.hash`.
    Diverged,
    /// No signed `seen`, or the held hash matches.
    Other,
}

pub(crate) fn seen_against(log: &SpaceLog, e: &Event) -> SeenAgainst {
    let Some(seen) = &e.seen else {
        return SeenAgainst::Other;
    };
    match log.events().get(seen.seq as usize) {
        Some(held) if held.seq == seen.seq && held.hash != seen.hash => SeenAgainst::Diverged,
        Some(held) if held.seq == seen.seq => SeenAgainst::Other,
        _ => SeenAgainst::Ahead,
    }
}

/// The key that signed the event: the device key when the event carries one,
/// otherwise the identity key.
fn signing_device(e: &Event) -> &str {
    e.device.as_deref().unwrap_or(e.author.as_str())
}

/// A foreign checkpoint whose `upto.seq` this device has not reached yet.
/// Stored under the signing device's key, one claim per device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingPrefix {
    seq: u64,
    hash: String,
    epoch: u64,
    digest: String,
}

fn storage(e: impl std::fmt::Display) -> CoreError {
    CoreError::Storage {
        message: e.to_string(),
    }
}

pub(crate) fn mls_err(e: MlsError) -> CoreError {
    match e {
        MlsError::Storage(message) => CoreError::Storage { message },
        other => CoreError::Invalid {
            reason: other.to_string(),
        },
    }
}

/// Folds a log's membership: the creator owns it, then adds and removals in order.
fn fold_roster<'a>(events: impl Iterator<Item = &'a Event>) -> BTreeMap<IdentityId, Role> {
    let mut roster = BTreeMap::new();
    for e in events {
        match &e.body {
            EventBody::SpaceCreated { .. } => {
                roster.insert(e.author.clone(), Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                roster.insert(identity.clone(), *role);
            }
            EventBody::MemberRemoved { identity } => {
                roster.remove(identity);
            }
            _ => {}
        }
    }
    roster
}

impl Engine {
    /// Whether `space` is relay-ordered and end-to-end encrypted.
    pub(crate) fn is_e2e(&self, space: &str) -> bool {
        self.net.synced.contains(space)
            && self
                .state
                .spaces
                .get(space)
                .is_some_and(|s| s.privacy == Privacy::EndToEnd && s.kind != SpaceKind::Personal)
    }

    /// This device as an MLS client over `conn` (the store, or a transaction on it).
    pub(crate) fn device_on<'c>(&self, conn: &'c Connection) -> R<Device<'c>> {
        let (Some(author), Some(acct)) = (&self.net.author, &self.net.account) else {
            return Err(CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            });
        };
        let secret = author.key.secret();
        Device::new(conn, state_key(&secret), &acct.identity, secret, &acct.cert).map_err(mls_err)
    }

    pub(crate) fn device(&self) -> R<Device<'_>> {
        self.device_on(self.store.conn())
    }

    /// Creates this device's tables for MLS state. Runs at every open.
    pub(crate) fn migrate_mls(&mut self) -> R<()> {
        roda_mls::migrate(self.store.conn_mut()).map_err(mls_err)
    }

    /// Rebuilds the membership cache from the verified logs and re-checks every group.
    pub(crate) fn reload_mls(&mut self) {
        let spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.is_e2e(s))
            .cloned()
            .collect();
        self.net.mls.rosters.clear();
        for space in spaces {
            let roster = self
                .logs
                .get(&space)
                .map(|l| fold_roster(l.events().iter()))
                .unwrap_or_default();
            self.net.mls.rosters.insert(space.clone(), roster);
            if let Ok(Some(reason)) = self.store.meta(&fork_meta(&space)) {
                self.show_fork(&space, reason);
            }
            self.net.mls.mark_group_dirty(space);
        }
        let pending: Vec<SpaceId> = self.logs.keys().cloned().collect();
        for space in pending {
            let _ = self.settle_pending_prefixes(&space);
        }
    }

    pub(crate) fn mls_roster(&self, space: &str) -> Option<&BTreeMap<IdentityId, Role>> {
        self.net.mls.rosters.get(space)
    }

    /// The group's leaves as (identity, device), if this device is in it.
    pub(crate) fn mls_leaves(&self, space: &str) -> Option<BTreeSet<(IdentityId, String)>> {
        let d = self.device().ok()?;
        if !d.has_group(space) {
            return None;
        }
        d.leaves(space).ok().map(|l| l.into_keys().collect())
    }

    pub(crate) fn roster(&self, space: &str) -> BTreeSet<IdentityId> {
        self.net
            .mls
            .rosters
            .get(space)
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// A membership event landed in an end-to-end Space's confirmed log.
    pub(crate) fn mls_membership_changed(&mut self, e: &Event) {
        if !self.is_e2e(&e.space) {
            return;
        }
        let roster = self.net.mls.rosters.entry(e.space.clone()).or_default();
        match &e.body {
            EventBody::SpaceCreated { .. } => {
                roster.insert(e.author.clone(), Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                roster.insert(identity.clone(), *role);
                self.net.mls.stuck.remove(&e.space);
            }
            EventBody::MemberRemoved { identity } => {
                roster.remove(identity);
                if self.me.as_deref() == Some(identity.as_str()) {
                    // The relay stops sending this Space here, so the commit removing this
                    // device never arrives: forget the group now.
                    if let Err(err) = self
                        .device()
                        .and_then(|d| d.forget(&e.space).map_err(mls_err))
                    {
                        tracing_like(&format!("forgetting the group of {}: {err}", e.space));
                    }
                }
            }
            EventBody::SpaceEncrypted => {
                *roster = self
                    .logs
                    .get(&e.space)
                    .map(|l| fold_roster(l.events().iter()))
                    .unwrap_or_default();
                if self.me.as_deref() == Some(e.author.as_str()) {
                    let started = self.device().and_then(|d| match d.has_group(&e.space) {
                        true => Ok(()),
                        false => d.create_group(&e.space).map_err(mls_err),
                    });
                    if let Err(err) = started {
                        tracing_like(&format!("starting the group of {}: {err}", e.space));
                    }
                }
            }
            _ => return,
        }
        self.net.mls.mark_group_dirty(e.space.clone());
    }

    /// The group for a Space this device just created (it's the only member).
    pub(crate) fn create_mls_group(&mut self, space: &str) -> R<()> {
        self.device()?.create_group(space).map_err(mls_err)?;
        self.net.mls.mark_group_dirty(space.to_string());
        Ok(())
    }

    /// The epoch to seal `space`'s messages at, once its group is ready for them.
    pub(crate) fn ready_epoch(&self, space: &str) -> Option<u64> {
        self.ready_epoch_in(space, &self.state)
    }

    pub(crate) fn ready_epoch_in(&self, space: &str, state: &crate::engine::State) -> Option<u64> {
        let device = self.device().ok()?;
        if !device.has_group(space) || device.pending(space) || device.needs_reconciliation(space) {
            return None;
        }
        // The commit can land before its Welcome is accepted (e.g. under a publish
        // limit). The new member cannot open messages that overtake that Welcome.
        if self
            .store
            .outbox_handshakes()
            .ok()?
            .iter()
            .any(|(_, s, kind)| s == space && kind == SealedKind::Welcome.name())
        {
            return None;
        }
        let me = self.me.as_deref()?;
        let s = state.spaces.get(space)?;
        let group = device.roster(space).ok()?;
        // Nobody seals for a group that still holds someone the log removed: wait for the
        // commit that takes them out.
        if group.iter().any(|g| !s.members.iter().any(|(m, _)| m == g)) {
            return None;
        }
        // An owner or admin about to add someone (including adds still on their way to
        // the relay) sends after the commit, so the newcomer reads it.
        let runs_it = s
            .members
            .iter()
            .any(|(m, r)| m == me && matches!(r, Role::Owner | Role::Admin));
        let m = &self.net.mls;
        let epoch = device.epoch(space).ok()?;
        if m.behind.get(space) == Some(&epoch) {
            return None;
        }
        let adding =
            m.claiming.contains(space) || s.members.iter().any(|(who, _)| !group.contains(who));
        if runs_it && adding && !m.stuck.contains(space) {
            return None;
        }
        device.epoch(space).ok()
    }

    /// The epoch a queued event was sealed at, without reading its sealed bytes.
    fn sealed_epoch(&self, e: &Event) -> Option<u64> {
        self.sealed_epoch_of(&e.client_id)
    }

    fn sealed_epoch_of(&self, client_id: &str) -> Option<u64> {
        self.store
            .conn()
            .query_row(
                "SELECT substr(value, 1, instr(value, ':') - 1) FROM meta WHERE key = ?1",
                [sealed_meta(client_id)],
                |r| r.get::<_, String>(0),
            )
            .ok()?
            .parse()
            .ok()
    }

    /// The historical sealed copy, without filtering by the group's current epoch.
    pub(crate) fn sealed_copy(&self, e: &Event) -> Option<(u64, Envelope)> {
        let v = self.store.meta(&sealed_meta(&e.client_id)).ok().flatten()?;
        let mut parts = v.splitn(3, ':');
        let epoch = parts.next()?.parse().ok()?;
        let content = hex::decode(parts.next()?).ok()?;
        let env = Envelope::new(content, parts.next()?.to_string(), e.cert.clone(), None)?;
        Some((epoch, env))
    }

    /// Seals every queued message whose group is ready and that has no copy for the
    /// group's current epoch. `true` when something new can go out.
    pub fn mls_seal_outbox(&mut self) -> R<bool> {
        if self.net.pending.is_empty() {
            return Ok(false);
        }
        // Runs after every write: look at the queue without decoding it, and read only the
        // events that still need sealing for their group's current epoch.
        let heads = self.store.outbox_heads()?;
        let mut ready: HashMap<SpaceId, Option<u64>> = HashMap::new();
        let mut sealed = false;
        for h in heads {
            if h.failed || !self.is_e2e(&h.space) {
                continue;
            }
            let epoch = *ready
                .entry(h.space.clone())
                .or_insert_with(|| self.ready_epoch(&h.space));
            let Some(epoch) = epoch else { continue };
            if self.sealed_epoch_of(&h.client_id) == Some(epoch) {
                continue;
            }
            let Some(e) = self.store.outbox_get(&h.client_id)? else {
                continue;
            };
            if !must_seal(&e.body) {
                continue;
            }
            self.seal_outgoing(&e, epoch)?;
            sealed = true;
        }
        Ok(sealed)
    }

    fn seal_outgoing(&mut self, e: &Event, epoch: u64) -> R<()> {
        let author = self
            .net
            .author
            .clone()
            .ok_or_else(|| CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            })?;
        let inner = InnerEvent {
            content: e.content.clone(),
            sig: e.sig.clone(),
        };
        let data = self
            .device()?
            .seal(&e.space, &inner.encode())
            .map_err(mls_err)?;
        let outer = Envelope::sealed(
            &author,
            &e.space,
            &e.client_id,
            e.at_ms,
            e.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, data),
        );
        self.store.set_meta(
            &sealed_meta(&e.client_id),
            &format!("{epoch}:{}:{}", hex::encode(outer.content()), outer.sig),
        )?;
        Ok(())
    }

    /// What goes out now for a queued event: its sealed copy for the current epoch, the
    /// handshake it is, or itself. `None` while it waits for its group.
    /// Each Space's ready epoch is looked up once per batch (`ready`).
    pub(crate) fn outgoing_envelope_with(
        &self,
        e: &Event,
        ready: &mut HashMap<SpaceId, Option<u64>>,
    ) -> Option<Envelope> {
        if matches!(e.body, EventBody::Sealed { .. }) {
            return Envelope::new(e.content.clone(), e.sig.clone(), e.cert.clone(), None);
        }
        if !(self.is_e2e(&e.space) && must_seal(&e.body)) {
            return Some(Envelope::plain(e));
        }
        let epoch = *ready
            .entry(e.space.clone())
            .or_insert_with(|| self.ready_epoch(&e.space));
        // Cheap check first: only decode the sealed copy when it is for this epoch.
        if epoch.is_none() || self.sealed_epoch(e) != epoch {
            return None;
        }
        let (at, env) = self.sealed_copy(e)?;
        (Some(at) == epoch).then_some(env)
    }

    /// Signs MLS bytes into the Space's outbox, in order.
    pub(crate) fn handshake_event(&self, space: &str, sealed: Sealed) -> R<Event> {
        let author = self
            .net
            .author
            .clone()
            .ok_or_else(|| CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            })?;
        let at_ms = crate::engine::now_ms();
        let client_id = new_ulid(at_ms);
        let seen = self.logs.get(space).and_then(|l| l.head());
        let (content, sig) = author.sign_sealed(space, &client_id, at_ms, seen.as_ref(), sealed);
        let e = event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .map_err(storage)?;
        Ok(e)
    }

    pub(crate) fn queued_recovery_commit(&mut self, events: Vec<Event>) {
        for e in events {
            self.net.pending.insert(e.client_id, e.space.clone());
            self.net.mls.mark_group_dirty(e.space);
        }
        self.net.wake();
    }

    /// A sealed entry the relay sequenced: opened (or kept sealed) and logged in one
    /// transaction with the group state it changes.
    pub(crate) fn ingest_sealed(&mut self, ev: Sequenced) -> Ingest {
        let space = ev.env.space().to_string();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if ev.seq < next {
            if self.approval_replay_needed {
                if let Err(error) = self.reproject_approval_overlay() {
                    return Ingest::Invalid(error.to_string());
                }
            }
            return Ingest::Duplicate;
        }
        if ev.seq > next {
            return Ingest::Gap { next };
        }
        // Opened/own events persist their inner signature. Authenticate the outer
        // envelope too before replacing it with that inner event or changing MLS state.
        if let Err(err) = ev.env.verify() {
            return Ingest::Invalid(err.to_string());
        }
        if ev.env.is_pruned() {
            return self.ingest_pruned(ev);
        }
        let Some((kind, data)) = ev.env.sealed_data().map(|(k, d)| (k, d.to_vec())) else {
            return Ingest::Invalid("not sealed".into());
        };
        let client_id = ev.env.client_id().to_string();
        let own = self.net.pending.get(&client_id) == Some(&space)
            && self.net.account.as_ref().is_some_and(|account| {
                account.identity == ev.env.author()
                    && ev.env.device() == Some(account.device.as_str())
            });
        let roster = self.roster(&space);
        let admitted_sender = roda_mls::Leaf {
            identity: ev.env.author().into(),
            device: ev.env.device().unwrap_or(ev.env.author()).into(),
            cert: ev.env.cert.clone().unwrap_or_default(),
        };
        let own_copy = if own && kind == SealedKind::Application {
            self.store.outbox_get(&client_id).ok().flatten()
        } else {
            None
        };

        let conn = self.store.conn();
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(e) => return Ingest::Invalid(e.to_string()),
        };
        let device = match self.device_on(&tx) {
            Ok(d) => d,
            Err(e) => return Ingest::Invalid(e.to_string()),
        };
        let mut opened: Option<Event> = own_copy;
        let mut opening = if opened.is_some() {
            crate::mls_opening::OpeningOutcome::OwnRetainedEcho
        } else {
            crate::mls_opening::OpeningOutcome::Opaque
        };
        let mut landed: Option<(u64, bool)> = None; // (epoch, joined)
        match kind {
            SealedKind::Application if opened.is_none() => {
                match device.open(&space, &data, &roster, &admitted_sender) {
                    Ok(Opened::Application { plaintext, from }) => {
                        match inner_event(&ev, &plaintext, &from) {
                            Some(e) => {
                                opening =
                                    crate::mls_opening::OpeningOutcome::from_application(&e, &from);
                                opened = Some(e);
                            }
                            None => tracing_like(&format!(
                                "sealed message in {space} doesn't match its envelope; kept sealed"
                            )),
                        }
                    }
                    Ok(_) => {}
                    Err(e @ roda_mls::MlsError::Storage(_)) => {
                        return Ingest::Invalid(e.to_string());
                    }
                    Err(e) => tracing_like(&format!("can't open a message in {space}: {e}")),
                }
            }
            SealedKind::Application => {}
            SealedKind::Commit => match if device.recovery_pending(&space) && own {
                device.confirm_recovery(&space, &data, &roster)
            } else {
                // A winning external commit can replace a tentative group at the same
                // epoch. Discard the loser before retaining the authenticated new context.
                if device.recovery_pending(&space) {
                    if let Err(err) = device.abandon_recovery(&space) {
                        return Ingest::Invalid(err.to_string());
                    }
                }
                device.open(&space, &data, &roster, &admitted_sender)
            } {
                Ok(Opened::Commit { epoch } | Opened::Reconcile { epoch }) => {
                    landed = Some((epoch, false))
                }
                Ok(_) => {}
                Err(e)
                    if own && device.recovery_pending(&space)
                        || matches!(e, roda_mls::MlsError::Storage(_)) =>
                {
                    return Ingest::Invalid(e.to_string());
                }
                Err(e) => tracing_like(&format!("refused a commit in {space}: {e}")),
            },
            SealedKind::Welcome if !device.has_group(&space) => {
                match device.join(&space, &data, &roster) {
                    Ok(true) => match device.epoch(&space) {
                        Ok(epoch) => landed = Some((epoch, true)),
                        Err(e @ roda_mls::MlsError::Storage(_)) => {
                            return Ingest::Invalid(e.to_string());
                        }
                        Err(e) => {
                            tracing_like(&format!("can't load a joined group in {space}: {e}"))
                        }
                    },
                    Ok(false) => {}
                    Err(e @ roda_mls::MlsError::Storage(_)) => {
                        return Ingest::Invalid(e.to_string());
                    }
                    Err(e) => tracing_like(&format!("refused a welcome in {space}: {e}")),
                }
            }
            SealedKind::Welcome | SealedKind::Unspecified => {}
        }
        let digest = if landed.is_some() {
            match device.checkpoint(&space) {
                Ok(checkpoint) => Some(checkpoint),
                Err(e @ roda_mls::MlsError::Storage(_)) => return Ingest::Invalid(e.to_string()),
                Err(_) => None,
            }
        } else {
            None
        };
        drop(device);

        let e = match opened {
            Some(mut inner) => {
                inner.seq = ev.seq;
                inner.prev = ev.prev.clone();
                inner.hash = ev.hash.clone();
                inner.sealed_wire = Some(ev.env.wire_hash());
                inner
            }
            None => match event_from_content(
                ev.env.content().to_vec(),
                ev.env.sig.clone(),
                ev.env.cert.clone(),
                ev.seq,
                ev.prev.clone(),
                ev.hash.clone(),
            ) {
                Ok(e) => e,
                Err(err) => return Ingest::Invalid(err.to_string()),
            },
        };
        let accept_err = {
            let log = self
                .logs
                .entry(space.clone())
                .or_insert_with(|| SpaceLog::new(space.clone()));
            log.accept(e.clone()).err()
        };
        if let Some(err) = accept_err {
            // The fork write uses this connection. It has to follow the rollback,
            // or the refused event takes the fork meta with it.
            drop(tx);
            return self.refuse_unaccepted(&space, &err, &e);
        }
        let mut stored = self.store.append_event(&e).map_err(storage);
        stored = stored
            .and_then(|_| crate::mls_opening::record(&self.store, &ev, opening).map_err(storage));
        stored = stored.and_then(|_| self.remember_recovery_context(&ev.env));
        if own && kind == SealedKind::Commit {
            if let Some(reference) = ev
                .env
                .recovery()
                .and_then(roda_log::recovery::RecoveryRef::parse)
            {
                stored = stored.and_then(|_| {
                    self.store
                        .clear_recovery_upload(&reference.blob)
                        .map_err(storage)
                });
            }
        }
        if let Some((epoch, digest)) = &digest {
            stored = stored.and_then(|_| {
                self.store
                    .set_meta(&digest_meta(&space, *epoch), digest)
                    .map_err(storage)
            });
        }
        if own {
            stored = stored.and_then(|_| {
                self.store.outbox_remove(&client_id).map_err(storage)?;
                self.store
                    .meta_delete(&sealed_meta(&client_id))
                    .map_err(storage)?;
                self.store
                    .meta_delete(&welcome_meta(&client_id))
                    .map_err(storage)
            });
        }
        if let Err(err) = stored.and_then(|_| tx.commit().map_err(storage)) {
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        if let Err(err) = self.compare_checkpoint(&e) {
            return Ingest::Invalid(err.to_string());
        }

        let _ = self.store.set_synced(&space);
        self.net.synced.insert(space.clone());
        let m = &mut self.net.mls;
        if landed.is_some() {
            m.mark_group_dirty(space.clone());
        }
        if matches!(landed, Some((_, true))) || (own && landed.is_some()) {
            m.checkpoint_due.insert(space.clone());
        }
        if kind == SealedKind::Commit && landed.is_none() {
            // Refused or stale: the group may still owe this Space a commit.
            m.mark_group_dirty(space.clone());
        }
        if e.seq >= self.checkpointed_at(&space) + checkpoint_every()
            && self.device().is_ok_and(|d| d.has_group(&space))
        {
            self.net.mls.checkpoint_due.insert(space.clone());
        }

        if self.approval_projection_needs_replay(&e) {
            if own && self.confirmed_item_already_projected(&e) {
                self.net.pending.remove(&client_id);
                if !self.note_own_confirmed(&space, &client_id, e.seq, &e.hash) {
                    self.state =
                        Self::merge_space_projections(&self.space_projections, &self.space_order);
                }
                return Ingest::Confirmed;
            }
            if own {
                self.net.pending.remove(&client_id);
            }
            if let Err(err) = self.reproject_approval_overlay() {
                return Ingest::Invalid(err.to_string());
            }
            return if own {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
        }

        if own {
            self.net.pending.remove(&client_id);
            if kind == SealedKind::Welcome {
                // The group may owe a commit that waited for this Welcome to land.
                self.net.mls.mark_group_dirty(space.clone());
            }
            if !self.note_own_confirmed(&space, &client_id, e.seq, &e.hash) {
                self.state =
                    Self::merge_space_projections(&self.space_projections, &self.space_order);
            }
            return Ingest::Confirmed;
        }
        if let Err(err) = self.apply_confirmed_event(&e) {
            return Ingest::Invalid(err.to_string());
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        Ingest::Applied
    }

    /// What a pruned entry leaves (ADR 0026): only a device that joined later reads it, and
    /// it predates its Welcome, so there is nothing to open. It keeps the chain whole: the
    /// stub hashes and verifies like the original (header plus the MLS bytes' hash).
    fn ingest_pruned(&mut self, ev: Sequenced) -> Ingest {
        let space = ev.env.space().to_string();
        let mut e = match event_from_content(
            ev.env.content().to_vec(),
            ev.env.sig.clone(),
            ev.env.cert.clone(),
            ev.seq,
            ev.prev.clone(),
            ev.hash.clone(),
        ) {
            Ok(e) => e,
            Err(err) => return Ingest::Invalid(err.to_string()),
        };
        e.sealed_wire = ev.env.legacy_pruned_hash().map(str::to_string);
        let accept_err = {
            let log = self
                .logs
                .entry(space.clone())
                .or_insert_with(|| SpaceLog::new(space.clone()));
            log.accept(e.clone()).err()
        };
        if let Some(err) = accept_err {
            return self.refuse_unaccepted(&space, &err, &e);
        }
        // No MLS state changes until the stub's signature and chain are accepted.
        // Forgetting a stale group and persisting the verified stub commit together.
        let stored = (|| -> R<bool> {
            let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
            let forgot = match self.device_on(&tx) {
                Ok(device) if device.has_group(&space) => {
                    device.forget(&space).map_err(mls_err)?;
                    true
                }
                _ => false,
            };
            self.store.append_event(&e).map_err(storage)?;
            self.remember_recovery_context(&ev.env)?;
            tx.commit().map_err(storage)?;
            Ok(forgot)
        })();
        let forgot = match stored {
            Ok(forgot) => forgot,
            Err(err) => {
                let _ = self.reload();
                return Ingest::Invalid(err.to_string());
            }
        };
        if forgot {
            // History this device never fetched is gone; a fresh Welcome is needed.
            self.net.mls.rejoin.insert(space.clone());
        }
        let _ = self.store.set_synced(&space);
        self.net.mls.wake_recovery(&space);
        self.net.synced.insert(space);
        if let Err(err) = self.compare_checkpoint(&e) {
            return Ingest::Invalid(err.to_string());
        }
        Ingest::Applied
    }

    fn joins(&self, space: &str) -> BTreeMap<String, String> {
        self.store
            .meta(&joins_meta(space))
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    }

    fn set_joins(&self, space: &str, joins: &BTreeMap<String, String>) {
        let _ = if joins.is_empty() {
            self.store.meta_delete(&joins_meta(space))
        } else {
            self.store.set_meta(
                &joins_meta(space),
                &serde_json::to_string(joins).unwrap_or_default(),
            )
        };
    }

    /// A `DeviceJoining` landed: remember which leaf (if any) the device had at this point
    /// of the log, so every member agrees on when it has a fresh one.
    pub(crate) fn mls_device_joining(&mut self, e: &Event) {
        let EventBody::DeviceJoining { device } = &e.body else {
            return;
        };
        if !self.is_e2e(&e.space) {
            return;
        }
        let mine = self.net.account.as_ref().map(|a| a.device.as_str());
        if mine == Some(device.as_str()) {
            return;
        }
        let Ok(leaves) = self
            .device()
            .and_then(|d| d.leaves(&e.space).map_err(mls_err))
        else {
            return;
        };
        let stale = leaves
            .get(&(e.author.clone(), device.clone()))
            .map(hex::encode)
            .unwrap_or_default();
        let mut joins = self.joins(&e.space);
        joins.insert(leaf_name(&e.author, device), stale);
        self.set_joins(&e.space, &joins);
        self.net.mls.mark_group_dirty(e.space.clone());
    }

    /// Drops joins that are done: the device has a leaf with a new key, or its identity
    /// left. Nothing settles while our own commit is still on its way.
    fn settle_joins(&mut self, space: &str) {
        let mut joins = self.joins(space);
        if joins.is_empty() {
            return;
        }
        let Ok(device) = self.device() else {
            return;
        };
        if device.pending(space) {
            return;
        }
        let Ok(leaves) = device.leaves(space) else {
            return;
        };
        drop(device);
        let roster = self.net.mls.rosters.get(space).cloned().unwrap_or_default();
        joins.retain(|name, stale| {
            let Some((id, dev)) = split_leaf(name) else {
                return false;
            };
            if !roster.contains_key(id) {
                return false;
            }
            match leaves.get(&(id.to_string(), dev.to_string())) {
                Some(key) => hex::encode(key) == *stale,
                None => true,
            }
        });
        self.set_joins(space, &joins);
    }

    /// After a catch-up: asks for a fresh leaf in each group this device fell out of.
    pub fn mls_ask_rejoin(&mut self) -> R<()> {
        let spaces: Vec<SpaceId> = self.net.mls.rejoin.drain().collect();
        let Some(device) = self.net.account.as_ref().map(|a| a.device.clone()) else {
            return Ok(());
        };
        let me = self.me_id()?;
        for space in spaces {
            let listed = self
                .net
                .mls
                .rosters
                .get(&space)
                .is_some_and(|r| r.contains_key(&me));
            if listed {
                self.append(
                    &space,
                    &me,
                    EventBody::DeviceJoining {
                        device: device.clone(),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Checkpoints this device owes for being alive: in every group it is in, once a
    /// refresh interval has passed since its last one (the relay's ceiling counts from it).
    pub fn mls_refresh_checkpoints(&mut self) {
        let now = crate::engine::now_ms();
        let spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.is_e2e(s))
            .cloned()
            .collect();
        for space in spaces {
            if !self.device().is_ok_and(|d| d.has_group(&space)) {
                continue;
            }
            let last: Option<i64> = self
                .store
                .meta(&checkpointed_ms_meta(&space))
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok());
            // A group this device never checkpointed starts its clock now: its hold (from
            // when it was added) is fresh, and a device in many new groups must not
            // checkpoint them all at its next launch.
            let Some(last) = last else {
                let _ = self
                    .store
                    .set_meta(&checkpointed_ms_meta(&space), &now.to_string());
                continue;
            };
            if now - last >= checkpoint_refresh_ms() {
                self.net.mls.checkpoint_due.insert(space);
            }
        }
    }

    /// Another member's checkpoint. Two checks share one fork path.
    ///
    /// The epoch digest is compared with this device's `digest_meta` for that epoch,
    /// as before. `upto.hash` is compared with the chain hash at `upto.seq` once that
    /// position is in the log. Either mismatch stores `fork_meta` and the Space's
    /// `integrity_error`. A checkpoint whose author is not a current member of the
    /// Space log is ignored.
    ///
    /// When `upto.seq` is still ahead of the chain, the checkpoint is kept under
    /// `mls.pending_prefix:{space}` and checked again from here when a later event in
    /// that Space reaches it, including after this device reopens. One signing device
    /// keeps only its highest unreached `upto.seq`. A later claim from that device
    /// with a higher seq replaces it. A lower or equal seq is ignored. This relies on
    /// the hash chain: the hash at the higher position commits to every earlier one,
    /// so a divergence at or before the lower position still fails the higher check.
    /// A device that leaves the roster keeps the claim it already stored until that
    /// check runs. The list is bounded by the Space's member devices. Past
    /// [`PENDING_PREFIX_CEILING`] entries a new device is refused. The refusal count
    /// is `mls.pending_prefix_refused:{space}`, read by [`Engine::pending_prefix_refusals`].
    /// Nothing already stored is dropped. Settling writes `fork_meta` and then the
    /// pending list in one store transaction, so a crash keeps the fork or the claim.
    /// Once the chain includes the seq, the entry is removed whether the prefix
    /// matches or forks. A match with no stored digest for the epoch is removed too.
    /// The digest check does not wait for a later epoch record.
    ///
    /// A checkpoint that never reaches this function can still be fork evidence.
    /// `seen` is protobuf field 7 of the signed content (`seq` and `hash`). The
    /// author signature covers those bytes. The relay's `seq`, `prev` and chain hash
    /// do not. When that signed `seen` names a position this device holds with a
    /// different hash, ingest marks the fork and leaves the event out of the log.
    /// A member event that is already the next sequence is the same fork when
    /// that `seen` is not held: the relay assigns the sequence after the author
    /// signed, so an honest `seen` on this chain is below the event. An event
    /// whose own sequence is still ahead of the local head stays a gap.
    pub(crate) fn compare_checkpoint(&mut self, e: &Event) -> R<()> {
        if let EventBody::Checkpoint {
            epoch,
            digest,
            upto,
        } = &e.body
        {
            if self.me.as_deref() != Some(e.author.as_str()) {
                self.observe_checkpoint(e, *epoch, digest, upto)?;
            }
        }
        self.settle_pending_prefixes(&e.space)
    }

    /// One decision for a failed `accept` on the next sequence, whether the
    /// body arrived in the clear, sealed, or pruned. The caller already
    /// returned a gap when the event's own sequence is still ahead.
    pub(crate) fn refuse_unaccepted(
        &mut self,
        space: &str,
        err: &roda_log::LogError,
        event: &Event,
    ) -> Ingest {
        let (seen, empty) = {
            let log = self.logs.get(space).expect("accept ran on this log");
            (seen_against(log, event), log.is_empty())
        };
        if empty {
            self.logs.remove(space);
        }
        let causal = matches!(err, roda_log::LogError::BrokenCausalLink { .. });
        self.refused_member_history(space, causal, seen, &event.author, &err.to_string())
    }

    /// `causal` means the signature and the event's own chain link already
    /// passed, and the signed `seen` did not. The event is this device's next
    /// sequence. A member whose `seen` disagrees with a held position, or names
    /// a position this chain does not hold, is fork evidence. Anything else is
    /// a refusal. Neither result asks the relay for this event again.
    pub(crate) fn refused_member_history(
        &mut self,
        space: &str,
        causal: bool,
        seen: SeenAgainst,
        author: &str,
        err: &str,
    ) -> Ingest {
        if !causal {
            return Ingest::Invalid(err.to_string());
        }
        let fork = matches!(seen, SeenAgainst::Ahead | SeenAgainst::Diverged)
            && self.author_in_roster(space, author);
        if fork {
            if let Err(error) = self.mark_checkpoint_fork(space) {
                return Ingest::Invalid(error.to_string());
            }
        }
        Ingest::Invalid(err.to_string())
    }

    fn observe_checkpoint(&mut self, e: &Event, epoch: u64, digest: &str, upto: &Seen) -> R<()> {
        if !self.author_in_roster(&e.space, &e.author) {
            return Ok(());
        }
        if self.digest_forks(&e.space, epoch, digest) || self.prefix_forks(&e.space, upto) {
            return self.mark_checkpoint_fork(&e.space);
        }
        if self.chain_hash_at(&e.space, upto.seq).is_none() {
            self.remember_pending_prefix(
                &e.space,
                signing_device(e),
                PendingPrefix {
                    seq: upto.seq,
                    hash: upto.hash.clone(),
                    epoch,
                    digest: digest.to_string(),
                },
            );
        }
        Ok(())
    }

    fn settle_pending_prefixes(&mut self, space: &str) -> R<()> {
        let pending = self.pending_prefixes(space);
        if pending.is_empty() {
            return Ok(());
        }
        let started = pending.len();
        let mut left = BTreeMap::new();
        let mut fork = false;
        for (device, item) in pending {
            let Some(mine) = self.chain_hash_at(space, item.seq) else {
                left.insert(device, item);
                continue;
            };
            if mine != item.hash || self.digest_forks(space, item.epoch, &item.digest) {
                fork = true;
            }
        }
        if !fork && left.len() == started {
            return Ok(());
        }
        self.persist_settlement(space, &left, fork)?;
        if fork {
            self.show_fork(space, fork_reason());
        }
        Ok(())
    }

    fn persist_settlement(
        &self,
        space: &str,
        pending: &BTreeMap<String, PendingPrefix>,
        fork: bool,
    ) -> R<()> {
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        if fork {
            self.store
                .set_meta(&fork_meta(space), &fork_reason())
                .map_err(storage)?;
            #[cfg(test)]
            if crash_after_fork_meta() {
                tx.commit().map_err(storage)?;
                return Err(storage("fork meta committed before pending removal"));
            }
        }
        let key = pending_prefix_meta(space);
        if pending.is_empty() {
            self.store.meta_delete(&key).map_err(storage)?;
        } else {
            let json = serde_json::to_string(pending).map_err(storage)?;
            self.store.set_meta(&key, &json).map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(())
    }

    fn remember_pending_prefix(&mut self, space: &str, device: &str, item: PendingPrefix) {
        let mut pending = self.pending_prefixes(space);
        if let Some(existing) = pending.get(device) {
            if item.seq <= existing.seq {
                return;
            }
            pending.insert(device.to_string(), item);
            self.store_pending_prefixes(space, &pending);
            return;
        }
        if pending.len() >= PENDING_PREFIX_CEILING {
            self.note_prefix_refusal(space, device, item.seq);
            return;
        }
        pending.insert(device.to_string(), item);
        self.store_pending_prefixes(space, &pending);
    }

    fn note_prefix_refusal(&self, space: &str, device: &str, seq: u64) {
        let count = self.pending_prefix_refusals(space).saturating_add(1);
        let _ = self
            .store
            .set_meta(&pending_refusal_meta(space), &count.to_string());
        tracing_like(&format!(
            "pending checkpoint prefix refused space={space} device={device} seq={seq} ceiling={PENDING_PREFIX_CEILING} count={count}"
        ));
    }

    /// How many new pending claims this Space has refused at the defensive ceiling.
    /// Stored at `mls.pending_prefix_refused:{space}`.
    pub(crate) fn pending_prefix_refusals(&self, space: &str) -> u64 {
        self.store
            .meta(&pending_refusal_meta(space))
            .ok()
            .flatten()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(0)
    }

    fn author_in_roster(&self, space: &str, identity: &str) -> bool {
        self.logs
            .get(space)
            .is_some_and(|log| fold_roster(log.events().iter()).contains_key(identity))
    }

    fn digest_forks(&self, space: &str, epoch: u64, digest: &str) -> bool {
        self.store
            .meta(&digest_meta(space, epoch))
            .ok()
            .flatten()
            .is_some_and(|mine| mine != digest)
    }

    fn prefix_forks(&self, space: &str, upto: &Seen) -> bool {
        self.chain_hash_at(space, upto.seq)
            .is_some_and(|mine| mine != upto.hash)
    }

    fn chain_hash_at(&self, space: &str, seq: u64) -> Option<String> {
        let event = self.logs.get(space)?.events().get(seq as usize)?;
        (event.seq == seq).then(|| event.hash.clone())
    }

    fn pending_prefixes(&self, space: &str) -> BTreeMap<String, PendingPrefix> {
        self.store
            .meta(&pending_prefix_meta(space))
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn store_pending_prefixes(&self, space: &str, pending: &BTreeMap<String, PendingPrefix>) {
        let key = pending_prefix_meta(space);
        let _ = if pending.is_empty() {
            self.store.meta_delete(&key)
        } else {
            self.store
                .set_meta(&key, &serde_json::to_string(pending).unwrap_or_default())
        };
    }

    pub(crate) fn recorded_fork(&self, space: &str) -> Option<String> {
        self.store.meta(&fork_meta(space)).ok().flatten()
    }

    pub(crate) fn fork_recorded(&self, space: &str) -> bool {
        self.recorded_fork(space).is_some()
    }

    pub(crate) fn restore_recorded_forks(&mut self) {
        let spaces = self.space_order.clone();
        for space in spaces {
            if let Some(reason) = self.recorded_fork(&space) {
                self.show_fork(&space, reason);
            }
        }
    }

    fn mark_checkpoint_fork(&mut self, space: &str) -> R<()> {
        let reason = fork_reason();
        self.store
            .set_meta(&fork_meta(space), &reason)
            .map_err(storage)?;
        self.show_fork(space, reason);
        Ok(())
    }

    /// The phrase has to live on the projection. The next confirmed event
    /// rebuilds `state` from there and would otherwise drop it.
    fn show_fork(&mut self, space: &str, reason: String) {
        if let Some(scoped) = self.space_projections.get_mut(space) {
            if let Some(s) = scoped.spaces.get_mut(space) {
                s.integrity_error = Some(reason.clone());
            }
        }
        if let Some(s) = self.state.spaces.get_mut(space) {
            s.integrity_error = Some(reason);
        }
    }

    // ── work for the network task ──

    /// This device's key packages, until the relay has them: the first full set with a
    /// last-resort one, then top-ups when the relay says the stock is low.
    pub fn mls_key_packages_to_publish(&mut self) -> Option<KeyPackageBatch> {
        let device_id = self.net.account.as_ref()?.device.clone();
        if self.net.mls.publishing.is_none() {
            let first = self.store.meta(META_PUBLISHED).ok().flatten() != Some(device_id);
            let device = self.device().ok()?;
            if first {
                let mut packages = device.key_packages(KEY_PACKAGES, true).ok()?;
                let last_resort = packages.pop()?;
                self.net.mls.publishing = Some((packages, Some(last_resort)));
            } else if self.net.mls.top_up > 0 {
                let packages = device.key_packages(self.net.mls.top_up, false).ok()?;
                tracing_like(&format!("key packages generated count={}", packages.len()));
                self.net.mls.top_up = 0;
                self.net.mls.publishing = Some((packages, None));
            }
        }
        self.net.mls.publishing.clone()
    }

    /// The relay says this device has `remaining` single-use packages left: refill to
    /// [`KEY_PACKAGES`]. Notices for the identity's other devices are theirs.
    pub fn mls_key_packages_low(&mut self, device: &str, remaining: u32) {
        let mine = self
            .net
            .account
            .as_ref()
            .is_some_and(|a| a.device == device);
        tracing_like(&format!(
            "key packages low remaining={remaining} mine={mine} publishing={}",
            self.net.mls.publishing.is_some()
        ));
        if mine && self.net.mls.publishing.is_none() {
            self.net.mls.top_up = KEY_PACKAGES.saturating_sub(remaining as usize);
        }
    }

    pub fn mls_key_packages_published(&mut self, result: Result<(), String>) {
        tracing_like(&format!("key packages published result={result:?}"));
        match result {
            Ok(()) => {
                if let Some(a) = &self.net.account {
                    let _ = self.store.set_meta(META_PUBLISHED, &a.device);
                }
                self.net.mls.publishing = None;
            }
            Err(e) => tracing_like(&format!("key packages refused: {e}")),
        }
    }

    /// One of this device's handshakes was refused for good: a commit that lost its epoch
    /// (`STALE_COMMIT`), or a Welcome whose commit isn't the last one any more. Neither is
    /// anything the person wrote, so it isn't shown as failed: a lost commit is dropped with
    /// its Welcome, the winner arrives from the log, and the group is checked again.
    /// `false` when `client_id` isn't a handshake.
    pub(crate) fn mls_handshake_refused(&mut self, client_id: &str, reason: &str) -> bool {
        let Some(e) = self.store.outbox().ok().and_then(|o| {
            o.into_iter()
                .map(|p| p.event)
                .find(|e| e.client_id == client_id)
        }) else {
            return false;
        };
        let EventBody::Sealed { kind } = &e.body else {
            return false;
        };
        if kind != SealedKind::Commit.name() && kind != SealedKind::Welcome.name() {
            return false;
        }
        tracing_like(&format!("{kind} in {} refused: {reason}", e.space));
        let dropped = (|| -> R<Vec<String>> {
            let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
            let mut dropped = vec![client_id.to_string()];
            if kind == SealedKind::Commit.name() {
                // Its Welcome, held behind it, has no commit to follow any more.
                let orphans: Vec<String> = self
                .store
                .outbox()
                .map_err(storage)?
                .into_iter()
                .map(|p| p.event)
                .filter(|o| {
                    o.space == e.space
                        && matches!(&o.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name())
                })
                .map(|o| o.client_id)
                .collect();
                for id in orphans {
                    self.store.outbox_remove(&id)?;
                    self.store.meta_delete(&welcome_meta(&id))?;
                    dropped.push(id);
                }
                let device = self.device_on(&tx)?;
                if device.has_group(&e.space) {
                    device.abandon(&e.space).map_err(mls_err)?;
                } else {
                    device.forget(&e.space).map_err(mls_err)?;
                }
                if let Some(reference) = Envelope::plain(&e)
                    .recovery()
                    .and_then(roda_log::recovery::RecoveryRef::parse)
                {
                    self.store.clear_recovery_upload(&reference.blob)?;
                }
            } else if kind == SealedKind::Welcome.name() {
                // Its commit landed but another got in before the Welcome: the newcomers' leaves
                // are in our group with no way in, so they get taken out and added afresh.
                let key = welcome_meta(client_id);
                let stranded = identity_set(self.store.meta(&key)?);
                self.store.meta_delete(&key)?;
                if !stranded.is_empty() {
                    let key = rewelcome_meta(&e.space);
                    let mut all = identity_set(self.store.meta(&key)?);
                    all.extend(stranded);
                    self.store.set_meta(&key, &join_set(&all))?;
                }
            }
            self.store.outbox_remove(client_id)?;
            tx.commit().map_err(storage)?;
            Ok(dropped)
        })();
        let dropped = match dropped {
            Ok(ids) => ids,
            Err(err) => {
                // This is still a handshake. Preserve its whole transaction for retry;
                // treating it as a failed user event would strand the pending group.
                tracing_like(&format!("dropping the handshake in {}: {err}", e.space));
                return true;
            }
        };
        for id in dropped {
            self.net.pending.remove(&id);
        }
        self.net.mls.mark_group_dirty(e.space);
        true
    }

    /// The next group that owes a commit adding someone: claim their key packages. Groups
    /// that only owe removals commit right here.
    pub fn mls_to_claim(
        &mut self,
        claim_clock: Option<KeyPackageClaimClock>,
    ) -> R<Option<(SpaceId, Vec<IdentityId>, String)>> {
        let now = Instant::now();
        let ready: Vec<SpaceId> = self
            .net
            .mls
            .retry_at
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(s, _)| s.clone())
            .collect();
        for s in ready {
            self.net.mls.retry_at.remove(&s);
            self.net.mls.mark_group_dirty(s);
        }
        let turn: Vec<SpaceId> = self
            .net
            .mls
            .turn_at
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(s, _)| s.clone())
            .collect();
        for s in turn {
            self.net.mls.turn_at.remove(&s);
            self.net.mls.waited.insert(s.clone());
            self.net.mls.mark_group_dirty(s);
        }
        let dirty: Vec<SpaceId> = self.net.mls.dirty.iter().cloned().collect();
        let mut unsupported = false;
        for space in dirty {
            self.settle_rewelcome(&space);
            self.settle_joins(&space);
            let owed = match self.owed(&space) {
                Ok(Some(owed)) => owed,
                Ok(None) => {
                    self.net.mls.waited.remove(&space);
                    self.net.mls.dirty.remove(&space);
                    continue;
                }
                Err(error) => {
                    self.net.mls.waited.remove(&space);
                    self.net.mls.dirty.remove(&space);
                    tracing_like(&format!("group check in {space}: {error}"));
                    continue;
                }
            };
            if let Some(turn) = self.admin_turn(&space) {
                self.net.mls.dirty.remove(&space);
                self.net.mls.turn_at.insert(space, now + turn);
                continue;
            }
            if owed.adds_nobody() {
                self.net.mls.dirty.remove(&space);
                if let Err(e) = self.commit_now(&space, &[], &owed.remove) {
                    tracing_like(&format!("removal commit in {space}: {e}"));
                    self.net
                        .mls
                        .retry_at
                        .insert(space.clone(), now + CLAIM_RETRY);
                }
                continue;
            }
            let Some(clock) = claim_clock else {
                unsupported = true;
                continue;
            };
            // Resolve recovery recipients before consuming one-shot key
            // packages. A pending agreement lookup must not spend a package
            // and then fail commit_now before staging the handshake.
            if self.recovery_recipients(&space).is_err() {
                continue;
            }
            let device = self
                .account()
                .ok_or_else(|| storage("no account"))?
                .device
                .clone();
            let epoch = self.device()?.epoch(&space).map_err(mls_err)?;
            let saved = self.load_claim(&space)?;
            let claim = match saved {
                Some(c) if c.device == device && c.epoch == epoch && c.intent == owed => c,
                _ => {
                    let c = ClaimOperation {
                        operation_id: claim_operation(clock)?,
                        device,
                        epoch,
                        targets: owed.to_claim(),
                        intent: owed,
                        records: None,
                    };
                    self.save_claim(&space, &c)?;
                    c
                }
            };
            self.net.mls.dirty.remove(&space);
            if let Some(records) = claim.records {
                self.mls_claimed(&space, &claim.operation_id, Ok(records));
                continue;
            }
            self.net.mls.claiming.insert(space.clone());
            return Ok(Some((space, claim.targets, claim.operation_id)));
        }
        if unsupported {
            return Err(CoreError::Invalid {
                reason: "This relay does not support durable key-package claims with a current server clock; upgrade or reconnect before adding chat members.".into(),
            });
        }
        Ok(None)
    }

    fn load_claim(&self, space: &str) -> R<Option<ClaimOperation>> {
        self.store
            .meta(&claim_meta(space))?
            .map(|data| serde_json::from_str(&data).map_err(storage))
            .transpose()
    }

    fn save_claim(&self, space: &str, claim: &ClaimOperation) -> R<()> {
        self.store.set_meta(
            &claim_meta(space),
            &serde_json::to_string(claim).map_err(storage)?,
        )?;
        Ok(())
    }

    /// A socket may close after a committed claim and before its reply. Only its volatile
    /// in-flight marker is cleared; the next session retries the persisted operation.
    pub(crate) fn mls_claims_disconnected(&mut self) {
        let interrupted: Vec<_> = self.net.mls.claiming.drain().collect();
        for space in interrupted {
            self.net.mls.mark_group_dirty(space);
        }
    }

    /// How long this device waits before committing an owed change in `space`: one stagger
    /// per admin ahead of it in identity order. Called after determining an owed change;
    /// `None` once it has waited or when it is first.
    fn admin_turn(&mut self, space: &str) -> Option<Duration> {
        if self.net.mls.waited.contains(space) || self.net.mls.turn_at.contains_key(space) {
            return None;
        }
        let me = self.me.as_deref()?;
        let roster = self.net.mls.rosters.get(space)?;
        let ahead = roster
            .iter()
            .filter(|(id, r)| matches!(r, Role::Owner | Role::Admin) && id.as_str() < me)
            .count() as u32;
        (ahead > 0).then(|| ADMIN_STAGGER * ahead)
    }

    /// People waiting for a fresh add whose old leaf is gone from our group: from here on
    /// they're simply listed and missing, and the next reconcile adds them. Nothing settles
    /// while our own commit is still on its way.
    fn settle_rewelcome(&mut self, space: &str) {
        let key = rewelcome_meta(space);
        let waiting = identity_set(self.store.meta(&key).ok().flatten());
        if waiting.is_empty() {
            return;
        }
        let Ok(device) = self.device() else {
            return;
        };
        if device.pending(space) {
            return;
        }
        let Ok(leaves) = device.leaves(space).map_err(mls_err) else {
            return;
        };
        drop(device);
        let has = |name: &str| {
            split_leaf(name)
                .is_some_and(|(id, dev)| leaves.contains_key(&(id.to_string(), dev.to_string())))
        };
        let (left, gone): (BTreeSet<_>, BTreeSet<_>) =
            waiting.into_iter().partition(|name| has(name));
        // Their leaf is out: they are added again like any device joining.
        if !gone.is_empty() {
            let mut joins = self.joins(space);
            for name in gone {
                joins.entry(name).or_default();
            }
            self.set_joins(space, &joins);
        }
        let _ = if left.is_empty() {
            self.store.meta_delete(&key)
        } else {
            self.store.set_meta(&key, &join_set(&left))
        };
    }

    /// What `space`'s group owes, when this device may commit and has nothing out
    /// already. Admins commit for everyone; any member commits for its own devices.
    fn owed(&self, space: &str) -> R<Option<Owed>> {
        let me = self.me_id()?;
        let Some(roster) = self.net.mls.rosters.get(space) else {
            return Ok(None);
        };
        let admin = match roster.get(&me) {
            Some(Role::Owner | Role::Admin) => true,
            Some(_) => false,
            None => return Ok(None),
        };
        let joins = self.joins(space);
        let stranded = identity_set(self.store.meta(&rewelcome_meta(space))?);
        let revoked = self.revoked_devices();
        let device = self.device()?;
        let reconciling = device.needs_reconciliation(space);
        let for_me = |name: &String| admin || split_leaf(name).is_some_and(|(id, _)| id == me);
        if !admin
            && !reconciling
            && !joins.keys().any(for_me)
            && !stranded.iter().any(for_me)
            && revoked.is_empty()
        {
            return Ok(None);
        }
        if self.net.mls.claiming.contains(space) || self.net.mls.retry_at.contains_key(space) {
            return Ok(None);
        }
        // Membership changes of ours still on their way: commit them all at once, after
        // the relay has ordered them (a commit must never get ahead of its log entries).
        let changing = self.net.pending.values().any(|s| s == space)
            && self.store.outbox()?.iter().any(|p| {
                p.event.space == space
                    && !p.failed
                    && matches!(
                        p.event.body,
                        EventBody::MemberAdded { .. } | EventBody::MemberRemoved { .. }
                    )
            });
        if changing {
            return Ok(None);
        }
        // A commit or Welcome of ours still on its way: the next commit waits for it, or a
        // newcomer could join from a Welcome that lands after a later commit and never
        // see that commit.
        let handshaking = self.store.outbox()?.iter().any(|p| {
            p.event.space == space
                && !p.failed
                && matches!(&p.event.body, EventBody::Sealed { kind }
                    if kind == SealedKind::Commit.name() || kind == SealedKind::Welcome.name())
        });
        if handshaking {
            return Ok(None);
        }
        let Some(leaves) = device.committed_leaves(space).map_err(mls_err)? else {
            return Ok(None);
        };
        Ok(reconciliation::plan(reconciliation::Input {
            me: &me,
            admin,
            reconciling,
            listed: &roster.keys().cloned().collect(),
            leaves: &leaves,
            joins: &joins,
            stranded: &stranded,
            revoked: &revoked,
        }))
    }

    /// Key packages for a group's newcomers came back: commit them (and any removals).
    pub fn mls_claimed(
        &mut self,
        space: &str,
        operation: &str,
        result: Result<Vec<KeyPackageRecord>, String>,
    ) {
        self.net.mls.wake_recovery(space);
        self.net.mls.claiming.remove(space);
        let mut claim = match self.load_claim(space) {
            Ok(Some(c)) if c.operation_id == operation => c,
            _ => {
                self.net.mls.mark_group_dirty(space.to_string());
                return;
            }
        };
        let records = match result {
            Ok(records) => {
                claim.records = Some(records.clone());
                if let Err(error) = self.save_claim(space, &claim) {
                    tracing_like(&format!("saving key-package claim in {space}: {error}"));
                    self.net
                        .mls
                        .retry_at
                        .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
                    return;
                }
                records
            }
            Err(error) => {
                if error == KEY_PACKAGE_CLAIM_EXPIRED {
                    let _ = self.store.meta_delete(&claim_meta(space));
                }
                tracing_like(&format!("key-package claim in {space}: {error}"));
                self.net
                    .mls
                    .retry_at
                    .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
                return;
            }
        };
        let owed = match self.owed(space) {
            Ok(Some(o)) => o,
            _ => {
                self.net.mls.mark_group_dirty(space.to_string());
                return;
            }
        };
        if owed != claim.intent {
            // A membership change while the reply was in flight is a new operation.
            self.net.mls.mark_group_dirty(space.to_string());
            return;
        }
        let Owed {
            add,
            add_devices,
            remove,
        } = owed;
        // Only packages whose verified leaf is one of the people (or devices) we asked for,
        // and never a device whose old leaf is still in (it goes first).
        let records: Vec<KeyPackageRecord> = records
            .into_iter()
            .filter(|r| {
                roda_mls::key_package_leaf(&r.data).is_ok_and(|l: Leaf| {
                    l.identity == r.identity
                        && l.device == r.device
                        && claim.targets.contains(&l.identity)
                        && (add.contains(&l.identity)
                            || add_devices.contains(&(l.identity.clone(), l.device.clone())))
                        && !remove.contains(&leaf_name(&l.identity, &l.device))
                })
            })
            .collect();
        let found: BTreeSet<&IdentityId> = records.iter().map(|r| &r.identity).collect();
        if add
            .iter()
            .any(|who| claim.targets.contains(who) && !found.contains(who))
        {
            // Someone can't be added yet (no device, or none with packages): messages
            // don't wait for them; they'll read from whenever their commit lands.
            self.net.mls.stuck.insert(space.to_string());
        }
        let devices_found: BTreeSet<(&str, &str)> = records
            .iter()
            .map(|r| (r.identity.as_str(), r.device.as_str()))
            .collect();
        let devices_missing = add_devices.iter().any(|(id, dev)| {
            claim.targets.contains(id) && !devices_found.contains(&(id.as_str(), dev.as_str()))
        });
        let packages: Vec<Vec<u8>> = records.into_iter().map(|r| r.data).collect();
        if packages.is_empty() && remove.is_empty() {
            // A recorded empty response will never acquire a newly published package.
            let _ = self.store.meta_delete(&claim_meta(space));
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
            return;
        }
        if let Err(e) = self.commit_with_claim(space, &packages, &remove, Some(operation)) {
            tracing_like(&format!("commit in {space}: {e}"));
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
        } else if devices_missing {
            // A joining device with no key packages yet (it hasn't come online): ask again.
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
        }
    }

    pub(crate) fn commit_now(
        &mut self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<IdentityId>,
    ) -> R<()> {
        self.commit_with_claim(space, add, remove, None)
    }

    fn commit_with_claim(
        &mut self,
        space: &str,
        add: &[Vec<u8>],
        remove: &BTreeSet<IdentityId>,
        operation: Option<&str>,
    ) -> R<()> {
        let recipients = self.recovery_recipients(space)?;
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        let c = self
            .device_on(&tx)?
            .commit(space, add, remove)
            .map_err(mls_err)?;
        let events = self.store_recovery_commit(space, c, recipients.as_deref())?;
        if let Some(welcome) = events.iter().find(
            |e| matches!(&e.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name()),
        ) {
            let newcomers: BTreeSet<String> = add
                .iter()
                .filter_map(|kp| roda_mls::key_package_leaf(kp).ok())
                .map(|l| leaf_name(&l.identity, &l.device))
                .collect();
            self.store
                .set_meta(&welcome_meta(&welcome.client_id), &join_set(&newcomers))?;
        }
        if let Some(operation) = operation {
            if self
                .load_claim(space)?
                .is_some_and(|c| c.operation_id == operation)
            {
                self.store.meta_delete(&claim_meta(space))?;
            }
        }
        tx.commit().map_err(storage)?;
        self.queued_recovery_commit(events);
        Ok(())
    }

    fn checkpointed_at(&mut self, space: &str) -> u64 {
        if let Some(at) = self.net.mls.checkpointed_at.get(space) {
            return *at;
        }
        let at = self
            .store
            .meta(&checkpointed_meta(space))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        self.net.mls.checkpointed_at.insert(space.to_string(), at);
        at
    }

    /// A stale publication blocks sealing only while its sent epoch is still current.
    /// A commit may already have landed and maintenance may have resealed this outbox
    /// entry before the old rejection arrives. Preserve that newer copy and epoch.
    pub(crate) fn mls_sealed_stale(&mut self, client_id: &str, sent_epoch: u64) {
        let Some(space) = self.net.pending.get(client_id).cloned() else {
            return;
        };
        if self.device().ok().and_then(|d| d.epoch(&space).ok()) == Some(sent_epoch) {
            self.net.mls.behind.insert(space, sent_epoch);
        }
    }

    /// Posts the checkpoints this device owes. `true` when it queued any.
    pub fn mls_checkpoints(&mut self) -> R<bool> {
        let due: Vec<SpaceId> = self.net.mls.checkpoint_due.drain().collect();
        let mut queued = false;
        for space in due {
            let Some(upto) = self.logs.get(&space).and_then(|l| l.head()) else {
                continue;
            };
            let (epoch, digest) = match self
                .device()
                .and_then(|d| d.checkpoint(&space).map_err(mls_err))
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            let me = self.me_id()?;
            let reached = upto.seq;
            self.append(
                &space,
                &me,
                EventBody::Checkpoint {
                    upto,
                    epoch,
                    digest,
                },
            )?;
            let _ = self
                .store
                .set_meta(&checkpointed_meta(&space), &reached.to_string());
            let _ = self.store.set_meta(
                &checkpointed_ms_meta(&space),
                &crate::engine::now_ms().to_string(),
            );
            self.net.mls.checkpointed_at.insert(space, reached);
            queued = true;
        }
        Ok(queued)
    }

    /// Nothing MLS-side left to do right now (the CLI waits for this before exiting).
    pub fn mls_settled(&self) -> bool {
        let m = &self.net.mls;
        let published = self.net.account.as_ref().is_none_or(|a| {
            self.store.meta(META_PUBLISHED).ok().flatten().as_deref() == Some(a.device.as_str())
        });
        published
            && m.publishing.is_none()
            && m.top_up == 0
            && m.dirty.is_empty()
            && m.claiming.is_empty()
            && m.retry_at.is_empty()
            && m.turn_at.is_empty()
            && m.checkpoint_due.is_empty()
            && m.rejoin.is_empty()
            && !m.has_recovery_work()
    }

    /// The group as this device has it: epoch, checkpoint digest and members.
    pub fn mls_status(&self, space: &str) -> Option<(u64, String, Vec<IdentityId>)> {
        let device = self.device().ok()?;
        let (epoch, digest) = device.checkpoint(space).ok()?;
        let members = device.roster(space).ok()?.into_iter().collect();
        Some((epoch, digest, members))
    }
}

/// The inner event of an opened message, if it is exactly what its envelope says: same
/// Space, client id, author, device, time and `seen`, sent by that author's MLS leaf.
fn inner_event(ev: &Sequenced, plaintext: &[u8], from: &Leaf) -> Option<Event> {
    let inner = InnerEvent::decode(plaintext)?;
    let e = event_from_content(
        inner.content,
        inner.sig,
        ev.env.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .ok()?;
    let env = &ev.env;
    let same = e.space == env.space()
        && e.client_id == env.client_id()
        && e.author == env.author()
        && e.device.as_deref() == env.device()
        && e.at_ms == env.at_ms()
        && e.seen == env.seen()
        && from.identity == e.author
        && Some(from.device.as_str()) == e.device.as_deref()
        && !stays_clear(&e.body)
        && !matches!(e.body, EventBody::Sealed { .. });
    same.then_some(e)
}

pub(crate) fn must_seal(body: &EventBody) -> bool {
    !stays_clear(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use roda_log::{chain_hash, Author, Signer};

    fn ordered(engine: &Engine, env: Envelope) -> Sequenced {
        let (seq, prev) = engine
            .logs
            .get(env.space())
            .map(|log| {
                (
                    log.next_seq(),
                    log.events()
                        .last()
                        .map(|e| e.hash.clone())
                        .unwrap_or_else(|| roda_types::GENESIS_PREV.into()),
                )
            })
            .unwrap_or_else(|| (0, roda_types::GENESIS_PREV.into()));
        Sequenced {
            seq,
            hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
            prev,
            env,
        }
    }

    fn seen(engine: &Engine, space: &str) -> Option<roda_types::Seen> {
        engine
            .logs
            .get(space)
            .and_then(|log| log.events().last())
            .map(|e| roda_types::Seen {
                seq: e.seq,
                hash: e.hash.clone(),
            })
    }

    fn confirm_claim_handshakes(engine: &mut Engine) -> Vec<Sequenced> {
        for (sha, _) in engine.pending_uploads(100) {
            engine.upload_done(&sha);
        }
        let mut frames = Vec::new();
        // Confirmation may enqueue the epoch checkpoint; order it through the same log.
        while engine.outbox_len() > 0 {
            let pending = engine.store.outbox().unwrap();
            assert!(!pending.is_empty());
            for queued in pending {
                let frame = ordered(engine, Envelope::plain(&queued.event));
                assert_eq!(engine.ingest(frame.clone()), Ingest::Confirmed);
                frames.push(frame);
            }
            assert!(frames.len() < 10, "handshake confirmation did not settle");
        }
        frames
    }

    fn claim_group(engine: &mut Engine, space: &str, peers: &mut [Engine]) -> Vec<Sequenced> {
        let author = engine.net.author.clone().unwrap();
        let mut bodies = vec![EventBody::SpaceCreated {
            title: space.into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        }];
        for peer in peers.iter_mut() {
            peer.net.profiles.supported = true;
            let identity = peer.me.clone().unwrap();
            let (public, signed) = peer.agreement_to_publish().unwrap();
            engine.agreement_keys_arrived(
                vec![roda_proto::AgreementKeyRecord {
                    identity: identity.clone(),
                    public,
                    signed,
                }],
                std::slice::from_ref(&identity),
            );
            bodies.push(EventBody::MemberAdded {
                identity,
                role: Role::Member,
            });
        }
        let mut frames = Vec::new();
        for (i, body) in bodies.into_iter().enumerate() {
            let env = Envelope::plain(&author.sign_event(
                space,
                &format!("{space}-{i}"),
                1,
                seen(engine, space),
                body,
            ));
            let frame = ordered(engine, env);
            assert_eq!(engine.ingest(frame.clone()), Ingest::Applied);
            frames.push(frame);
        }
        engine.create_mls_group(space).unwrap();
        frames
    }

    fn claimed_records(
        peers: &mut [Engine],
        targets: &[String],
    ) -> Vec<roda_proto::KeyPackageRecord> {
        peers
            .iter_mut()
            .filter_map(|peer| {
                let identity = peer.me.clone().unwrap();
                if !targets.contains(&identity) {
                    return None;
                }
                let device = peer.net.account.as_ref().unwrap().device.clone();
                let data = peer
                    .device()
                    .unwrap()
                    .key_packages(1, false)
                    .unwrap()
                    .remove(0);
                Some(roda_proto::KeyPackageRecord {
                    identity,
                    device,
                    data,
                })
            })
            .collect()
    }

    #[test]
    fn fifty_one_members_reconcile_in_two_durable_claim_batches() {
        let mut owner = Engine::open(":memory:").unwrap();
        owner
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        owner.net.profiles.supported = true;
        let mut peers: Vec<Engine> = (0..51)
            .map(|i| {
                let mut peer = Engine::open(":memory:").unwrap();
                peer.create_account(
                    &format!("Peer {i}"),
                    &format!("peer{i}"),
                    "http://relay.test",
                )
                .unwrap();
                peer
            })
            .collect();
        let space = "fifty-one-claim-members";
        let mut frames = claim_group(&mut owner, space, &mut peers);
        let clock = Some(
            roda_proto::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap(),
        );
        let (first_space, first_targets, first_operation) =
            owner.mls_to_claim(clock).unwrap().unwrap();
        assert_eq!(first_space, space);
        assert_eq!(first_targets.len(), 50);
        assert!(first_targets.windows(2).all(|pair| pair[0] < pair[1]));
        owner.mls_claimed(
            space,
            &first_operation,
            Ok(claimed_records(&mut peers, &first_targets)),
        );
        frames.extend(confirm_claim_handshakes(&mut owner));
        let (second_space, second_targets, second_operation) =
            owner.mls_to_claim(clock).unwrap().unwrap();
        assert_eq!(second_space, space);
        assert_eq!(second_targets.len(), 1);
        assert!(!first_targets.contains(&second_targets[0]));
        assert_ne!(second_operation, first_operation);
        owner.mls_claimed(
            space,
            &second_operation,
            Ok(claimed_records(&mut peers, &second_targets)),
        );
        frames.extend(confirm_claim_handshakes(&mut owner));
        assert!(owner.mls_to_claim(clock).unwrap().is_none());
        assert_eq!(owner.device().unwrap().leaves(space).unwrap().len(), 52);
        assert!(owner.ready_epoch(space).is_some());
        let recipient = peers
            .iter_mut()
            .find(|p| p.me.as_ref() == Some(&second_targets[0]))
            .unwrap();
        for frame in frames {
            assert_eq!(recipient.ingest(frame), Ingest::Applied);
        }
        assert_eq!(
            recipient.device().unwrap().checkpoint(space).unwrap(),
            owner.device().unwrap().checkpoint(space).unwrap()
        );
    }

    #[test]
    fn unsupported_claims_in_one_chat_do_not_block_another_chats_removal() {
        let mut owner = Engine::open(":memory:").unwrap();
        owner
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        owner.net.profiles.supported = true;
        let mut peer = Engine::open(":memory:").unwrap();
        peer.create_account("Bruno", "bruno", "http://relay.test")
            .unwrap();
        let peer_identity = peer.me.clone().unwrap();
        for space in ["fallback-one", "fallback-two"] {
            claim_group(&mut owner, space, std::slice::from_mut(&mut peer));
            let records = claimed_records(
                std::slice::from_mut(&mut peer),
                std::slice::from_ref(&peer_identity),
            );
            owner
                .commit_now(space, &[records[0].data.clone()], &Default::default())
                .unwrap();
            confirm_claim_handshakes(&mut owner);
        }
        owner.net.mls.dirty = ["fallback-one".into(), "fallback-two".into()]
            .into_iter()
            .collect();
        let adding = owner.net.mls.dirty.iter().next().unwrap().clone();
        let removing = owner
            .net
            .mls
            .dirty
            .iter()
            .find(|s| **s != adding)
            .unwrap()
            .clone();
        let author = owner.net.author.clone().unwrap();
        let mut newcomer = Engine::open(":memory:").unwrap();
        newcomer
            .create_account("Carol", "carol", "http://relay.test")
            .unwrap();
        for (space, body) in [
            (
                adding.as_str(),
                EventBody::MemberAdded {
                    identity: newcomer.me.clone().unwrap(),
                    role: Role::Member,
                },
            ),
            (
                removing.as_str(),
                EventBody::MemberRemoved {
                    identity: peer_identity.clone(),
                },
            ),
        ] {
            let env = Envelope::plain(&author.sign_event(
                space,
                &format!("change-{space}"),
                2,
                seen(&owner, space),
                body,
            ));
            assert_eq!(owner.ingest(ordered(&owner, env)), Ingest::Applied);
        }
        assert_eq!(
            owner.net.mls.dirty.iter().next(),
            Some(&adding),
            "blocked addition must precede removal in this regression"
        );
        assert!(owner.mls_to_claim(None).is_err());
        assert!(owner.net.mls.dirty.contains(&adding));
        assert!(!owner.device().unwrap().pending(&adding));
        assert!(
            owner.device().unwrap().pending(&removing),
            "unrelated removal was never staged"
        );
        confirm_claim_handshakes(&mut owner);
        assert_eq!(owner.device().unwrap().leaves(&removing).unwrap().len(), 1);
        assert!(owner.ready_epoch(&removing).is_some());
    }

    #[test]
    fn an_obsolete_stale_rejection_cannot_release_a_current_epoch_block() {
        let mut engine = Engine::open(":memory:").unwrap();
        engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        let space = "sp_current_stale_epoch";
        let mut log = SpaceLog::new(space);
        let genesis = log
            .sequence(author.sign_event(
                space,
                "created",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Current epoch".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            ))
            .clone();
        assert_eq!(
            engine.ingest(Sequenced {
                seq: genesis.seq,
                prev: genesis.prev.clone(),
                hash: genesis.hash.clone(),
                env: Envelope::plain(&genesis),
            }),
            Ingest::Applied
        );
        engine.create_mls_group(space).unwrap();
        let commit = engine
            .device()
            .unwrap()
            .commit(space, &[], &Default::default())
            .unwrap();
        let env = Envelope::sealed(
            &author,
            space,
            "advance",
            2,
            log.head().as_ref(),
            Sealed::new(SealedKind::Commit, SUITE_ID, commit.commit),
        );
        assert_eq!(
            engine.ingest(Sequenced {
                seq: 1,
                prev: genesis.hash.clone(),
                hash: chain_hash(space, 1, &genesis.hash, &env.wire_hash()),
                env,
            }),
            Ingest::Applied
        );
        assert_eq!(engine.mls_status(space).unwrap().0, 1);
        let message = engine
            .append_synced(
                space,
                &author,
                3,
                EventBody::MessagePosted {
                    message: "pending".into(),
                    text: "Waiting for the next commit".into(),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                },
            )
            .unwrap();
        assert!(engine.mls_seal_outbox().unwrap());
        let copy = engine
            .store
            .meta(&sealed_meta(&message.client_id))
            .unwrap()
            .unwrap();
        assert_eq!(engine.outbox_envelopes().len(), 1);
        engine.reject(&message.client_id, roda_proto::STALE_SEAL, true, Some(1));
        assert!(
            engine.outbox_envelopes().is_empty(),
            "a rejected current epoch waits for its missing commit"
        );
        engine.reject(&message.client_id, roda_proto::STALE_SEAL, true, Some(0));
        assert!(
            engine.outbox_envelopes().is_empty(),
            "an obsolete rejection must not overwrite the current epoch's block"
        );
        assert_eq!(engine.net.mls.behind.get(space), Some(&1));
        assert_eq!(
            engine.store.meta(&sealed_meta(&message.client_id)).unwrap(),
            Some(copy)
        );
        assert_eq!(
            engine.store.outbox_get(&message.client_id).unwrap(),
            Some(message)
        );
        engine.logs[space].verify().unwrap();
    }

    #[test]
    fn rejected_outer_signature_keeps_the_own_event_pending() {
        let mut engine = Engine::open(":memory:").unwrap();
        engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        let space = "sp_own_outer_auth";
        let mut log = SpaceLog::new(space);
        let genesis = log
            .sequence(author.sign_event(
                space,
                "created",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Outer signature".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            ))
            .clone();
        assert_eq!(
            engine.ingest(Sequenced {
                seq: genesis.seq,
                prev: genesis.prev.clone(),
                hash: genesis.hash.clone(),
                env: Envelope::plain(&genesis),
            }),
            Ingest::Applied
        );
        engine.create_mls_group(space).unwrap();
        let own = author.sign_event(
            space,
            "pending",
            2,
            log.head(),
            EventBody::MessagePosted {
                message: "message".into(),
                text: "Still pending".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        );
        engine.store.outbox_put(&own).unwrap();
        engine
            .net
            .pending
            .insert(own.client_id.clone(), space.into());
        let mut env = Envelope::sealed(
            &author,
            space,
            &own.client_id,
            own.at_ms,
            own.seen.as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, vec![1, 2, 3]),
        );
        let hash = chain_hash(space, 1, &genesis.hash, &env.wire_hash());
        env.sig = "00".repeat(64);
        let group_before = engine.mls_status(space).unwrap();
        let count_before = engine.store.event_count().unwrap();
        assert!(matches!(
            engine.ingest(Sequenced {
                seq: 1,
                prev: genesis.hash,
                hash,
                env,
            }),
            Ingest::Invalid(_)
        ));
        assert_eq!(
            engine.store.outbox_get(&own.client_id).unwrap(),
            Some(own.clone())
        );
        assert!(engine.net.pending.contains_key(&own.client_id));
        assert_eq!(engine.logs[space].next_seq(), 1);
        assert_eq!(engine.store.event_count().unwrap(), count_before);
        assert_eq!(engine.mls_status(space), Some(group_before));
    }

    #[test]
    fn rejected_pruned_history_preserves_the_mls_group() {
        let mut engine = Engine::open(":memory:").unwrap();
        engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        let author: Author = engine.net.author.clone().unwrap();
        let space = "sp_pruned_auth";
        let mut log = SpaceLog::new(space);
        let genesis = log
            .sequence(author.sign_event(
                space,
                "created",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Verified history".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            ))
            .clone();
        assert_eq!(
            engine.ingest(Sequenced {
                seq: genesis.seq,
                prev: genesis.prev.clone(),
                hash: genesis.hash.clone(),
                env: Envelope::plain(&genesis),
            }),
            Ingest::Applied,
        );
        engine.create_mls_group(space).unwrap();
        let group_before = engine.mls_status(space).unwrap();
        let full = Envelope::sealed(
            &author,
            space,
            "missed-ciphertext",
            2,
            log.head().as_ref(),
            Sealed::new(SealedKind::Application, SUITE_ID, vec![1, 2, 3]),
        );
        let valid = Sequenced {
            seq: 1,
            prev: genesis.hash.clone(),
            hash: chain_hash(space, 1, &genesis.hash, &full.wire_hash()),
            env: full.pruned().unwrap(),
        };
        let mut bad_signature = valid.clone();
        bad_signature.env.sig = "00".repeat(64);
        let mut bad_chain = valid.clone();
        bad_chain.hash = "00".repeat(32);
        let stored_before = engine.store.event_count().unwrap();
        for invalid in [bad_signature, bad_chain] {
            assert!(matches!(engine.ingest(invalid), Ingest::Invalid(_)));
            assert_eq!(engine.mls_status(space), Some(group_before.clone()));
            assert!(!engine.net.mls.rejoin.contains(space));
            assert_eq!(engine.store.event_count().unwrap(), stored_before);
            assert_eq!(engine.logs[space].next_seq(), 1);
        }
        assert_eq!(engine.ingest(valid), Ingest::Applied);
        assert!(engine.mls_status(space).is_none());
        assert!(engine.net.mls.rejoin.contains(space));
        assert_eq!(engine.store.event_count().unwrap(), stored_before + 1);
        engine.logs[space].verify().unwrap();
    }

    /// Twelve sealed pages, delivered in the worst order the publish window allows.
    /// A seventh generation opened before the first is permanent loss; the outbox
    /// never offers that sixth in-flight ciphertext.
    #[test]
    fn publish_window_keeps_a_reordered_burst_decryptable() {
        use roda_log::Signer;
        use roda_mls::sealed::state_key;
        use std::collections::{BTreeSet, HashMap};

        let mut engine = Engine::open(":memory:").unwrap();
        let (root_secret, _, _) = engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        let space = "sp_ratchet_window";
        let genesis = Envelope::plain(&author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Window".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ));
        assert_eq!(engine.ingest(ordered(&engine, genesis)), Ingest::Applied);
        engine.create_mls_group(space).unwrap();

        let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate());
        let mut peer_conn = Connection::open_in_memory().unwrap();
        roda_mls::migrate(&mut peer_conn).unwrap();
        let peer = Device::new(
            &peer_conn,
            state_key(&peer_author.key.secret()),
            &peer_author.identity,
            peer_author.key.secret(),
            peer_author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let packages = peer.key_packages(1, false).unwrap();
        let adding = engine
            .device()
            .unwrap()
            .commit(space, &packages, &Default::default())
            .unwrap();
        let commit = Envelope::sealed(
            &author,
            space,
            "add-peer",
            2,
            seen(&engine, space).as_ref(),
            Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
        );
        assert_eq!(engine.ingest(ordered(&engine, commit)), Ingest::Applied);
        let roster: BTreeSet<String> = BTreeSet::from([author.identity.clone()]);
        let welcome_bytes = adding.welcome.unwrap();
        assert!(peer.join(space, &welcome_bytes, &roster).unwrap());
        let welcome = Envelope::sealed(
            &author,
            space,
            "welcome-peer",
            3,
            seen(&engine, space).as_ref(),
            Sealed::new(SealedKind::Welcome, SUITE_ID, welcome_bytes),
        );
        assert_eq!(engine.ingest(ordered(&engine, welcome)), Ingest::Applied);
        let sender = engine.device().unwrap().leaf().clone();

        for i in 0..12 {
            engine
                .append_synced(
                    space,
                    &author,
                    1_000 + i,
                    EventBody::MessagePosted {
                        message: format!("m{i}"),
                        text: format!("page-{i}"),
                        attaches: None,
                        reply: None,
                        forwarded: false,
                    },
                )
                .unwrap();
        }
        assert!(engine.mls_seal_outbox().unwrap());
        let mut opened = 0u32;
        let mut remaining = 12usize;
        while remaining > 0 {
            let apps: Vec<_> = engine
                .outbox_envelopes()
                .into_iter()
                .filter(|env| env.sealed_kind() == Some(SealedKind::Application))
                .collect();
            assert!(
                !apps.is_empty() && apps.len() <= 5,
                "in flight {}, still queued {remaining}",
                apps.len()
            );
            assert!(apps.len() < remaining || remaining <= 5);
            let mut sent = HashMap::new();
            for env in &apps {
                sent.insert(env.client_id().to_string(), Some(0u64));
            }
            assert!(
                engine
                    .outbox_envelopes_except(&sent)
                    .iter()
                    .all(|env| env.sealed_kind() != Some(SealedKind::Application)),
                "a later generation left while {} were still unaccepted",
                apps.len()
            );
            for env in apps.iter().rev() {
                let data = env.sealed_data().unwrap().1;
                match peer.open(space, data, &roster, &sender).unwrap() {
                    Opened::Application { plaintext, .. } => {
                        assert!(
                            plaintext.windows(b"page-".len()).any(|w| w == b"page-"),
                            "opened bytes are not a page"
                        );
                    }
                    other => panic!("expected an application message, got {other:?}"),
                }
                opened += 1;
            }
            for env in &apps {
                assert_eq!(
                    engine.ingest(ordered(&engine, env.clone())),
                    Ingest::Confirmed
                );
            }
            remaining -= apps.len();
        }
        assert_eq!(opened, 12);

        let mut late = Vec::new();
        for i in 0..7 {
            late.push(
                engine
                    .device()
                    .unwrap()
                    .seal(space, format!("gap-{i}").as_bytes())
                    .unwrap(),
            );
        }
        peer.open(space, &late[6], &roster, &sender).unwrap();
        let err = peer.open(space, &late[0], &roster, &sender).unwrap_err();
        assert!(
            format!("{err:?}").contains("TooDistantInThePast"),
            "{err:?}"
        );
    }

    /// Space A already has five application generations in flight. Its sixth
    /// application, and anything newer in that same Space, stays queued. Space B
    /// still publishes.
    #[test]
    fn publish_window_lets_the_next_space_through() {
        use roda_log::Signer;
        use roda_mls::sealed::state_key;
        use std::collections::{BTreeSet, HashMap};

        let mut engine = Engine::open(":memory:").unwrap();
        let (root_secret, _, _) = engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        let space_a = "sp_cap_a";
        assert_eq!(
            engine.ingest(ordered(
                &engine,
                Envelope::plain(&author.sign_event(
                    space_a,
                    "created-a",
                    1,
                    None,
                    EventBody::SpaceCreated {
                        title: "A".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::EndToEnd,
                    },
                )),
            )),
            Ingest::Applied
        );
        engine.create_mls_group(space_a).unwrap();

        let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate());
        let mut peer_conn = Connection::open_in_memory().unwrap();
        roda_mls::migrate(&mut peer_conn).unwrap();
        let peer = Device::new(
            &peer_conn,
            state_key(&peer_author.key.secret()),
            &peer_author.identity,
            peer_author.key.secret(),
            peer_author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let packages = peer.key_packages(1, false).unwrap();
        let adding = engine
            .device()
            .unwrap()
            .commit(space_a, &packages, &Default::default())
            .unwrap();
        assert_eq!(
            engine.ingest(ordered(
                &engine,
                Envelope::sealed(
                    &author,
                    space_a,
                    "add-peer",
                    2,
                    seen(&engine, space_a).as_ref(),
                    Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
                ),
            )),
            Ingest::Applied
        );
        let roster: BTreeSet<String> = BTreeSet::from([author.identity.clone()]);
        let welcome_bytes = adding.welcome.unwrap();
        assert!(peer.join(space_a, &welcome_bytes, &roster).unwrap());
        assert_eq!(
            engine.ingest(ordered(
                &engine,
                Envelope::sealed(
                    &author,
                    space_a,
                    "welcome-peer",
                    3,
                    seen(&engine, space_a).as_ref(),
                    Sealed::new(SealedKind::Welcome, SUITE_ID, welcome_bytes),
                ),
            )),
            Ingest::Applied
        );

        let space_b = "sp_cap_b";
        assert_eq!(
            engine.ingest(ordered(
                &engine,
                Envelope::plain(&author.sign_event(
                    space_b,
                    "created-b",
                    1,
                    None,
                    EventBody::SpaceCreated {
                        title: "B".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::Closed,
                    },
                )),
            )),
            Ingest::Applied
        );

        let mut a_messages = Vec::new();
        for i in 0..6 {
            a_messages.push(
                engine
                    .append_synced(
                        space_a,
                        &author,
                        1_000 + i,
                        EventBody::MessagePosted {
                            message: format!("a{i}"),
                            text: format!("page-{i}"),
                            attaches: None,
                            reply: None,
                            forwarded: false,
                        },
                    )
                    .unwrap(),
            );
        }
        let b_message = engine
            .append_synced(
                space_b,
                &author,
                2_000,
                EventBody::MessagePosted {
                    message: "b".into(),
                    text: "other-space".into(),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                },
            )
            .unwrap();
        assert!(engine.mls_seal_outbox().unwrap());
        let handshake = engine
            .handshake_event(
                space_a,
                Sealed::new(SealedKind::Commit, SUITE_ID, b"later-commit".to_vec()),
            )
            .unwrap();
        assert!(matches!(
            &handshake.body,
            EventBody::Sealed { kind } if kind == "Commit"
        ));
        engine.store.outbox_put(&handshake).unwrap();
        assert!(a_messages[5].at_ms < b_message.at_ms);
        assert!(b_message.at_ms < handshake.at_ms);

        let order: Vec<String> = engine
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .map(|pending| pending.event.client_id)
            .collect();
        let sixth_at = order
            .iter()
            .position(|id| id == &a_messages[5].client_id)
            .unwrap();
        let b_at = order
            .iter()
            .position(|id| id == &b_message.client_id)
            .unwrap();
        let handshake_at = order
            .iter()
            .position(|id| id == &handshake.client_id)
            .unwrap();
        assert!(sixth_at < b_at && b_at < handshake_at);

        let mut sent = HashMap::new();
        for message in a_messages.iter().take(5) {
            sent.insert(message.client_id.clone(), Some(0u64));
        }
        let offered = engine.outbox_envelopes_except(&sent);
        assert!(
            offered
                .iter()
                .any(|env| env.client_id() == b_message.client_id),
            "space B is missing from the publish batch"
        );
        assert!(
            offered
                .iter()
                .all(|env| env.client_id() != a_messages[5].client_id),
            "A's sixth application left while five were in flight"
        );
        assert!(
            offered
                .iter()
                .all(|env| env.client_id() != handshake.client_id),
            "A's later handshake overtook the held application"
        );
        assert!(offered.iter().all(|env| env.space() != space_a));
    }
    const FORK: &str = "Members see different groups in this chat (the server may have forked it).";

    fn member(path: &str) -> (Engine, Author, [u8; 32]) {
        let mut engine = Engine::open(path).unwrap();
        let (_, device_secret, _) = engine
            .create_account("Ana", "ana_u18", "http://relay.test")
            .unwrap();
        let author = engine.net.author.clone().unwrap();
        (engine, author, device_secret)
    }

    fn post(text: &str) -> EventBody {
        EventBody::MessagePosted {
            message: format!("m-{text}"),
            text: text.into(),
            attaches: None,
            reply: None,
            forwarded: false,
        }
    }

    fn checkpoint(upto: roda_types::Seen, digest: &str) -> EventBody {
        EventBody::Checkpoint {
            upto,
            epoch: 1,
            digest: digest.into(),
        }
    }

    fn ingest_signed(engine: &mut Engine, author: &Author, space: &str, body: EventBody) {
        let seq = engine.next_seq(space);
        let event = author.sign_event(
            space,
            &format!("{space}-{seq}"),
            1_700_000_000_000 + seq as i64,
            seen(engine, space),
            body,
        );
        let got = engine.ingest(ordered(engine, Envelope::plain(&event)));
        assert!(
            matches!(got, Ingest::Applied | Ingest::Confirmed),
            "ingest {seq}: {got:?}"
        );
    }

    fn start_space(engine: &mut Engine, author: &Author, space: &str) {
        ingest_signed(
            engine,
            author,
            space,
            EventBody::SpaceCreated {
                title: space.into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        );
    }

    fn hash_at(engine: &Engine, space: &str, seq: u64) -> String {
        engine.logs[space].events()[seq as usize].hash.clone()
    }

    fn integrity(engine: &Engine, space: &str) -> Option<String> {
        engine
            .state
            .spaces
            .get(space)
            .and_then(|s| s.integrity_error.clone())
    }

    fn device_key(author: &Author) -> String {
        author
            .device
            .clone()
            .unwrap_or_else(|| author.identity.clone())
    }

    fn add_member(engine: &mut Engine, owner: &Author, space: &str, peer: &Author) {
        ingest_signed(
            engine,
            owner,
            space,
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        );
    }

    fn pending(engine: &Engine, space: &str) -> BTreeMap<String, PendingPrefix> {
        engine
            .store
            .meta(&pending_prefix_meta(space))
            .unwrap()
            .map(|raw| serde_json::from_str(&raw).unwrap())
            .unwrap_or_default()
    }

    fn claim(seq: u64, hash: &str) -> EventBody {
        checkpoint(
            roda_types::Seen {
                seq,
                hash: hash.into(),
            },
            &"ab".repeat(32),
        )
    }

    fn extend_until(engine: &mut Engine, author: &Author, space: &str, seq: u64) {
        while engine.next_seq(space) <= seq {
            let n = engine.next_seq(space);
            ingest_signed(engine, author, space, post(&format!("reach-{n}")));
        }
    }

    #[test]
    fn divergent_prefix_on_the_local_chain_marks_the_space() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-now";
        start_space(&mut engine, &author, space);
        ingest_signed(&mut engine, &author, space, post("one"));
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        ingest_signed(
            &mut engine,
            &peer,
            space,
            checkpoint(
                roda_types::Seen {
                    seq: 0,
                    hash: "11".repeat(32),
                },
                &"ab".repeat(32),
            ),
        );
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn matching_prefix_does_not_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-match";
        start_space(&mut engine, &author, space);
        ingest_signed(&mut engine, &author, space, post("one"));
        let hash = hash_at(&engine, space, 0);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        ingest_signed(
            &mut engine,
            &peer,
            space,
            checkpoint(roda_types::Seen { seq: 0, hash }, &"ab".repeat(32)),
        );
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(pending(&engine, space).is_empty());
        assert_eq!(engine.logs[space].events().len(), 4);
    }

    #[test]
    fn matching_pending_prefix_is_cleared_without_a_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-clear";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let mut stored = BTreeMap::new();
        stored.insert(
            device_key(&peer),
            PendingPrefix {
                seq: 0,
                hash: hash_at(&engine, space, 0),
                epoch: 1,
                digest: "ab".repeat(32),
            },
        );
        engine
            .store
            .set_meta(
                &pending_prefix_meta(space),
                &serde_json::to_string(&stored).unwrap(),
            )
            .unwrap();
        ingest_signed(&mut engine, &author, space, post("later"));
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn pending_prefix_forks_when_the_chain_reaches_it() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-later";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let claimed = "22".repeat(32);
        let target = engine.next_seq(space) + 3;
        ingest_signed(&mut engine, &peer, space, claim(target, &claimed));
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(
            pending(&engine, space).get(&device_key(&peer)),
            Some(&PendingPrefix {
                seq: target,
                hash: claimed,
                epoch: 1,
                digest: "ab".repeat(32),
            })
        );
        let quiet = target - engine.next_seq(space);
        for _ in 0..quiet {
            ingest_signed(
                &mut engine,
                &author,
                space,
                post(&format!("before-{target}")),
            );
            assert_eq!(integrity(&engine, space), None);
            assert_eq!(pending(&engine, space).len(), 1);
        }
        ingest_signed(&mut engine, &author, space, post("reached"));
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
        assert!(pending(&engine, space).is_empty());
        assert_ne!(hash_at(&engine, space, target), "22".repeat(32));
    }

    #[test]
    fn pending_prefix_survives_reopen_and_forks_when_reached() {
        let dir = std::env::temp_dir().join(format!("u18-prefix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("device.sqlite");
        let _ = std::fs::remove_file(&path);
        let path = path.to_str().unwrap().to_string();
        let space = "prefix-reopen";
        let claimed = "33".repeat(32);
        let peer = Author::root(Signer::generate());
        let device_secret = {
            let (mut engine, author, device_secret) = member(&path);
            start_space(&mut engine, &author, space);
            add_member(&mut engine, &author, space, &peer);
            assert_eq!(engine.next_seq(space), 2);
            ingest_signed(&mut engine, &peer, space, claim(5, &claimed));
            assert_eq!(integrity(&engine, space), None);
            assert_eq!(
                pending(&engine, space)
                    .get(&device_key(&peer))
                    .map(|item| item.seq),
                Some(5)
            );
            device_secret
        };
        let mut engine = Engine::open(&path).unwrap();
        assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(
            pending(&engine, space).get(&device_key(&peer)),
            Some(&PendingPrefix {
                seq: 5,
                hash: claimed,
                epoch: 1,
                digest: "ab".repeat(32),
            })
        );
        let author = engine.net.author.clone().unwrap();
        let quiet = 5 - engine.next_seq(space);
        for _ in 0..quiet {
            ingest_signed(&mut engine, &author, space, post("before-reopen"));
            assert_eq!(integrity(&engine, space), None);
        }
        ingest_signed(&mut engine, &author, space, post("reached-reopen"));
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn divergent_digest_still_marks_the_space() {
        let (mut engine, author, _) = member(":memory:");
        let space = "digest-still";
        start_space(&mut engine, &author, space);
        engine
            .store
            .set_meta(&digest_meta(space, 1), &"aa".repeat(32))
            .unwrap();
        let hash = hash_at(&engine, space, 0);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        ingest_signed(
            &mut engine,
            &peer,
            space,
            checkpoint(roda_types::Seen { seq: 0, hash }, &"bb".repeat(32)),
        );
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
    }

    #[test]
    fn colluding_near_claims_do_not_evict_an_honest_far_claim() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-collude";
        start_space(&mut engine, &author, space);
        let honest = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &honest);
        let mut colluders = Vec::new();
        for _ in 0..40 {
            let peer = Author::root(Signer::generate());
            add_member(&mut engine, &author, space, &peer);
            colluders.push(peer);
        }
        let near = engine.next_seq(space) + 40 + 8;
        let far = near + 40;
        let honest_hash = "44".repeat(32);
        for (n, peer) in colluders.iter().enumerate() {
            ingest_signed(
                &mut engine,
                peer,
                space,
                claim(near, &format!("{:064x}", n as u64 + 1)),
            );
        }
        ingest_signed(&mut engine, &honest, space, claim(far, &honest_hash));
        let kept = pending(&engine, space);
        assert_eq!(kept.len(), 41);
        assert_eq!(
            kept.get(&device_key(&honest)).map(|item| item.seq),
            Some(far)
        );
        assert_eq!(kept.values().filter(|item| item.seq == near).count(), 40);
        assert_eq!(integrity(&engine, space), None);
        extend_until(&mut engine, &author, space, near);
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&honest))
                .map(|item| item.seq),
            Some(far)
        );
        extend_until(&mut engine, &author, space, far);
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_ne!(hash_at(&engine, space, far), honest_hash);
        assert!(!pending(&engine, space).contains_key(&device_key(&honest)));
    }

    #[test]
    fn same_device_higher_claim_replaces_the_lower_and_catches_an_earlier_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-higher";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let prefix = hash_at(&engine, space, 0);
        let lower = engine.next_seq(space) + 12;
        let higher = lower + 10;
        let lower_hash = "55".repeat(32);
        let higher_hash = "66".repeat(32);
        ingest_signed(&mut engine, &peer, space, claim(lower, &lower_hash));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.seq),
            Some(lower)
        );
        ingest_signed(&mut engine, &peer, space, claim(higher, &higher_hash));
        assert_eq!(
            pending(&engine, space).get(&device_key(&peer)),
            Some(&PendingPrefix {
                seq: higher,
                hash: higher_hash.clone(),
                epoch: 1,
                digest: "ab".repeat(32),
            })
        );
        assert_eq!(pending(&engine, space).len(), 1);
        ingest_signed(&mut engine, &peer, space, claim(higher, &"77".repeat(32)));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.hash.clone()),
            Some(higher_hash.clone())
        );
        ingest_signed(&mut engine, &peer, space, claim(lower, &"77".repeat(32)));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.seq),
            Some(higher)
        );
        extend_until(&mut engine, &author, space, higher);
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_ne!(hash_at(&engine, space, higher), higher_hash);
        assert_eq!(hash_at(&engine, space, 0), prefix);
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn two_devices_of_one_member_keep_separate_claims() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-devices";
        start_space(&mut engine, &author, space);
        let identity = Signer::generate();
        let phone = Author::device(&identity, Signer::generate());
        let laptop = Author::device(&identity, Signer::generate());
        add_member(&mut engine, &author, space, &phone);
        let near = engine.next_seq(space) + 8;
        let far = near + 8;
        ingest_signed(&mut engine, &phone, space, claim(near, &"a1".repeat(32)));
        ingest_signed(&mut engine, &laptop, space, claim(far, &"b2".repeat(32)));
        let kept = pending(&engine, space);
        assert_eq!(kept.len(), 2);
        assert_ne!(device_key(&phone), device_key(&laptop));
        assert_eq!(
            kept.get(&device_key(&phone)).map(|item| item.seq),
            Some(near)
        );
        assert_eq!(
            kept.get(&device_key(&laptop)).map(|item| item.seq),
            Some(far)
        );
        ingest_signed(&mut engine, &phone, space, claim(far, &"c3".repeat(32)));
        let kept = pending(&engine, space);
        assert_eq!(kept.len(), 2);
        assert_eq!(
            kept.get(&device_key(&phone)).map(|item| item.hash.clone()),
            Some("c3".repeat(32))
        );
        assert_eq!(
            kept.get(&device_key(&laptop)).map(|item| item.hash.clone()),
            Some("b2".repeat(32))
        );
    }

    #[test]
    fn a_non_member_checkpoint_is_ignored() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-stranger";
        start_space(&mut engine, &author, space);
        ingest_signed(&mut engine, &author, space, post("one"));
        let stranger = Author::root(Signer::generate());
        ingest_signed(&mut engine, &stranger, space, claim(0, &"11".repeat(32)));
        ingest_signed(
            &mut engine,
            &stranger,
            space,
            claim(9_000, &"12".repeat(32)),
        );
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(pending(&engine, space).is_empty());
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        ingest_signed(&mut engine, &peer, space, claim(0, &"11".repeat(32)));
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    }

    #[test]
    fn an_honest_conversation_with_many_members_ends_with_no_pending_prefix() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-honest";
        start_space(&mut engine, &author, space);
        let peers: Vec<Author> = (0..12).map(|_| Author::root(Signer::generate())).collect();
        for peer in &peers {
            add_member(&mut engine, &author, space, peer);
        }
        for round in 0..3 {
            ingest_signed(&mut engine, &author, space, post(&format!("round-{round}")));
            let upto = engine.logs[space].head().unwrap();
            for peer in &peers {
                ingest_signed(
                    &mut engine,
                    peer,
                    space,
                    checkpoint(upto.clone(), &"ab".repeat(32)),
                );
                assert_eq!(integrity(&engine, space), None);
                assert!(pending(&engine, space).is_empty());
            }
        }
        assert_eq!(engine.logs[space].events().len(), 1 + 12 + 3 + 36);
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn a_removed_member_pending_claim_is_kept_until_the_chain_reaches_it() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-removed";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let target = engine.next_seq(space) + 6;
        ingest_signed(&mut engine, &peer, space, claim(target, &"88".repeat(32)));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.seq),
            Some(target)
        );
        ingest_signed(
            &mut engine,
            &author,
            space,
            EventBody::MemberRemoved {
                identity: peer.identity.clone(),
            },
        );
        ingest_signed(
            &mut engine,
            &peer,
            space,
            claim(target + 20, &"99".repeat(32)),
        );
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.seq),
            Some(target)
        );
        assert_eq!(pending(&engine, space).len(), 1);
        assert_eq!(integrity(&engine, space), None);
        extend_until(&mut engine, &author, space, target);
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert!(pending(&engine, space).is_empty());
    }

    #[test]
    fn pending_prefix_ceiling_refuses_a_new_device_without_evicting() {
        let (mut engine, author, _) = member(":memory:");
        let space = "prefix-ceiling";
        start_space(&mut engine, &author, space);
        let held = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &held);
        let mut planted = BTreeMap::new();
        planted.insert(
            device_key(&held),
            PendingPrefix {
                seq: 40_000,
                hash: "11".repeat(32),
                epoch: 1,
                digest: "ab".repeat(32),
            },
        );
        for n in 1..PENDING_PREFIX_CEILING {
            planted.insert(
                format!("device-{n:04}"),
                PendingPrefix {
                    seq: 40_000,
                    hash: format!("{n:064x}"),
                    epoch: 1,
                    digest: "ab".repeat(32),
                },
            );
        }
        assert_eq!(planted.len(), PENDING_PREFIX_CEILING);
        engine
            .store
            .set_meta(
                &pending_prefix_meta(space),
                &serde_json::to_string(&planted).unwrap(),
            )
            .unwrap();
        ingest_signed(&mut engine, &held, space, claim(50_000, &"22".repeat(32)));
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&held))
                .map(|item| item.seq),
            Some(50_000)
        );
        assert_eq!(pending(&engine, space).len(), PENDING_PREFIX_CEILING);
        let extra = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &extra);
        ingest_signed(&mut engine, &extra, space, claim(50_001, &"aa".repeat(32)));
        let kept = pending(&engine, space);
        assert_eq!(kept.len(), PENDING_PREFIX_CEILING);
        assert!(!kept.contains_key(&device_key(&extra)));
        assert_eq!(
            kept.get(&device_key(&held)).map(|item| item.seq),
            Some(50_000)
        );
        assert_eq!(kept.get("device-0001").map(|item| item.seq), Some(40_000));
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.pending_prefix_refusals(space), 1);
    }

    fn deliver(engine: &mut Engine, event: Event) -> Ingest {
        engine.ingest(ordered(engine, Envelope::plain(&event)))
    }

    #[test]
    fn a_member_seen_at_or_beyond_the_event_seq_is_a_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "seen-ahead";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let next = engine.next_seq(space);
        let event = peer.sign_event(
            space,
            "seen-ahead",
            1_700_000_000_100,
            Some(roda_types::Seen {
                seq: 9_000,
                hash: "12".repeat(32),
            }),
            claim(9_000, &"12".repeat(32)),
        );
        let got = deliver(&mut engine, event);
        assert!(9_000 >= next);
        assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
        assert_eq!(engine.next_seq(space), next);
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        assert_eq!(
            engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
            Some(FORK)
        );
        assert!(engine.logs[space]
            .events()
            .iter()
            .all(|e| e.client_id != "seen-ahead"));
    }

    #[test]
    fn an_event_ahead_of_the_local_head_stays_a_gap() {
        let (mut engine, author, _) = member(":memory:");
        let space = "real-gap";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let next = engine.next_seq(space);
        let seen_seq = next - 1;
        let seen_hash = hash_at(&engine, space, seen_seq);
        let event = peer.sign_event(
            space,
            "real-gap",
            1_700_000_000_100,
            Some(roda_types::Seen {
                seq: seen_seq,
                hash: seen_hash.clone(),
            }),
            claim(seen_seq, &seen_hash),
        );
        let mut ev = ordered(&engine, Envelope::plain(&event));
        ev.seq = next + 3;
        let got = engine.ingest(ev);
        assert!(
            matches!(got, Ingest::Gap { next: n } if n == next),
            "{got:?}"
        );
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    }

    #[test]
    fn a_non_member_seen_beyond_the_event_is_refused_without_a_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "seen-stranger-ahead";
        start_space(&mut engine, &author, space);
        let stranger = Author::root(Signer::generate());
        let event = stranger.sign_event(
            space,
            "seen-stranger-ahead",
            1_700_000_000_100,
            Some(roda_types::Seen {
                seq: 9_000,
                hash: "12".repeat(32),
            }),
            claim(9_000, &"12".repeat(32)),
        );
        let got = deliver(&mut engine, event);
        assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(engine.logs[space]
            .events()
            .iter()
            .all(|e| e.client_id != "seen-stranger-ahead"));
    }

    #[test]
    fn a_forged_signature_is_refused_without_a_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "seen-forged";
        start_space(&mut engine, &author, space);
        let peer = Author::root(Signer::generate());
        add_member(&mut engine, &author, space, &peer);
        let mut event = peer.sign_event(
            space,
            "seen-forged",
            1_700_000_000_100,
            Some(roda_types::Seen {
                seq: 0,
                hash: "11".repeat(32),
            }),
            claim(0, &"11".repeat(32)),
        );
        event.sig = "00".repeat(64);
        let got = deliver(&mut engine, event);
        assert!(matches!(got, Ingest::Invalid(_)));
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(engine.logs[space]
            .events()
            .iter()
            .all(|e| e.client_id != "seen-forged"));
    }

    #[test]
    fn a_non_member_diverged_seen_is_refused_without_a_fork() {
        let (mut engine, author, _) = member(":memory:");
        let space = "seen-stranger";
        start_space(&mut engine, &author, space);
        ingest_signed(&mut engine, &author, space, post("one"));
        let stranger = Author::root(Signer::generate());
        let event = stranger.sign_event(
            space,
            "seen-stranger",
            1_700_000_000_100,
            Some(roda_types::Seen {
                seq: 0,
                hash: "11".repeat(32),
            }),
            claim(0, &"11".repeat(32)),
        );
        let got = deliver(&mut engine, event);
        assert!(matches!(got, Ingest::Invalid(_)));
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
        assert!(pending(&engine, space).is_empty());
        assert!(engine.logs[space]
            .events()
            .iter()
            .all(|e| e.client_id != "seen-stranger"));
    }

    #[test]
    fn a_crash_after_fork_meta_still_reports_the_fork_after_reopen() {
        set_crash_after_fork_meta(false);
        let dir = std::env::temp_dir().join(format!("u18-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("device.sqlite");
        let _ = std::fs::remove_file(&path);
        let path = path.to_str().unwrap().to_string();
        let space = "prefix-crash";
        let device_secret = {
            let (mut engine, author, device_secret) = member(&path);
            start_space(&mut engine, &author, space);
            let peer = Author::root(Signer::generate());
            add_member(&mut engine, &author, space, &peer);
            let mut stored = BTreeMap::new();
            stored.insert(
                device_key(&peer),
                PendingPrefix {
                    seq: 0,
                    hash: "11".repeat(32),
                    epoch: 1,
                    digest: "ab".repeat(32),
                },
            );
            engine
                .store
                .set_meta(
                    &pending_prefix_meta(space),
                    &serde_json::to_string(&stored).unwrap(),
                )
                .unwrap();
            set_crash_after_fork_meta(true);
            let failed = engine.settle_pending_prefixes(space);
            set_crash_after_fork_meta(false);
            assert!(failed.is_err());
            assert_eq!(
                engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
                Some(FORK)
            );
            assert!(engine
                .store
                .meta(&pending_prefix_meta(space))
                .unwrap()
                .is_some());
            device_secret
        };
        let mut engine = Engine::open(&path).unwrap();
        assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pending_claim_settles_through_a_pruned_stub() {
        let (mut engine, author, _) = member(":memory:");
        let peer = Author::root(Signer::generate());
        for (space, mismatch) in [("prefix-pruned-match", false), ("prefix-pruned-fork", true)] {
            start_space(&mut engine, &author, space);
            add_member(&mut engine, &author, space, &peer);
            let frame = pruned_frame(&engine, &author, space);
            let claimed = if mismatch {
                "11".repeat(32)
            } else {
                frame.hash.clone()
            };
            let mut stored = BTreeMap::new();
            stored.insert(
                device_key(&peer),
                PendingPrefix {
                    seq: frame.seq,
                    hash: claimed,
                    epoch: 1,
                    digest: "ab".repeat(32),
                },
            );
            engine
                .store
                .set_meta(
                    &pending_prefix_meta(space),
                    &serde_json::to_string(&stored).unwrap(),
                )
                .unwrap();
            assert!(matches!(
                engine.ingest(frame),
                Ingest::Applied | Ingest::Confirmed
            ));
            assert!(pending(&engine, space).is_empty());
            if mismatch {
                assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
            } else {
                assert_eq!(integrity(&engine, space), None);
                assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
            }
        }
    }

    fn pruned_frame(engine: &Engine, author: &Author, space: &str) -> Sequenced {
        let seen = engine.logs.get(space).and_then(SpaceLog::head);
        let seq = engine.next_seq(space);
        let sealed = Envelope::sealed(
            author,
            space,
            &format!("{space}-pruned-{seq}"),
            1_700_000_000_000 + seq as i64,
            seen.as_ref(),
            roda_log::content::Sealed::new(
                roda_log::content::SealedKind::Application,
                1,
                vec![7, 7, 7, 7],
            ),
        );
        ordered(
            engine,
            sealed.pruned().expect("a sealed event leaves a stub"),
        )
    }

    #[test]
    fn concurrent_sealed_siblings_send_until_a_member_proves_a_fork() {
        let dir = std::env::temp_dir().join(format!("u18-r6-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("device.sqlite");
        let path = path.to_str().unwrap().to_string();
        let space = "sp-sealed-siblings";
        let device_secret = {
            let (mut engine, author, device_secret) = member(&path);
            start_space(&mut engine, &author, space);
            let peer = Author::root(Signer::generate());
            add_member(&mut engine, &author, space, &peer);

            let first = ordered(
                &engine,
                Envelope::sealed(
                    &peer,
                    space,
                    "peer-only",
                    1_700_000_000_050,
                    seen(&engine, space).as_ref(),
                    Sealed::new(SealedKind::Application, SUITE_ID, vec![1]),
                ),
            );
            assert_eq!(engine.ingest(first), Ingest::Applied);
            assert_eq!(integrity(&engine, space), None);

            let head = seen(&engine, space).unwrap();
            let (content, sig) = author.sign_sealed(
                space,
                "own-successor",
                1_700_000_000_100,
                Some(&head),
                Sealed::new(SealedKind::Application, SUITE_ID, vec![2, 2]),
            );
            let own = event_from_content(
                content,
                sig,
                author.cert.clone(),
                0,
                String::new(),
                String::new(),
            )
            .unwrap();
            engine.store.outbox_put(&own).unwrap();
            let before = engine.store.outbox().unwrap().len();
            assert_eq!(before, 1);
            assert!(!engine.store.outbox().unwrap()[0].failed);

            let frame = ordered(
                &engine,
                Envelope::sealed(
                    &peer,
                    space,
                    "peer-successor",
                    1_700_000_000_200,
                    Some(&head),
                    Sealed::new(SealedKind::Application, SUITE_ID, vec![3, 3, 3]),
                ),
            );
            let got = engine.ingest(frame.clone());
            assert!(
                matches!(got, Ingest::Applied),
                "canonical sibling was not stored: {got:?}"
            );
            let stored = engine.logs[space].events().last().unwrap();
            assert_eq!(stored.hash, frame.hash);
            assert_eq!(stored.author, peer.identity);
            assert_eq!(stored.seen.as_ref(), Some(&head));

            assert_eq!(integrity(&engine, space), None);
            assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
            let sent = engine.send_message(space, "concurrent send").unwrap();
            assert!(matches!(
                sent.kind,
                crate::dto::EntryKind::Message { ref text, .. } if text == "concurrent send"
            ));
            assert_eq!(engine.store.outbox().unwrap().len(), before + 1);
            assert_eq!(
                engine.store.outbox_get("own-successor").unwrap(),
                Some(own.clone())
            );
            let published = engine.outbox_envelopes();
            assert_eq!(published.len(), 1);
            assert_eq!(published[0].client_id(), "own-successor");
            engine.reproject_approval_overlay().unwrap();
            assert_eq!(integrity(&engine, space), None);
            assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);

            let next = engine.next_seq(space);
            let diverged = roda_types::Seen {
                seq: head.seq,
                hash: "00".repeat(32),
            };
            assert_ne!(diverged.hash, head.hash);
            let conflicting = ordered(
                &engine,
                Envelope::sealed(
                    &peer,
                    space,
                    "peer-conflicting-prefix",
                    1_700_000_000_300,
                    Some(&diverged),
                    Sealed::new(SealedKind::Application, SUITE_ID, vec![4, 4, 4, 4]),
                ),
            );
            conflicting.env.verify().unwrap();
            let got = engine.ingest(conflicting);
            assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
            assert_eq!(engine.next_seq(space), next);
            assert!(engine.logs[space]
                .events()
                .iter()
                .all(|event| event.client_id != "peer-conflicting-prefix"));
            let before = engine.store.outbox().unwrap().len();
            let phrase = integrity(&engine, space);
            let sent = engine.send_message(space, "again");
            let after = engine.store.outbox().unwrap().len();
            let published = engine.outbox_envelopes().len();
            assert_eq!(
                (phrase.as_deref(), sent.is_err(), after, published),
                (Some(FORK), true, before, 0),
                "{sent:?}"
            );
            assert_eq!(
                engine.store.outbox().unwrap()[0].event.client_id,
                "own-successor"
            );
            assert!(!engine.store.outbox().unwrap()[0].failed);
            assert_eq!(
                engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
                Some(FORK)
            );
            let queued = engine.store.outbox().unwrap().len();
            engine.reproject_approval_overlay().unwrap();
            let replayed = integrity(&engine, space);
            let resent = engine.send_message(space, "after replay");
            let grown = engine.store.outbox().unwrap().len();
            let refused = match &resent {
                Err(crate::CoreError::Invalid { reason }) => reason.clone(),
                other => format!("{other:?}"),
            };
            assert_eq!(
                (replayed.as_deref(), refused.as_str(), grown),
                (Some(FORK), FORK, queued),
                "{resent:?}"
            );
            device_secret
        };

        let mut engine = Engine::open(&path).unwrap();
        assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
        assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
        let before = engine.store.outbox().unwrap().len();
        let sent = engine.send_message(space, "after reopen");
        assert!(sent.is_err(), "{sent:?}");
        assert_eq!(sent.unwrap_err().to_string(), FORK);
        assert_eq!(engine.store.outbox().unwrap().len(), before);
        assert!(engine.outbox_envelopes().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
