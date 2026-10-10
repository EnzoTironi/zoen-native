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
//!   was shown another group: the Space is marked as forked.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

/// What a group owes, as one commit: identities to add (every device of theirs), single
/// devices to add (identity already in), and removals (identities or leaf names).
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Owed {
    add: BTreeSet<IdentityId>,
    add_devices: BTreeSet<(IdentityId, String)>,
    remove: BTreeSet<String>,
}

impl MlsNet {
    pub(crate) fn mark_dirty(&mut self, spaces: Vec<SpaceId>) {
        self.dirty.extend(spaces);
    }
}

impl Owed {
    fn is_empty(&self) -> bool {
        self.add.is_empty() && self.add_devices.is_empty() && self.remove.is_empty()
    }

    fn adds_nobody(&self) -> bool {
        self.add.is_empty() && self.add_devices.is_empty()
    }

    /// Whose key packages a claim asks for.
    fn to_claim(&self) -> Vec<IdentityId> {
        let mut ids = self.add.clone();
        ids.extend(self.add_devices.iter().map(|(id, _)| id.clone()));
        // The relay serves up to fifty distinct identities per operation. Remaining
        // members reconcile after this batch's commit/Welcome lands.
        ids.into_iter().take(50).collect()
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
                if let Some(s) = self.state.spaces.get_mut(&space) {
                    s.integrity_error = Some(reason);
                }
            }
            self.net.mls.dirty.insert(space);
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
        self.net.mls.dirty.insert(e.space.clone());
    }

    /// The group for a Space this device just created (it's the only member).
    pub(crate) fn create_mls_group(&mut self, space: &str) -> R<()> {
        self.device()?.create_group(space).map_err(mls_err)?;
        self.net.mls.dirty.insert(space.to_string());
        Ok(())
    }

    /// The epoch to seal `space`'s messages at, once its group is ready for them.
    pub(crate) fn ready_epoch(&self, space: &str) -> Option<u64> {
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
        let s = self.state.spaces.get(space)?;
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

    /// The sealed copy of a queued event: the epoch it was sealed at, and the envelope.
    fn sealed_copy(&self, e: &Event) -> Option<(u64, Envelope)> {
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
            self.net.mls.dirty.insert(e.space);
        }
        self.net.wake();
    }

    /// A sealed entry the relay sequenced: opened (or kept sealed) and logged in one
    /// transaction with the group state it changes.
    pub(crate) fn ingest_sealed(&mut self, ev: Sequenced) -> Ingest {
        let space = ev.env.space().to_string();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if ev.seq < next {
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
        let own = self.net.pending.contains_key(&client_id)
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
        let mut landed: Option<(u64, bool)> = None; // (epoch, joined)
        match kind {
            SealedKind::Application if opened.is_none() => {
                match device.open(&space, &data, &roster, &admitted_sender) {
                    Ok(Opened::Application { plaintext, from }) => {
                        match inner_event(&ev, &plaintext, &from) {
                            Some(e) => opened = Some(e),
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
        let log = self
            .logs
            .entry(space.clone())
            .or_insert_with(|| SpaceLog::new(space.clone()));
        if let Err(err) = log.accept(e.clone()) {
            if log.is_empty() {
                self.logs.remove(&space);
            }
            return Ingest::Invalid(err.to_string()); // the transaction rolls back
        }
        let mut stored = self.store.append_event(&e).map_err(storage);
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

        let _ = self.store.set_synced(&space);
        self.net.synced.insert(space.clone());
        let m = &mut self.net.mls;
        if landed.is_some() {
            m.dirty.insert(space.clone());
        }
        if matches!(landed, Some((_, true))) || (own && landed.is_some()) {
            m.checkpoint_due.insert(space.clone());
        }
        if kind == SealedKind::Commit && landed.is_none() {
            // Refused or stale: the group may still owe this Space a commit.
            m.dirty.insert(space.clone());
        }
        if e.seq >= self.checkpointed_at(&space) + checkpoint_every()
            && self.device().is_ok_and(|d| d.has_group(&space))
        {
            self.net.mls.checkpoint_due.insert(space.clone());
        }

        if self.approval_projection_needs_replay(&e) {
            if own {
                self.net.pending.remove(&client_id);
            }
            if let Err(err) = self.reproject_approval_overlay() {
                let _ = self.reload();
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
                self.net.mls.dirty.insert(space.clone());
            }
            if let Some(s) = self.state.spaces.get_mut(&space) {
                for entry in s.entries.iter_mut().filter(|x| {
                    x.client_id == client_id
                        || x.client_id.strip_suffix(":result") == Some(client_id.as_str())
                }) {
                    entry.seq = e.seq;
                    entry.hash = e.hash.clone();
                }
            }
            return Ingest::Confirmed;
        }
        self.state.apply(&e, &self.identities);
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
        let log = self
            .logs
            .entry(space.clone())
            .or_insert_with(|| SpaceLog::new(space.clone()));
        if let Err(err) = log.accept(e.clone()) {
            if log.is_empty() {
                self.logs.remove(&space);
            }
            return Ingest::Invalid(err.to_string());
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
        self.net.synced.insert(space);
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
        self.net.mls.dirty.insert(e.space.clone());
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

    /// Another member's checkpoint: if this device was at that epoch and computed a
    /// different digest, the two were shown different groups.
    pub(crate) fn compare_checkpoint(&mut self, e: &Event) {
        let EventBody::Checkpoint { epoch, digest, .. } = &e.body else {
            return;
        };
        if self.me.as_deref() == Some(e.author.as_str()) {
            return;
        }
        let Ok(Some(mine)) = self.store.meta(&digest_meta(&e.space, *epoch)) else {
            return;
        };
        if mine != *digest {
            let reason = t(
                "Membros veem grupos diferentes nesta conversa (possível bifurcação do servidor).",
                "Members see different groups in this chat (the server may have forked it).",
            );
            let _ = self.store.set_meta(&fork_meta(&e.space), &reason);
            if let Some(s) = self.state.spaces.get_mut(&e.space) {
                s.integrity_error = Some(reason);
            }
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
        self.net.mls.dirty.insert(e.space);
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
            self.net.mls.dirty.insert(s);
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
            self.net.mls.dirty.insert(s);
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
        self.net.mls.dirty.extend(interrupted);
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
        let group: BTreeSet<IdentityId> = leaves.keys().map(|(id, _)| id.clone()).collect();
        let mut owed = Owed::default();
        if admin || reconciling {
            let listed: BTreeSet<IdentityId> = roster.keys().cloned().collect();
            if admin {
                owed.add = listed.difference(&group).cloned().collect();
            }
            owed.remove = group.difference(&listed).cloned().collect();
        }
        let has = |id: &str, dev: &str| leaves.get(&(id.to_string(), dev.to_string()));
        // Our unlinked devices leave every group we are in.
        for (id, dev) in leaves.keys() {
            if *id == me && revoked.contains(dev) {
                owed.remove.insert(leaf_name(id, dev));
            }
        }
        for name in stranded.iter().filter(|n| for_me(n)) {
            if split_leaf(name).is_some_and(|(id, dev)| has(id, dev).is_some()) {
                owed.remove.insert(name.clone());
            }
        }
        for (name, stale) in joins.iter().filter(|(n, _)| for_me(n)) {
            let Some((id, dev)) = split_leaf(name) else {
                continue;
            };
            if !roster.contains_key(id) || owed.add.contains(id) || owed.remove.contains(id) {
                continue;
            }
            match has(id, dev) {
                Some(key) if hex::encode(key) == *stale => {
                    owed.remove.insert(name.clone());
                }
                Some(_) => {}
                None => {
                    owed.add_devices.insert((id.to_string(), dev.to_string()));
                }
            }
        }
        Ok((!owed.is_empty() || reconciling).then_some(owed))
    }

    /// Key packages for a group's newcomers came back: commit them (and any removals).
    pub fn mls_claimed(
        &mut self,
        space: &str,
        operation: &str,
        result: Result<Vec<KeyPackageRecord>, String>,
    ) {
        self.net.mls.claiming.remove(space);
        let mut claim = match self.load_claim(space) {
            Ok(Some(c)) if c.operation_id == operation => c,
            _ => {
                self.net.mls.dirty.insert(space.to_string());
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
                self.net.mls.dirty.insert(space.to_string());
                return;
            }
        };
        if owed != claim.intent {
            // A membership change while the reply was in flight is a new operation.
            self.net.mls.dirty.insert(space.to_string());
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
    use roda_log::{chain_hash, Author};

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
}
