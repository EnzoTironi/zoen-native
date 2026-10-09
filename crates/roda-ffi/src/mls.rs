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
use roda_mls::{sealed::state_key, Device, Leaf, MlsError, Opened, SUITE_ID};
use roda_proto::{Envelope, KeyPackageRecord, Sequenced};
use roda_types::*;
use rusqlite::Connection;

use crate::engine::Engine;
use crate::i18n::t;
use crate::net::tracing_like;
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

fn mls_err(e: MlsError) -> CoreError {
    CoreError::Invalid {
        reason: e.to_string(),
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
    fn device_on<'c>(&self, conn: &'c Connection) -> R<Device<'c>> {
        let (Some(author), Some(acct)) = (&self.net.author, &self.net.account) else {
            return Err(CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            });
        };
        let secret = author.key.secret();
        Device::new(conn, state_key(&secret), &acct.identity, secret, &acct.cert).map_err(mls_err)
    }

    fn device(&self) -> R<Device<'_>> {
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

    fn roster(&self, space: &str) -> BTreeSet<IdentityId> {
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
    fn ready_epoch(&self, space: &str) -> Option<u64> {
        let device = self.device().ok()?;
        if !device.has_group(space) || device.pending(space) {
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
    fn queue_handshake(&mut self, space: &str, kind: SealedKind, data: Vec<u8>) -> R<String> {
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
        let (content, sig) = author.sign_sealed(
            space,
            &client_id,
            at_ms,
            seen.as_ref(),
            Sealed::new(kind, SUITE_ID, data),
        );
        let e = event_from_content(
            content,
            sig,
            author.cert.clone(),
            0,
            String::new(),
            String::new(),
        )
        .map_err(storage)?;
        self.store.outbox_put(&e)?;
        self.net
            .pending
            .insert(client_id.clone(), space.to_string());
        self.net.wake();
        Ok(client_id)
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
        if ev.env.is_pruned() {
            return self.ingest_pruned(ev);
        }
        let Some((kind, data)) = ev.env.sealed_data().map(|(k, d)| (k, d.to_vec())) else {
            return Ingest::Invalid("not sealed".into());
        };
        let client_id = ev.env.client_id().to_string();
        let own = self.net.pending.contains_key(&client_id);
        let roster = self.roster(&space);
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
                match device.open(&space, &data, &roster) {
                    Ok(Opened::Application { plaintext, from }) => {
                        match inner_event(&ev, &plaintext, &from) {
                            Some(e) => opened = Some(e),
                            None => tracing_like(&format!(
                                "sealed message in {space} doesn't match its envelope; kept sealed"
                            )),
                        }
                    }
                    Ok(_) => {}
                    Err(e) => tracing_like(&format!("can't open a message in {space}: {e}")),
                }
            }
            SealedKind::Application => {}
            SealedKind::Commit => match device.open(&space, &data, &roster) {
                Ok(Opened::Commit { epoch }) => landed = Some((epoch, false)),
                Ok(_) => {}
                Err(e) => tracing_like(&format!("refused a commit in {space}: {e}")),
            },
            SealedKind::Welcome if !device.has_group(&space) => {
                match device.join(&space, &data, &roster) {
                    Ok(true) => landed = device.epoch(&space).ok().map(|ep| (ep, true)),
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("refused a welcome in {space}: {e}")),
                }
            }
            SealedKind::Welcome | SealedKind::Unspecified => {}
        }
        let digest = landed.and_then(|_| device.checkpoint(&space).ok());
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
        if let Some((epoch, digest)) = &digest {
            stored = stored.and_then(|_| {
                self.store
                    .set_meta(&digest_meta(&space, *epoch), digest)
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

        if own {
            self.net.pending.remove(&client_id);
            let _ = self.store.outbox_remove(&client_id);
            let _ = self.store.meta_delete(&sealed_meta(&client_id));
            let _ = self.store.meta_delete(&welcome_meta(&client_id));
            if kind == SealedKind::Welcome {
                // The group may owe a commit that waited for this Welcome to land.
                self.net.mls.dirty.insert(space.clone());
            }
            if let Some(s) = self.state.spaces.get_mut(&space) {
                for entry in s.entries.iter_mut().filter(|x| x.client_id == client_id) {
                    entry.seq = e.seq;
                    entry.hash = e.hash.clone();
                }
            }
            return Ingest::Confirmed;
        }
        self.state.apply(&e);
        self.index_dirty = true;
        self.note_unknown(&e);
        Ingest::Applied
    }

    /// What a pruned entry leaves (ADR 0026): only a device that joined later reads it, and
    /// it predates its Welcome, so there is nothing to open. It keeps the chain whole: the
    /// stub links by the original's wire hash, which members' signed checkpoints pin.
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
        e.sealed_wire = ev.env.pruned_wire().map(str::to_string);
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
        if let Err(err) = self.store.append_event(&e) {
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        let _ = self.store.set_synced(&space);
        self.net.synced.insert(space);
        Ingest::Applied
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
        tracing_like(&format!("{kind} in {} refused: {reason}", e.space));
        if kind == SealedKind::Commit.name() {
            // Its Welcome, held behind it, has no commit to follow any more.
            let orphans: Vec<String> = self
                .store
                .outbox()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.event)
                .filter(|o| {
                    o.space == e.space
                        && matches!(&o.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name())
                })
                .map(|o| o.client_id)
                .collect();
            for id in orphans {
                let _ = self.store.outbox_remove(&id);
                let _ = self.store.meta_delete(&welcome_meta(&id));
                self.net.pending.remove(&id);
            }
            if let Err(err) = self
                .device()
                .and_then(|d| d.abandon(&e.space).map_err(mls_err))
            {
                tracing_like(&format!("dropping the commit in {}: {err}", e.space));
            }
        } else if kind == SealedKind::Welcome.name() {
            // Its commit landed but another got in before the Welcome: the newcomers' leaves
            // are in our group with no way in, so they get taken out and added afresh.
            let key = welcome_meta(client_id);
            let stranded = identity_set(self.store.meta(&key).ok().flatten());
            let _ = self.store.meta_delete(&key);
            if !stranded.is_empty() {
                let key = rewelcome_meta(&e.space);
                let mut all = identity_set(self.store.meta(&key).ok().flatten());
                all.extend(stranded);
                let _ = self.store.set_meta(&key, &join_set(&all));
            }
        }
        let _ = self.store.outbox_remove(client_id);
        self.net.pending.remove(client_id);
        self.net.mls.dirty.insert(e.space);
        true
    }

    /// The next group that owes a commit adding someone: claim their key packages. Groups
    /// that only owe removals commit right here.
    pub fn mls_to_claim(&mut self) -> Option<(SpaceId, Vec<IdentityId>)> {
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
        for space in dirty {
            self.settle_rewelcome(&space);
            if let Some(turn) = self.admin_turn(&space) {
                self.net.mls.dirty.remove(&space);
                self.net.mls.turn_at.insert(space, now + turn);
                continue;
            }
            match self.owed(&space) {
                Ok(Some((add, remove))) if add.is_empty() => {
                    self.net.mls.dirty.remove(&space);
                    if let Err(e) = self.commit_now(&space, &[], &remove) {
                        tracing_like(&format!("removal commit in {space}: {e}"));
                    }
                }
                Ok(Some((add, _))) => {
                    self.net.mls.dirty.remove(&space);
                    self.net.mls.claiming.insert(space.clone());
                    return Some((space, add.into_iter().collect()));
                }
                Ok(None) => {
                    self.net.mls.dirty.remove(&space);
                }
                Err(e) => {
                    self.net.mls.dirty.remove(&space);
                    tracing_like(&format!("group check in {space}: {e}"));
                }
            }
        }
        None
    }

    /// How long this device waits before committing an owed change in `space`: one stagger
    /// per admin ahead of it in identity order. `None` once it has waited, when it is first,
    /// or when nothing is owed.
    fn admin_turn(&mut self, space: &str) -> Option<Duration> {
        if !matches!(self.owed(space), Ok(Some(_))) {
            self.net.mls.waited.remove(space);
            return None;
        }
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
        let Ok(group) = device.roster(space).map_err(mls_err) else {
            return;
        };
        let left: BTreeSet<_> = waiting.intersection(&group).cloned().collect();
        let _ = if left.is_empty() {
            self.store.meta_delete(&key)
        } else {
            self.store.set_meta(&key, &join_set(&left))
        };
    }

    /// What `space`'s group owes: (identities to add, identities to remove), when this
    /// device may commit and has nothing out already.
    fn owed(&self, space: &str) -> R<Option<(BTreeSet<IdentityId>, BTreeSet<IdentityId>)>> {
        let me = self.me_id()?;
        let Some(roster) = self.net.mls.rosters.get(space) else {
            return Ok(None);
        };
        if !matches!(roster.get(&me), Some(Role::Owner | Role::Admin)) {
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
        let device = self.device()?;
        if !device.has_group(space) || device.pending(space) {
            return Ok(None);
        }
        let group = device.roster(space).map_err(mls_err)?;
        let listed: BTreeSet<IdentityId> = roster.keys().cloned().collect();
        let add: BTreeSet<_> = listed.difference(&group).cloned().collect();
        let mut remove: BTreeSet<_> = group.difference(&listed).cloned().collect();
        let stranded = identity_set(self.store.meta(&rewelcome_meta(space))?);
        remove.extend(stranded.intersection(&group).cloned());
        Ok((!add.is_empty() || !remove.is_empty()).then_some((add, remove)))
    }

    /// Key packages for a group's newcomers came back: commit them (and any removals).
    pub fn mls_claimed(&mut self, space: &str, result: Result<Vec<KeyPackageRecord>, String>) {
        self.net.mls.claiming.remove(space);
        let owed = match self.owed(space) {
            Ok(Some(o)) => o,
            _ => return,
        };
        let (add, remove) = owed;
        // Only packages whose verified leaf is one of the people we asked for.
        let records: Vec<KeyPackageRecord> = result
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                roda_mls::key_package_leaf(&r.data).is_ok_and(|l: Leaf| {
                    l.identity == r.identity && l.device == r.device && add.contains(&l.identity)
                })
            })
            .collect();
        let found: BTreeSet<&IdentityId> = records.iter().map(|r| &r.identity).collect();
        if add.iter().any(|who| !found.contains(who)) {
            // Someone can't be added yet (no device, or none with packages): messages
            // don't wait for them; they'll read from whenever their commit lands.
            self.net.mls.stuck.insert(space.to_string());
        }
        let packages: Vec<Vec<u8>> = records.into_iter().map(|r| r.data).collect();
        if packages.is_empty() && remove.is_empty() {
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
            return;
        }
        if let Err(e) = self.commit_now(space, &packages, &remove) {
            tracing_like(&format!("commit in {space}: {e}"));
            self.net
                .mls
                .retry_at
                .insert(space.to_string(), Instant::now() + CLAIM_RETRY);
        }
    }

    fn commit_now(&mut self, space: &str, add: &[Vec<u8>], remove: &BTreeSet<IdentityId>) -> R<()> {
        let c = self.device()?.commit(space, add, remove).map_err(mls_err)?;
        self.queue_handshake(space, SealedKind::Commit, c.commit)?;
        if let Some(w) = c.welcome {
            let welcome = self.queue_handshake(space, SealedKind::Welcome, w)?;
            let newcomers: BTreeSet<IdentityId> = add
                .iter()
                .filter_map(|kp| roda_mls::key_package_leaf(kp).ok())
                .map(|l| l.identity)
                .collect();
            self.store
                .set_meta(&welcome_meta(&welcome), &join_set(&newcomers))?;
        }
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

    /// The relay refused a message sealed at an epoch the group has left: this device is
    /// behind a commit it hasn't applied yet. It stops sealing in that Space until it has.
    pub(crate) fn mls_sealed_stale(&mut self, client_id: &str) {
        let Some(space) = self.net.pending.get(client_id).cloned() else {
            return;
        };
        if let Some(epoch) = self.device().ok().and_then(|d| d.epoch(&space).ok()) {
            self.net.mls.behind.insert(space, epoch);
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
            && m.turn_at.is_empty()
            && m.checkpoint_due.is_empty()
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
