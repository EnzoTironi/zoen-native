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

use ::mls::{leaf_name, sealed::state_key, Device, Leaf, MlsError, Opened, SUITE_ID};
use event_log::content::{InnerEvent, Sealed, SealedKind};
use event_log::{event_from_content, SpaceLog};
use protocol::{
    Envelope, KeyPackageClaimClock, KeyPackageRecord, Sequenced, KEY_PACKAGE_CLAIM_EXPIRED,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use types::*;

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

mod checkpoints;
mod claims;
mod ingestion;
mod joins;
mod outgoing;
mod packages;
mod prefix;
mod readiness;

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
    /// Groups where someone listed had no key packages: existing readers may proceed.
    /// Direct chats still wait for every logical member to be able to read.
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
    /// Transient download hints from recovery classification, never group authority.
    pub(crate) recovery_downloads: HashMap<SpaceId, crate::mls_recovery::ContextRef>,
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

use ::mls::reconciliation::{self, Owed};

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
mod tests;

pub(crate) use readiness::ReadinessRejection;
