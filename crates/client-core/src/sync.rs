//! # Engine side of Zoen Sync
//!
//! - **Account:** an Ed25519 identity key and a device key certified by it. Secrets live
//!   in the platform vault (iOS Keychain); SQLite keeps only public keys and the cert.
//! - **Outbox:** writing to a relay-ordered Space signs the event, queues it in SQLite
//!   and applies it to the projection at once (shown as *Sending*). The relay assigns the
//!   sequence number when the device is online; the confirmed copy replaces the pending
//!   one in place (same `client_id`, so the UI row never jumps).
//! - **Ingest:** events from the relay must be the next in their Space, link to the head
//!   and carry a valid author signature, or they are refused. A member whose signed
//!   `seen` names a position this device already holds under a different hash is fork
//!   evidence: the Space is marked and the event is not stored. So is a member
//!   event that is already the next sequence when its signed `seen` is not held.
//!   An honest author signed `seen` below the sequence the relay assigns, and
//!   sync does not ask for that event again. An event whose own sequence is
//!   still ahead of the local head stays a gap. A sealed application stored
//!   while another author's successor of the same `seen` is still queued
//!   marks that fork too. Queued rows for a forked Space are not published,
//!   including rows that have not failed. `seen` lives in the signed
//!   content. The relay's `seq`, `prev` and chain hash do not.
//! - **Reopen:** confirmed events are re-verified from disk, then the outbox is replayed
//!   on top. Kill the app mid-send and nothing is lost or duplicated.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use event_log::content::SealedKind;
use event_log::{chain_hash, content_hash_of, Author, Signer, SpaceLog};
use protocol::{normalize_handle, Cursor, Envelope, Sequenced};
use serde::{Deserialize, Serialize};
use types::*;

use crate::dto::Delivery;
use crate::engine::{Engine, Entry, State};
use crate::i18n::t;
use crate::pages::{PlanReplay, Projection};
use crate::CoreError;

type R<T> = Result<T, CoreError>;

pub(crate) const VAULT_IDENTITY: &str = "zoen.identity.v1";
pub(crate) const VAULT_DEVICE: &str = "zoen.device.v1";

/// Keys are prepared by the caller before installation. This borrowed internal
/// input neither loads a vault nor creates fresh keys while replaying a setup.
pub(crate) struct PreparedAccount<'a> {
    pub(crate) root: &'a [u8; 32],
    pub(crate) device: &'a [u8; 32],
    pub(crate) agreement: &'a [u8; 32],
    pub(crate) name: &'a str,
    pub(crate) handle: &'a str,
    pub(crate) relay: &'a str,
    pub(crate) agent: Option<(&'a str, &'a types::AgentOwnerProof)>,
}
pub(crate) const EVENT_FORMAT_META: &str = "event_format";
/// Format v3 (signed protobuf bytes). Older logs were signed over JSON and restart.
pub(crate) const EVENT_FORMAT_VALUE: &str = "3";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountMeta {
    pub identity: IdentityId,
    pub device: String,
    pub cert: String,
    pub relay_url: String,
    pub registered: bool,
}

#[derive(Default)]
pub struct NetState {
    pub(crate) file_demands: crate::file_demand::FileDemands,
    pub account: Option<AccountMeta>,
    /// The unlocked device key (loaded from the vault).
    pub author: Option<Author>,
    pub synced: HashSet<SpaceId>,
    /// client_id → Space, for events in the outbox.
    pub pending: HashMap<String, SpaceId>,
    /// Spaces whose pending resolutions overlay their ordered approval state.
    pub(crate) optimistic_approvals: HashSet<SpaceId>,
    /// client_id → why the relay refused it.
    pub failed: HashMap<String, String>,
    /// Identities seen in logs whose profile this device doesn't have yet.
    pub unknown: HashSet<IdentityId>,
    pub presence: HashMap<IdentityId, bool>,
    /// Space → member → (delivered up to, read up to). Restored from the device-local
    /// receipt cache before publishing a snapshot; relay replay can only raise it.
    pub receipts: HashMap<SpaceId, HashMap<IdentityId, (u64, u64)>>,
    /// Space → the highest seq this device already said it has (so `Delivered` goes out
    /// once per new head, not once per event).
    pub delivered_sent: HashMap<SpaceId, u64>,
    /// Wakes the network task (set while sync runs).
    pub poke: Option<Box<dyn Fn() + Send>>,
    pub profiles: crate::profile::ProfileNet,
    pub mls: crate::mls::MlsNet,
    pub link: crate::linking::LinkNet,
}

impl NetState {
    pub fn account_author(&self, who: &str) -> Option<Author> {
        self.author.as_ref().filter(|a| a.identity == who).cloned()
    }

    pub(crate) fn wake(&self) {
        if let Some(p) = &self.poke {
            p();
        }
    }
}

/// What happened to an event from the relay.
#[derive(Debug, PartialEq, Eq)]
pub enum Ingest {
    /// Someone else's verified event was retained; projection may await media.
    Applied,
    /// One of ours: transport confirmed the pending copy; authority is rechecked.
    Confirmed,
    /// Already had it.
    Duplicate,
    /// There's a hole before it: sync this Space from `next`.
    Gap { next: u64 },
    /// Bad signature, broken chain…: refused and not stored.
    Invalid(String),
}

/// Local handling of a relay publication refusal; never a wire/UI error code.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RejectionOutcome {
    HandshakeReconciled,
    Retry,
    UserFailed,
    Ignored,
    StorageFailure(String),
}
impl RejectionOutcome {
    pub(crate) fn user_failed(&self) -> bool {
        matches!(self, Self::UserFailed)
    }
}

pub(crate) fn tint_for(id: &str) -> String {
    const TINTS: [&str; 8] = [
        "#4F7CFF", "#FF5C8A", "#19B5A5", "#F59E0B", "#7C5CFF", "#22C55E", "#0EA5E9", "#FF9F0A",
    ];
    let n = id
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    TINTS[n % TINTS.len()].to_string()
}

fn is_message_send(body: &EventBody) -> bool {
    matches!(body, EventBody::MessagePosted { .. })
        || matches!(body, EventBody::ItemCreated {
            kind: ItemKind::File, content: ItemContent::File(doc), ..
        } if doc.is_voice())
}

/// Disposable admission facts from the verified, relay-confirmed log only. An
/// optimistic File or album member (including one already written to a socket)
/// cannot release its dependent message. Build once per relevant Space/offer pass.
struct ConfirmedMediaDependencies {
    state: State,
}

impl ConfirmedMediaDependencies {
    fn from_log(engine: &Engine, log: &SpaceLog) -> R<Self> {
        let mut state = State::default();
        let mut plans = PlanReplay::new(&engine.store, &engine.logs);
        for (end, event) in log.events().iter().enumerate() {
            let context = plans.before(&state, &engine.identities, event, end)?;
            state.apply_checked(event, &engine.identities, &context);
        }
        Ok(Self { state })
    }

    fn allows(&self, event: &Event) -> bool {
        self.state
            .media_message_dependencies_valid(&event.space, &event.author, &event.body)
    }
}

fn same_media_group(first: &Event, second: &Event) -> bool {
    let (
        EventBody::MessagePosted {
            media: Some(first),
            reply: first_reply,
            forwarded: false,
            ..
        },
        EventBody::MessagePosted {
            media: Some(second),
            reply: second_reply,
            forwarded: false,
            ..
        },
    ) = (&first.body, &second.body)
    else {
        return false;
    };
    first.is_valid()
        && second.is_valid()
        && first.presentation == second.presentation
        && first_reply == second_reply
        && first
            .group
            .as_ref()
            .zip(second.group.as_ref())
            .is_some_and(|(first, second)| first.id == second.id && first.count == second.count)
}

impl Engine {
    /// Upload availability is not write authority: signed/canonical admission still
    /// runs normally. This only resolves the File bytes an outbox intent references.
    fn outbox_attachment<'a>(&'a self, event: &'a Event) -> Option<&'a FileDoc> {
        let content = match &event.body {
            EventBody::ItemCreated { content, .. } | EventBody::ItemVersioned { content, .. } => {
                content
            }
            EventBody::MessagePosted {
                attaches: Some(item),
                ..
            } => &self.state.items.get(item)?.current().content,
            _ => return None,
        };
        if let ItemContent::File(file) = content {
            Some(file)
        } else {
            None
        }
    }

    /// Logs from an older event format can't be re-verified under the current one: start over (pre-launch only; ADR 0010).
    pub(crate) fn migrate_event_format(&mut self) -> R<()> {
        let fmt = self.store.meta(EVENT_FORMAT_META)?;
        if fmt.as_deref() != Some(EVENT_FORMAT_VALUE) {
            if self.store.event_count()? > 0 {
                self.store.wipe()?;
                self.store.wipe_sync()?;
            }
            self.store.set_meta(EVENT_FORMAT_META, EVENT_FORMAT_VALUE)?;
        }
        Ok(())
    }

    /// Part of `reload`: account, relay-ordered Spaces and the outbox on top.
    pub(crate) fn reload_sync(&mut self) -> R<()> {
        self.net.account = match self.store.meta("account")? {
            Some(j) => serde_json::from_str(&j).ok(),
            None => None,
        };
        if self.net.account.as_ref().map(|a| a.identity.as_str())
            != self.net.author.as_ref().map(|a| a.identity.as_str())
        {
            self.net.author = None;
        }
        self.net.synced = self.store.synced_spaces()?.into_iter().collect();
        self.net.pending.clear();
        self.net.optimistic_approvals.clear();
        self.net.failed.clear();
        self.net.profiles.forget_shared();

        let confirmed: HashSet<String> = self
            .net
            .synced
            .iter()
            .filter_map(|s| self.logs.get(s))
            .flat_map(|l| l.events().iter().map(|e| e.client_id.clone()))
            .collect();
        let outbox = self.store.outbox()?;
        for p in outbox {
            let e = p.event;
            if confirmed.contains(&e.client_id) {
                // Crashed between storing the confirmed copy and clearing the outbox.
                self.store.outbox_remove(&e.client_id)?;
                continue;
            }
            if p.failed {
                // Only refused messages stay visible (with a retry/delete affordance).
                if is_message_send(&e.body) && self.space_order.contains(&e.space) {
                    self.net
                        .failed
                        .insert(e.client_id.clone(), p.last_error.unwrap_or_default());
                }
                continue;
            }
            if matches!(e.body, EventBody::SpaceCreated { .. })
                && !self.space_order.contains(&e.space)
            {
                self.space_order.push(e.space.clone());
            }
            self.net
                .pending
                .insert(e.client_id.clone(), e.space.clone());
            if matches!(e.body, EventBody::RequestResolved { .. }) {
                self.net.optimistic_approvals.insert(e.space.clone());
            }
        }
        self.reproject_approval_overlay()?;
        self.restore_peer_receipts()?;
        self.recompute_unknown();
        self.reload_mls();
        Ok(())
    }

    fn recompute_unknown(&mut self) {
        let mut unknown = HashSet::new();
        for s in self.state.spaces.values() {
            for (m, _) in &s.members {
                if !self.identities.contains_key(m) {
                    unknown.insert(m.clone());
                }
            }
            for e in &s.entries {
                if !self.identities.contains_key(&e.author) {
                    unknown.insert(e.author.clone());
                }
            }
        }
        self.net.unknown = unknown;
    }

    pub(crate) fn approval_projection_needs_replay(&self, e: &Event) -> bool {
        self.approval_replay_needed
            || self.state.waiting_spaces.contains(&e.space)
            || matches!(
                e.body,
                EventBody::RequestOpened { .. }
                    | EventBody::RequestResolved { .. }
                    | EventBody::MemberAdded { .. }
                    | EventBody::MemberRemoved { .. }
                    | EventBody::GrantIssued { .. }
                    | EventBody::GrantRevoked { .. }
                    | EventBody::ItemCreated { .. }
                    | EventBody::ItemVersioned { .. }
                    | EventBody::ItemReverted { .. }
            )
    }

    pub(crate) fn apply_confirmed_event(&mut self, event: &Event) -> R<Projection> {
        let end = self
            .logs
            .get(&event.space)
            .and_then(|log| log.len().checked_sub(1))
            .ok_or_else(|| CoreError::Invalid {
                reason: "missing confirmed frontier".into(),
            })?;
        let scoped = self
            .space_projections
            .entry(event.space.clone())
            .or_default();
        let context = PlanReplay::new(&self.store, &self.logs).before(
            scoped,
            &self.identities,
            event,
            end,
        )?;
        let outcome = scoped.apply_checked(event, &self.identities, &context);
        self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
        Ok(outcome)
    }

    /// Rebuild authority into temporary state before publishing an optimistic overlay.
    /// Use the verified log cache; do not reload keys, MLS state or open page sessions.
    pub(crate) fn reproject_approval_overlay(&mut self) -> R<()> {
        self.approval_replay_needed = true;
        let mut order = self.store.space_ids()?;
        let pending = self.store.outbox()?;
        // These source events stay in the one existing outbox. References below are
        // display-only recovery facts, never a second durable or writer authority.
        let sources: HashMap<_, _> = pending
            .iter()
            .filter_map(|row| match &row.event.body {
                EventBody::ItemCreated {
                    item,
                    kind: types::ItemKind::File,
                    ..
                } => Some((
                    (
                        row.event.space.as_str(),
                        item.as_str(),
                        row.event.author.as_str(),
                    ),
                    &row.event,
                )),
                _ => None,
            })
            .collect();
        let mut recovery_sources: HashMap<&str, Vec<(&Event, &Event)>> = HashMap::new();
        if let Some(local) = self.me.as_deref() {
            for row in pending.iter().filter(|row| row.failed) {
                if let EventBody::MessagePosted {
                    attaches: Some(item),
                    ..
                } = &row.event.body
                {
                    if let Some(source) =
                        sources.get(&(row.event.space.as_str(), item.as_str(), local))
                    {
                        recovery_sources
                            .entry(row.event.space.as_str())
                            .or_default()
                            .push((&row.event, source));
                    }
                }
            }
        }
        let mut projections = HashMap::new();
        let mut confirmed = HashSet::new();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        for space in &order {
            let mut scoped = State::default();
            if let (Some(local), Some(recovery)) =
                (self.me.as_deref(), recovery_sources.get(space.as_str()))
            {
                for (event, source) in recovery {
                    scoped.reserve_failed_attachment(event, local, source);
                }
            }
            if let Some(log) = self.logs.get(space) {
                for (end, e) in log.events().iter().enumerate() {
                    confirmed.insert((e.space.clone(), e.client_id.clone()));
                    let context = plans.before(&scoped, &self.identities, e, end)?;
                    scoped.apply_checked(e, &self.identities, &context);
                }
            } else if let Some(broken) = self.broken_spaces.get(space) {
                let mut broken = broken.clone();
                broken.entries.clear();
                scoped.spaces.insert(space.clone(), broken);
            }
            projections.insert(space.clone(), scoped);
        }
        let mut optimistic = HashSet::new();
        for p in &pending {
            let e = &p.event;
            if confirmed.contains(&(e.space.clone(), e.client_id.clone())) {
                continue;
            }
            if p.failed {
                if is_message_send(&e.body)
                    && projections
                        .get(&e.space)
                        .is_some_and(|scoped| scoped.spaces.contains_key(&e.space))
                {
                    let scoped = projections.get_mut(&e.space).expect("checked above");
                    let retained = if let EventBody::MessagePosted {
                        attaches: Some(item),
                        ..
                    } = &e.body
                    {
                        self.me.as_deref().is_some_and(|local| {
                            scoped.retain_failed_attachment(
                                e,
                                local,
                                sources
                                    .get(&(e.space.as_str(), item.as_str(), local))
                                    .copied(),
                            )
                        })
                    } else {
                        false
                    };
                    // Preserve normal replay for text and canonical non-File
                    // cards. Recovery reservations still fence the checked fold.
                    if !retained && matches!(e.body, EventBody::MessagePosted { media: None, .. }) {
                        let context = plans.pending(scoped, &self.identities, e)?;
                        scoped.apply_checked(e, &self.identities, &context);
                    } else if !matches!(e.body, EventBody::MessagePosted { .. }) {
                        scoped.retain_failed_voice(e);
                    }
                }
                continue;
            }
            if matches!(e.body, EventBody::SpaceCreated { .. }) && !order.contains(&e.space) {
                order.push(e.space.clone());
            }
            if matches!(e.body, EventBody::RequestResolved { .. }) {
                optimistic.insert(e.space.clone());
            }
            let scoped = projections.entry(e.space.clone()).or_default();
            let context = plans.pending(scoped, &self.identities, e)?;
            scoped.apply_checked(e, &self.identities, &context);
        }
        let state = Self::merge_space_projections(&projections, &order);
        self.state = state;
        self.space_projections = projections;
        self.space_order = order;
        self.net.optimistic_approvals = optimistic;
        self.approval_replay_needed = false;
        self.index_dirty = true;
        // The rebuild starts from an empty projection, so a stored fork has to
        // be written back before the next send.
        self.restore_recorded_forks();
        self.recompute_unknown();
        Ok(())
    }

    pub(crate) fn note_unknown(&mut self, e: &Event) {
        if !self.identities.contains_key(&e.author) {
            self.net.unknown.insert(e.author.clone());
        }
        if let EventBody::MemberAdded { identity, .. } = &e.body {
            if !self.identities.contains_key(identity) {
                self.net.unknown.insert(identity.clone());
            }
        }
    }

    pub(crate) fn delivery(&self, e: &Entry) -> Delivery {
        let source = e.client_id.strip_suffix(":result").unwrap_or(&e.client_id);
        if self.net.failed.contains_key(source) {
            Delivery::Failed
        } else if self.net.pending.contains_key(source) {
            Delivery::Sending
        } else if self.net.synced.contains(&e.space) || self.net.receipts.contains_key(&e.space) {
            self.receipt_for(e)
        } else {
            Delivery::Local
        }
    }

    /// What a new event in `space` says its author had seen: the relay-ordered head, or,
    /// for a Space this device created and the relay hasn't confirmed yet, the genesis link
    /// it will get (the creator's `SpaceCreated` always lands at seq 0, so its chain hash is
    /// known before it's sent).
    pub fn causal_head(&self, space: &str) -> R<Option<Seen>> {
        if let Some(head) = self.logs.get(space).and_then(SpaceLog::head) {
            return Ok(Some(head));
        }
        let genesis = self
            .store
            .outbox()?
            .into_iter()
            .map(|p| p.event)
            .find(|e| e.space == space && matches!(e.body, EventBody::SpaceCreated { .. }));
        Ok(genesis.map(|g| Seen {
            seq: 0,
            hash: chain_hash(space, 0, GENESIS_PREV, &content_hash_of(&g)),
        }))
    }

    /// Writes to a relay-ordered Space: sign, queue, project.
    pub(crate) fn append_synced(
        &mut self,
        space: &str,
        signer: &Author,
        at_ms: i64,
        body: EventBody,
    ) -> R<Event> {
        if self.approval_replay_needed {
            self.reproject_approval_overlay()?;
        }
        let creating = matches!(body, EventBody::SpaceCreated { .. });
        if !creating && !self.state.spaces.contains_key(space) {
            return Err(CoreError::NotFound {
                what: t("Espaço", "Space"),
            });
        }
        let client_id = new_ulid(at_ms);
        let seen = if creating {
            None
        } else {
            self.causal_head(space)?
        };
        let e = signer.sign_event(space, &client_id, at_ms, seen, body);
        let empty = State::default();
        let scoped = self.space_projections.get(space).unwrap_or(&empty);
        let context =
            PlanReplay::new(&self.store, &self.logs).pending(scoped, &self.identities, &e)?;
        if scoped.approval_event_valid(&e, &self.identities, &context) != Projection::Applied {
            return Err(CoreError::Invalid {
                reason: t(
                    "pedido, decisão ou uso não autorizado",
                    "unauthorized request, decision or usage",
                ),
            });
        }
        self.store.outbox_put(&e)?;
        if creating && !self.space_order.contains(&space.to_string()) {
            self.space_order.push(space.to_string());
        }
        self.net.pending.insert(client_id, space.to_string());
        if matches!(e.body, EventBody::RequestResolved { .. }) {
            self.net.optimistic_approvals.insert(space.to_string());
        }
        self.space_projections
            .entry(space.to_owned())
            .or_default()
            .apply_checked(&e, &self.identities, &context);
        if self.approval_replay_needed
            || !self.state.waiting_spaces.is_empty()
            || !self.state.requests.is_empty()
            || !matches!(e.body, EventBody::ItemCreated { .. })
        {
            self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
        } else {
            self.state.apply_checked(&e, &self.identities, &context);
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        self.note_profile_event(&e);
        self.net.wake();
        Ok(e)
    }

    /// An event the relay sequenced.
    pub fn ingest(&mut self, ev: Sequenced) -> Ingest {
        if ev.env.is_sealed() {
            return self.ingest_sealed(ev);
        }
        let Some(e) = ev.plain_event() else {
            return Ingest::Invalid(t("evento ilegível", "unreadable event"));
        };
        let r = self.ingest_plain(e.clone());
        if matches!(r, Ingest::Applied | Ingest::Confirmed) {
            self.mls_membership_changed(&e);
            if let Err(err) = self.compare_checkpoint(&e) {
                return Ingest::Invalid(err.to_string());
            }
            self.mls_device_joining(&e);
        }
        r
    }

    fn ingest_plain(&mut self, e: Event) -> Ingest {
        let space = e.space.clone();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if e.seq < next {
            if self.approval_replay_needed {
                if let Err(error) = self.reproject_approval_overlay() {
                    return Ingest::Invalid(error.to_string());
                }
            }
            return Ingest::Duplicate;
        }
        if e.seq > next {
            return Ingest::Gap { next };
        }
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
        let own = self.net.pending.get(&e.client_id) == Some(&space);
        // The confirmed event, its catch-up scope and an own pending copy describe
        // one receipt. Never publish a confirmation whose bookkeeping failed.
        let stored = (|| -> store::Result<()> {
            let tx = self.store.conn().unchecked_transaction()?;
            self.store.append_event(&e)?;
            self.store.set_synced(&space)?;
            if own {
                self.store.outbox_remove(&e.client_id)?;
            }
            tx.commit()?;
            Ok(())
        })();
        if let Err(err) = stored {
            // Rollback precedes reload, so memory sees the same pending/history state.
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        self.net.synced.insert(space.clone());
        if !self.space_order.contains(&space) {
            self.space_order.push(space.clone());
        }
        let replay_approval = self.approval_projection_needs_replay(&e);
        if own {
            self.net.pending.remove(&e.client_id);
        }
        if replay_approval {
            if own && self.confirmed_item_already_projected(&e) {
                if !self.note_own_confirmed(&space, &e.client_id, e.seq, &e.hash) {
                    self.state =
                        Self::merge_space_projections(&self.space_projections, &self.space_order);
                }
                return Ingest::Confirmed;
            }
            if let Err(err) = self.reproject_approval_overlay() {
                return Ingest::Invalid(err.to_string());
            }
            self.note_profile_event(&e);
            return if own {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
        }
        if own {
            let joining = matches!(e.body, EventBody::MemberAdded { .. })
                && !self.is_member_of(&space, &e.author);
            let had = self.note_own_confirmed(&space, &e.client_id, e.seq, &e.hash);
            if had && !joining {
                return Ingest::Confirmed;
            }
            self.state = Self::merge_space_projections(&self.space_projections, &self.space_order);
            // A join we sent before we had the Space's history: project it now.
            if !self.state.spaces.contains_key(&space) || joining {
                if let Err(err) = self.apply_confirmed_event(&e) {
                    return Ingest::Invalid(err.to_string());
                }
            }
            return Ingest::Confirmed;
        }
        if let Err(err) = self.apply_confirmed_event(&e) {
            return Ingest::Invalid(err.to_string());
        }
        self.index_dirty = true;
        self.note_unknown(&e);
        self.note_profile_event(&e);
        Ingest::Applied
    }

    /// True when this own item is already projected and no approval overlay is in flight.
    pub(crate) fn confirmed_item_already_projected(&self, e: &Event) -> bool {
        if self.approval_replay_needed
            || !self.state.waiting_spaces.is_empty()
            || !self.net.optimistic_approvals.is_empty()
            || !self.state.requests.is_empty()
        {
            return false;
        }
        matches!(
            e.body,
            EventBody::ItemCreated { .. }
                | EventBody::ItemVersioned { .. }
                | EventBody::ItemReverted { .. }
        )
    }

    /// Writes the relay seq onto the optimistic entry. Returns whether the Space is on screen.
    pub(crate) fn note_own_confirmed(
        &mut self,
        space: &str,
        client_id: &str,
        seq: u64,
        hash: &str,
    ) -> bool {
        let stamp = |entries: &mut Vec<Entry>| {
            for entry in entries.iter_mut().filter(|entry| {
                entry.client_id == client_id
                    || entry.client_id.strip_suffix(":result") == Some(client_id)
            }) {
                entry.seq = seq;
                entry.hash = hash.to_string();
            }
        };
        if let Some(space_state) = self
            .space_projections
            .get_mut(space)
            .and_then(|scoped| scoped.spaces.get_mut(space))
        {
            stamp(&mut space_state.entries);
        }
        let Some(space_state) = self.state.spaces.get_mut(space) else {
            return false;
        };
        stamp(&mut space_state.entries);
        true
    }

    fn is_member_of(&self, space: &str, who: &str) -> bool {
        self.state
            .spaces
            .get(space)
            .map(|s| s.members.iter().any(|(m, _)| m == who))
            .unwrap_or(false)
    }

    /// The relay refused one of ours. `sent_epoch` describes the application envelope
    /// actually sent on this connection, not a copy resealed since then.
    pub(crate) fn reject(
        &mut self,
        client_id: &str,
        reason: &str,
        permanent: bool,
        sent_epoch: Option<u64>,
    ) -> RejectionOutcome {
        if permanent {
            match self.mls_handshake_refused(client_id, reason) {
                Ok(true) => return RejectionOutcome::HandshakeReconciled,
                Ok(false) => {}
                Err(error) => return RejectionOutcome::StorageFailure(error.to_string()),
            }
        }
        let rejected = match self.store.outbox_get(client_id) {
            Ok(None) => return RejectionOutcome::Ignored,
            Err(error) => return RejectionOutcome::StorageFailure(error.to_string()),
            Ok(Some(event)) => event,
        };
        // Written in the clear just before the Space went end-to-end: it waits for the
        // group and goes out sealed, it doesn't fail.
        // Sealed before this device caught up with a commit: it seals again once it has.
        if let (protocol::STALE_SEAL, Some(epoch)) = (reason, sent_epoch) {
            self.mls_sealed_stale(client_id, epoch);
        }
        let permanent =
            permanent && reason != protocol::SEAL_REQUIRED && reason != protocol::STALE_SEAL;
        let rejected_creation = if permanent {
            self.store.outbox_get(client_id).ok().flatten().filter(|e| {
                matches!(e.body, EventBody::SpaceCreated { .. }) && self.next_seq(&e.space) == 0
            })
        } else {
            None
        };
        let noted = if let Some(creation) = rejected_creation {
            self.store
                .outbox_refuse_space(&creation.space, reason)
                .map_err(CoreError::from)
        } else if permanent {
            self.refuse_outbox_media_dependents(&rejected, reason)
        } else {
            self.store
                .outbox_note(client_id, reason, permanent)
                .map_err(CoreError::from)
        };
        if let Err(error) = noted {
            return RejectionOutcome::StorageFailure(error.to_string());
        }
        if !permanent {
            return RejectionOutcome::Retry;
        }
        let _ = self.store.meta_delete(&format!("invite:{client_id}"));
        // Re-project without the refused event (messages stay visible as failed).
        let _ = self.reload();
        RejectionOutcome::UserFailed
    }

    /// Preserve terminal signed intents as readable failures, atomically. A
    /// refused File fails its exact referencing media messages; a refused group
    /// member also fails its matching group, so newer Space heads can progress.
    fn refuse_outbox_media_dependents(&self, rejected: &Event, reason: &str) -> R<()> {
        let candidate = matches!(
            &rejected.body,
            EventBody::MessagePosted { media: Some(_), .. }
        ) || matches!(&rejected.body, EventBody::ItemCreated { kind: ItemKind::File, content: ItemContent::File(file), .. } if !file.is_voice());
        if !candidate || event_log::verify_author(rejected).is_err() {
            return self
                .store
                .outbox_note(&rejected.client_id, reason, true)
                .map_err(Into::into);
        }
        let rows = self.store.outbox()?;
        let mut seeds = Vec::new();
        if matches!(&rejected.body, EventBody::MessagePosted { media: Some(media), .. } if media.is_valid())
        {
            seeds.push(rejected);
        }
        if let EventBody::ItemCreated {
            item,
            kind: ItemKind::File,
            content: ItemContent::File(file),
            ..
        } = &rejected.body
        {
            if !file.is_voice() {
                // A refused duplicate creation must not invalidate references to
                // an already authoritative Item. Never consult optimistic state.
                let confirmed = self
                    .logs
                    .get(&rejected.space)
                    .map(|log| ConfirmedMediaDependencies::from_log(self, log))
                    .transpose()?;
                if confirmed
                    .as_ref()
                    .is_none_or(|confirmed| !confirmed.state.items.contains_key(item))
                {
                    seeds.extend(
                        rows.iter()
                            .filter(|row| !row.failed)
                            .map(|row| &row.event)
                            .filter(|event| {
                                event.space == rejected.space
                                    && event.author == rejected.author
                                    && event.device == rejected.device
                                    && event_log::verify_author(event).is_ok()
                                    && matches!(&event.body, EventBody::MessagePosted {
                                attaches: Some(attaches), media, ..
                            } if attaches == item && media.as_ref().is_none_or(|media| media.is_valid()))
                            }),
                    );
                }
            }
        }
        if seeds.is_empty() {
            return self
                .store
                .outbox_note(&rejected.client_id, reason, true)
                .map_err(Into::into);
        }
        let is_member = |event: &Event| {
            event.space == rejected.space
                && event.author == rejected.author
                && event.device == rejected.device
                && event_log::verify_author(event).is_ok()
                && seeds
                    .iter()
                    .any(|seed| seed.client_id == event.client_id || same_media_group(seed, event))
        };
        // An aborted album must not leave a fresh, otherwise unused File head
        // blocking later ordinary messages while that orphan's upload retries.
        // Shared references and already canonical Items retain their own fate.
        let orphan_files: HashSet<_> = rows.iter().filter(|row| is_member(&row.event))
            .filter_map(|row| match &row.event.body {
                EventBody::MessagePosted { attaches: Some(item), .. } => Some(item),
                _ => None,
            }).filter(|item| !rows.iter().any(|row| !row.failed && !is_member(&row.event)
                && matches!(&row.event.body, EventBody::MessagePosted { attaches: Some(other), .. } if other == *item)))
            .collect();
        let confirmed = self
            .logs
            .get(&rejected.space)
            .map(|log| ConfirmedMediaDependencies::from_log(self, log))
            .transpose()?;
        let tx = self
            .store
            .conn()
            .is_autocommit()
            .then(|| self.store.conn().unchecked_transaction())
            .transpose()
            .map_err(store::StoreError::from)?;
        for row in &rows {
            let same_owner = row.event.space == rejected.space
                && row.event.author == rejected.author
                && row.event.device == rejected.device
                && event_log::verify_author(&row.event).is_ok();
            let orphan = same_owner
                && matches!(&row.event.body,
                EventBody::ItemCreated { item, kind: ItemKind::File, content: ItemContent::File(file), .. }
                    if orphan_files.contains(item) && confirmed.as_ref().is_none_or(|state| !state.state.items.contains_key(item))
                    && file.chunks.iter().chain(file.thumb.iter()).any(|chunk| self.store.upload_pending(&chunk.blob).unwrap_or(true)));
            let dependent = is_member(&row.event) || orphan;
            if row.event.client_id == rejected.client_id || (!row.failed && dependent) {
                self.store.outbox_note(&row.event.client_id, reason, true)?;
            }
        }
        if let Some(tx) = tx {
            tx.commit().map_err(store::StoreError::from)?;
        }
        Ok(())
    }

    /// A terminal HTTP refusal and the corresponding failed signed intents are
    /// one durable transaction. No relay confirmation is inferred from local bytes.
    pub fn upload_refused(&mut self, blob: &str) -> R<Vec<String>> {
        let tx = self
            .store
            .conn()
            .unchecked_transaction()
            .map_err(store::StoreError::from)?;
        if !self.store.upload_refused(blob)? {
            return Ok(Vec::new());
        }
        let mut changed = HashSet::new();
        for row in self.store.outbox()? {
            let file = self.outbox_attachment(&row.event);
            if !row.failed
                && event_log::verify_author(&row.event).is_ok()
                && file.is_some_and(|file| {
                    file.chunks
                        .iter()
                        .chain(file.thumb.iter())
                        .any(|chunk| chunk.blob == blob)
                })
            {
                self.refuse_outbox_media_dependents(&row.event, "attachment upload refused")?;
                changed.insert(row.event.space.clone());
            }
        }
        tx.commit().map_err(store::StoreError::from)?;
        self.reload()?;
        self.net.wake();
        Ok(changed.into_iter().collect())
    }

    pub fn cursors(&self) -> Vec<Cursor> {
        self.net
            .synced
            .iter()
            .map(|s| Cursor {
                space: s.clone(),
                next_seq: self.logs.get(s).map(|l| l.next_seq()).unwrap_or(0),
            })
            .collect()
    }

    pub fn next_seq(&self, space: &str) -> u64 {
        self.logs.get(space).map(|l| l.next_seq()).unwrap_or(0)
    }

    /// OpenMLS `SenderRatchetConfiguration` default `out_of_order_tolerance`.
    /// A generation this far behind one already opened still decrypts; one step
    /// further is `TooDistantInThePast` and that page never opens on the device.
    const APPLICATION_IN_FLIGHT: usize = 5;

    /// Envelopes still waiting for the relay, oldest first.
    pub fn outbox_envelopes(&self) -> Vec<Envelope> {
        self.outbox_envelopes_except(&HashMap::new())
    }

    /// The outbox as envelopes, leaving out the ones already sent on this connection.
    /// Skips sent entries before sealing work and asks each Space's group for its epoch
    /// once. Media admission separately folds confirmed history once per relevant Space
    /// and offer pass; that replay cost is not a constant-time publication guarantee.
    ///
    /// At most [`APPLICATION_IN_FLIGHT`] application messages per Space are unpublished
    /// or still awaiting accept. Once a Space is at that cap, every later head of that
    /// same Space stays queued, in order, until a confirmation frees a slot. Heads of
    /// other Spaces keep publishing, each under the same cap. OpenMLS drops a generation
    /// more than that far behind the one it has already opened (`TooDistantInThePast`),
    /// and a rate-limit reject retries the older ciphertext after a later one has been
    /// sequenced.
    pub fn outbox_envelopes_except(&self, sent: &HashMap<String, Option<u64>>) -> Vec<Envelope> {
        let mut ready = HashMap::new();
        let mut media_dependencies = HashMap::new();
        // A Welcome goes out only once its commit is in: sent together, a commit held back
        // by the relay (rate limit) would let the Welcome arrive first and be refused.
        let held = self.welcomes_behind_commits();
        // Ahead of any in-flight application cap: a forked Space publishes nothing.
        let mut forked = HashMap::new();
        let heads = self.store.outbox_heads().unwrap_or_default();
        let mut in_flight: HashMap<String, usize> = HashMap::new();
        for head in &heads {
            if sent
                .get(&head.client_id)
                .is_some_and(|epoch| epoch.is_some())
            {
                *in_flight.entry(head.space.clone()).or_default() += 1;
            }
        }
        let mut blocked: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for p in heads {
            if blocked.contains(&p.space) {
                continue;
            }
            if p.failed || sent.contains_key(&p.client_id) || held.contains(&p.client_id) {
                continue;
            }
            if *forked
                .entry(p.space.clone())
                .or_insert_with(|| self.fork_recorded(&p.space))
            {
                continue;
            }
            // Refused for being clear in a Space that went end-to-end: it waits until this
            // device has caught up with that, then goes out sealed.
            if p.last_error.as_deref() == Some(protocol::SEAL_REQUIRED) && !self.is_e2e(&p.space) {
                continue;
            }
            let Some(event) = self.store.outbox_get(&p.client_id).ok().flatten() else {
                continue;
            };
            // An offline creation has a predicted genesis link. Its descendants
            // must wait for confirmation: a retryable creation failure can otherwise
            // let them reach the relay before the Space exists.
            if event
                .seen
                .as_ref()
                .is_some_and(|seen| self.next_seq(&event.space) <= seen.seq)
            {
                continue;
            }
            // File publication must not outrun its actual HTTP upload. A terminal
            // refusal retains this row and fails the exact dependent outbox intents;
            // transient retries keep the original signature/CID/ciphertext queued.
            let file = self.outbox_attachment(&event);
            if file.is_some_and(|file| {
                file.chunks
                    .iter()
                    .chain(file.thumb.iter())
                    .any(|chunk| self.store.upload_pending(&chunk.blob).unwrap_or(true))
            }) {
                blocked.insert(event.space.clone());
                continue;
            }
            if matches!(&event.body, EventBody::MessagePosted { media: Some(_), .. }) {
                let Some(log) = self.logs.get(&event.space) else {
                    blocked.insert(event.space.clone());
                    continue;
                };
                let confirmed = media_dependencies
                    .entry(event.space.clone())
                    .or_insert_with(|| ConfirmedMediaDependencies::from_log(self, log).ok());
                if !confirmed
                    .as_ref()
                    .is_some_and(|confirmed| confirmed.allows(&event))
                {
                    // Keep the same Space's sealed generations behind this head;
                    // skipping it could outrun OpenMLS's existing reorder window.
                    blocked.insert(event.space.clone());
                    continue;
                }
            }
            let Some(mut env) = self.outgoing_envelope_with(&event, &mut ready) else {
                continue;
            };
            if let Some(bytes) = env.recovery() {
                let Some(reference) = event_log::recovery::RecoveryRef::parse(bytes) else {
                    continue;
                };
                if !self
                    .store
                    .recovery_upload_confirmed(&reference.blob)
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if env.sealed_kind() == Some(SealedKind::Application) {
                let n = in_flight.get(env.space()).copied().unwrap_or(0);
                if n >= Self::APPLICATION_IN_FLIGHT {
                    blocked.insert(env.space().to_string());
                    continue;
                }
                *in_flight.entry(env.space().to_string()).or_default() += 1;
            }
            if !env.is_sealed() {
                env.invite = self
                    .store
                    .meta(&format!("invite:{}", p.client_id))
                    .ok()
                    .flatten();
            }
            out.push(env);
        }
        out
    }

    /// Our queued Welcomes that wait behind a commit of ours still in the outbox.
    fn welcomes_behind_commits(&self) -> HashSet<String> {
        let mut committing: HashSet<String> = HashSet::new();
        let mut held = HashSet::new();
        for (client_id, space, kind) in self.store.outbox_handshakes().unwrap_or_default() {
            if kind == SealedKind::Commit.name() {
                committing.insert(space);
            } else if kind == SealedKind::Welcome.name() && committing.contains(&space) {
                held.insert(client_id);
            }
        }
        held
    }

    pub fn outbox_len(&self) -> u64 {
        self.store.outbox_len().unwrap_or(0)
    }

    pub fn take_unknown(&mut self) -> Vec<IdentityId> {
        self.net.unknown.iter().take(500).cloned().collect()
    }

    /// The relay answered a profile lookup: whoever it didn't know stays unnamed
    /// (until the next launch) instead of being asked for again and again.
    pub fn profiles_answered(&mut self, asked: &[IdentityId]) {
        for id in asked {
            self.net.unknown.remove(id);
        }
    }

    /// Directory entries from the relay: handles and public keys. A person's name and bio
    /// come from their encrypted profile when this device holds the key, else the @handle.
    pub fn put_profiles(&mut self, profiles: Vec<Identity>) -> R<()> {
        let mut reproject_agents = false;
        for mut p in profiles {
            if p.id.len() != 64 || !event_log::agent_owner::profile_authorized(&p) {
                continue;
            }
            // Never let the directory rename someone this device holds keys for.
            if self.signers.contains_key(&p.id) || self.me.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            if self.identities.get(&p.id).is_some_and(|known| {
                // Unsigned legacy directory metadata cannot establish an owner pin.
                known.owner_proof.is_some()
                    && event_log::agent_owner::profile_authorized(known)
                    && (known.kind != p.kind
                        || known.owner != p.owner
                        || known.owner_proof != p.owner_proof)
            }) {
                continue;
            }
            reproject_agents |= p.kind == IdentityKind::Agent
                && self
                    .identities
                    .get(&p.id)
                    .is_none_or(|old| old.kind != p.kind || old.owner != p.owner);
            self.overlay_profile(&mut p);
            self.store.put_identity(&p, None)?;
            if !self.identities.contains_key(&p.id) {
                self.identity_order.push(p.id.clone());
            }
            self.net.unknown.remove(&p.id);
            self.identities.insert(p.id.clone(), p);
            self.index_dirty = true;
        }
        if reproject_agents {
            self.reload()?;
        }
        Ok(())
    }

    // ── account ──

    pub fn account(&self) -> Option<&AccountMeta> {
        self.net.account.as_ref()
    }

    pub fn is_unlocked(&self) -> bool {
        self.net.author.is_some()
    }

    /// Authorizes an agent root from this account's unlocked, certified device.
    /// The relay must still check enrollment at the agent's first registration.
    pub fn authorize_agent(&self, agent: &str) -> R<types::AgentOwnerProof> {
        self.net
            .author
            .as_ref()
            .filter(|author| {
                self.account()
                    .is_some_and(|a| a.identity == author.identity)
            })
            .and_then(|author| event_log::agent_owner::authorize(author, agent))
            .ok_or_else(|| CoreError::Forbidden {
                reason: "agent authorization needs an unlocked owner device and a valid agent key"
                    .into(),
            })
    }

    pub(crate) fn save_linked_account(&mut self, a: AccountMeta) -> R<()> {
        self.save_account(a)
    }

    fn save_account(&mut self, a: AccountMeta) -> R<()> {
        self.store.set_meta(
            "account",
            &serde_json::to_string(&a).map_err(|e| CoreError::Storage {
                message: e.to_string(),
            })?,
        )?;
        self.net.account = Some(a);
        Ok(())
    }

    /// Creates this device's account. Returns the identity, device and agreement secrets for
    /// the vault; nothing secret is written to SQLite.
    pub fn create_account(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        self.create_account_as(name, handle, relay_url, Signer::generate(), None, false)
    }

    /// Only the protected staging identity may repair its unfinished local install.
    /// An existing real account, a different vault, or unrelated local data is never
    /// replaced by recovery of a signup journal.
    pub(crate) fn validate_provider_setup(
        &self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        let root = Signer::from_secret(&setup.root);
        let device = Signer::from_secret(&setup.device);
        let identity = root.id();
        let certificate = root.sign(&event_log::device_cert_message(&device.id()));
        let forbidden = || CoreError::Invalid {
            reason:
                "A preparação não pertence a esta conta. Seus dados e chaves foram preservados."
                    .into(),
        };
        if self.store.meta("account")?.is_some() && self.account().is_none() {
            return Err(forbidden());
        }
        if let Some(account) = self.account() {
            if account.registered
                || account.identity != identity
                || account.device != device.id()
                || account.cert != certificate
                || account.relay_url != setup.relay
                || self.store.meta("provider_signup_pending")?.as_deref() != Some("1")
            {
                return Err(forbidden());
            }
        }
        if self.me.as_deref().is_some_and(|me| me != identity)
            || self.identities.values().any(|person| {
                if person.id == identity {
                    person.kind != IdentityKind::Person || person.handle != setup.handle
                } else {
                    person.kind != IdentityKind::Agent
                        || person.owner.as_deref() != Some(identity.as_str())
                        || !event_log::agent_owner::profile_authorized(person)
                }
            })
            || self.logs.values().any(|log| {
                log.events().iter().any(|event| {
                    event.author != identity
                        || event.device.as_deref() != Some(device.id().as_str())
                        || event.cert.as_deref() != Some(certificate.as_str())
                })
            })
            || !self.broken_spaces.is_empty()
            || !self.store.synced_spaces()?.is_empty()
            || self
                .store
                .profile_keys()?
                .iter()
                .any(|row| row.identity != identity)
        {
            return Err(forbidden());
        }
        Ok(())
    }

    pub(crate) fn validate_installed_provider_setup(
        &self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        self.validate_provider_setup(setup)?;
        let identity = Signer::from_secret(&setup.root).id();
        let profile = self.store.profile_key(&identity)?;
        if self.account().is_none()
            || !self.is_unlocked()
            || self.me.as_deref() != Some(identity.as_str())
            || !self.identities.contains_key(&identity)
            || profile.as_ref().is_none_or(|row| {
                event_log::profile::ProfileKey::from_hex(&row.key).is_none()
                    || row
                        .fields
                        .as_deref()
                        .and_then(event_log::profile::ProfileFields::from_bytes)
                        .is_none()
            })
        {
            return Err(CoreError::Invalid {
                reason:
                    "A preparação desta conta ainda não terminou. Suas chaves foram preservadas."
                        .into(),
            });
        }
        Ok(())
    }

    /// The journal already owns all three fixed secrets before this transaction.
    /// A crash before RELEASE rolls SQLite back; a crash after it replays the same
    /// account, private profile key and personal spaces without creating duplicates.
    pub(crate) fn install_provider_setup(
        &mut self,
        setup: &crate::provider_auth::ProviderSetup,
    ) -> R<()> {
        self.validate_provider_setup(setup)?;
        self.install_prepared_account(
            PreparedAccount {
                root: &setup.root,
                device: &setup.device,
                agreement: &setup.agreement,
                name: &setup.name,
                handle: &setup.handle,
                relay: &setup.relay,
                agent: None,
            },
            true,
            |engine| engine.validate_installed_provider_setup(setup),
        )
    }

    /// The only initial account writer. Callers validate their own setup scope;
    /// durable setup paths retain all supplied keys before entering this savepoint.
    /// This method never wipes an account or generates a replacement device.
    pub(crate) fn install_prepared_account(
        &mut self,
        setup: PreparedAccount<'_>,
        provider_signup: bool,
        validate: impl FnOnce(&mut Self) -> R<()>,
    ) -> R<()> {
        let root = Signer::from_secret(setup.root);
        let device = Signer::from_secret(setup.device);
        let identity = root.id();
        let author = Author::device(&root, device.clone());
        let storage_error = |error: rusqlite::Error| CoreError::Storage {
            message: error.to_string(),
        };
        self.store
            .conn()
            .execute_batch("SAVEPOINT prepared_account")
            .map_err(storage_error)?;
        let result = (|| {
            if provider_signup {
                self.store.set_meta("provider_signup_pending", "1")?;
            }
            if !self.identities.contains_key(&identity) {
                let (owner, owner_proof) = setup.agent.map_or((None, None), |(owner, proof)| {
                    (Some(owner.to_string()), Some(Box::new(proof.clone())))
                });
                let me = Identity {
                    id: identity.clone(),
                    kind: if setup.agent.is_some() {
                        IdentityKind::Agent
                    } else {
                        IdentityKind::Person
                    },
                    name: setup.name.into(),
                    handle: setup.handle.into(),
                    tint_hex: tint_for(&identity),
                    glyph: None,
                    owner,
                    bio: String::new(),
                    owner_proof,
                };
                self.store.put_identity(&me, None)?;
                self.identity_order.push(identity.clone());
                self.identities.insert(identity.clone(), me);
            }
            self.store.set_meta("me", &identity)?;
            self.me = Some(identity.clone());
            self.save_account(AccountMeta {
                identity: identity.clone(),
                device: device.id(),
                cert: author.cert.clone().unwrap_or_default(),
                relay_url: setup.relay.into(),
                registered: false,
            })?;
            self.net.author = Some(author);
            let agreement = event_log::profile::AgreementKey::from_secret(*setup.agreement);
            if self.store.profile_key(&identity)?.is_none() {
                self.init_profile_with_agreement(&identity, setup.name, agreement)?;
            } else {
                self.net.profiles.agreement = Some(agreement);
            }
            if setup.agent.is_none() {
                self.seed_personal_agent(&identity)?;
            }
            validate(self)?;
            self.store
                .conn()
                .execute_batch("RELEASE prepared_account")
                .map_err(storage_error)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self
                .store
                .conn()
                .execute_batch("ROLLBACK TO prepared_account; RELEASE prepared_account");
            self.net = Default::default();
            self.reload()?;
        }
        result
    }

    pub(crate) fn provider_setup_pending(&self) -> bool {
        // A storage failure must not turn unfinished signup into an active account.
        !matches!(self.store.meta("provider_signup_pending"), Ok(None))
    }

    /// An Agent citizen's own account: its root key, a fresh certified device and the
    /// owner device's proof (ADR 0049). Same registration, MLS and sync as a person.
    pub fn create_agent_account(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
        root_secret: [u8; 32],
        owner: &str,
        proof: types::AgentOwnerProof,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        let root = Signer::from_secret(&root_secret);
        if !event_log::agent_owner::verify(owner, &root.id(), &proof) {
            return Err(CoreError::Forbidden {
                reason: "invalid agent owner proof".into(),
            });
        }
        self.create_account_as(
            name,
            handle,
            relay_url,
            root,
            Some((owner.to_string(), proof)),
            false,
        )
    }

    fn create_account_as(
        &mut self,
        name: &str,
        handle: &str,
        relay_url: &str,
        root: Signer,
        agent: Option<(String, types::AgentOwnerProof)>,
        provider_signup: bool,
    ) -> R<([u8; 32], [u8; 32], [u8; 32])> {
        if self.net.account.is_some() {
            return Err(CoreError::Invalid {
                reason: t(
                    "Este aparelho já tem uma conta.",
                    "This device already has an account.",
                ),
            });
        }
        let handle = normalize_handle(handle).ok_or_else(|| CoreError::Invalid {
            reason: t(
                "O @ precisa ter de 3 a 24 letras, números, '.' ou '_', começando com letra.",
                "Your @ needs 3–24 letters, digits, '.' or '_', starting with a letter.",
            ),
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("Diga seu nome.", "Tell us your name."),
            });
        }
        // A real account replaces the demo story.
        if !self.is_empty() {
            self.wipe()?;
        }
        let root_secret = root.secret();
        let device = Signer::generate().secret();
        let agreement = event_log::profile::AgreementKey::generate().secret();
        self.install_prepared_account(
            PreparedAccount {
                root: &root_secret,
                device: &device,
                agreement: &agreement,
                name,
                handle: &handle,
                relay: relay_url.trim_end_matches('/'),
                agent: agent.as_ref().map(|(owner, proof)| (owner.as_str(), proof)),
            },
            provider_signup,
            |_| Ok(()),
        )?;
        Ok((root_secret, device, agreement))
    }

    /// After a backup's tables land (ADR 0046): this device becomes a new device of the
    /// restored identity, with its own key and certificate. The relay already knows the
    /// identity, so it isn't registered again.
    pub(crate) fn install_restored_account(
        &mut self,
        root: &Signer,
        device: Signer,
        relay_url: &str,
    ) -> R<()> {
        let author = Author::device(root, device.clone());
        self.store.set_meta("me", &root.id())?;
        self.save_account(AccountMeta {
            identity: root.id(),
            device: device.id(),
            cert: author.cert.clone().unwrap_or_default(),
            relay_url: relay_url.trim_end_matches('/').to_string(),
            registered: true,
        })?;
        self.net.author = Some(author);
        self.reload()?;
        Ok(())
    }

    /// Your on-device Zoen (local agent, local DM) until the agent runtime takes over.
    fn seed_personal_agent(&mut self, me: &str) -> R<()> {
        let at = crate::engine::now_ms();
        self.ensure_personal_space()?;
        let existing = self
            .identities
            .values()
            .find(|person| {
                person.kind == IdentityKind::Agent
                    && person.owner.as_deref() == Some(me)
                    && person.handle == "zoen"
            })
            .map(|person| person.id.clone());
        let zoen = match existing {
            Some(id) => id,
            None => self.create_identity(
                Identity {
                    owner_proof: None,
                    id: String::new(),
                    kind: IdentityKind::Agent,
                    name: "Zoen".into(),
                    handle: "zoen".into(),
                    tint_hex: "#7C5CFF".into(),
                    glyph: Some("sparkles".into()),
                    owner: Some(me.to_string()),
                    bio: t(
                        "Seu agente. Transforma conversa em coisa.",
                        "Your agent. Turns conversation into things.",
                    ),
                },
                true,
            )?,
        };
        if self.state.spaces.values().any(|space| {
            space.kind == SpaceKind::Direct
                && space.members.iter().any(|(id, _)| id == me)
                && space.members.iter().any(|(id, _)| id == &zoen)
        }) {
            return Ok(());
        }
        let partial = self
            .state
            .spaces
            .values()
            .find(|space| {
                space.kind == SpaceKind::Direct
                    && space.privacy == Privacy::EndToEnd
                    && space.title == "Zoen"
                    && space.members.len() == 1
                    && space
                        .members
                        .iter()
                        .any(|(id, role)| id == me && *role == Role::Owner)
            })
            .map(|space| space.id.clone());
        let dm = match partial {
            Some(id) => id,
            None => {
                let id = new_id("sp");
                self.append_at(
                    &id,
                    me,
                    at,
                    EventBody::SpaceCreated {
                        title: "Zoen".into(),
                        kind: SpaceKind::Direct,
                        privacy: Privacy::EndToEnd,
                    },
                )?;
                id
            }
        };
        self.append_at(
            &dm,
            me,
            at,
            EventBody::MemberAdded {
                identity: zoen,
                role: Role::Member,
            },
        )?;
        Ok(())
    }

    /// Loads the device key from the vault. `false` = no account (or key missing).
    pub fn unlock(&mut self, device_secret: Option<Vec<u8>>) -> R<bool> {
        let Some(acct) = self.net.account.clone() else {
            return Ok(false);
        };
        let Some(secret) = device_secret.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        else {
            return Ok(false);
        };
        let key = Signer::from_secret(&secret);
        if key.id() != acct.device {
            return Err(CoreError::Invalid {
                reason: t(
                    "A chave deste aparelho não confere com a conta.",
                    "This device's key doesn't match the account.",
                ),
            });
        }
        self.net.author = Some(Author::certified(
            acct.identity.clone(),
            key,
            acct.cert.clone(),
        ));
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        Ok(true)
    }

    pub fn set_registered(&mut self, registered: bool) -> R<()> {
        if let Some(mut a) = self.net.account.clone() {
            if a.registered != registered {
                a.registered = registered;
                self.save_account(a)?;
            }
        }
        Ok(())
    }

    pub fn my_profile(&self) -> Option<Identity> {
        let id = self.net.account.as_ref()?.identity.clone();
        self.identities.get(&id).cloned()
    }

    pub fn update_profile(&mut self, name: &str, handle: &str, bio: &str) -> R<()> {
        let mut me = self.my_profile().ok_or_else(|| CoreError::NotFound {
            what: "account".into(),
        })?;
        me.handle = normalize_handle(handle).ok_or_else(|| CoreError::Invalid {
            reason: t("@ inválido.", "Invalid @."),
        })?;
        if !name.trim().is_empty() {
            me.name = name.trim().to_string();
        }
        me.bio = bio.chars().take(280).collect();
        self.store.put_identity(&me, None)?;
        let (name, bio) = (me.name.clone(), me.bio.clone());
        self.identities.insert(me.id.clone(), me);
        self.set_profile_text(&name, &bio)?;
        self.set_registered(false)?;
        self.net.wake();
        Ok(())
    }

    /// (identity, device key, cert, profile, registered) for the network task.
    pub fn net_credentials(&self) -> Option<(IdentityId, Signer, String, Identity, bool, String)> {
        let a = self.net.account.as_ref()?;
        let author = self.net.author.as_ref()?;
        Some((
            a.identity.clone(),
            author.key.clone(),
            a.cert.clone(),
            self.my_profile()?,
            a.registered,
            a.relay_url.clone(),
        ))
    }

    // ── Spaces on the relay ──

    pub fn create_synced_space(
        &mut self,
        title: &str,
        kind: SpaceKind,
        privacy: Privacy,
        members: &[IdentityId],
    ) -> R<SpaceId> {
        let me = self.me_id()?;
        if self.net.author.is_none() {
            return Err(CoreError::Forbidden {
                reason: t("Entre na sua conta primeiro.", "Sign in first."),
            });
        }
        let space = new_id("sp");
        self.store.set_synced(&space)?;
        self.net.synced.insert(space.clone());
        self.append(
            &space,
            &me,
            EventBody::SpaceCreated {
                title: title.to_string(),
                kind,
                privacy,
            },
        )?;
        if privacy == Privacy::EndToEnd {
            self.create_mls_group(&space)?;
        }
        for m in members.iter().filter(|m| **m != me) {
            self.append(
                &space,
                &me,
                EventBody::MemberAdded {
                    identity: m.clone(),
                    role: Role::Member,
                },
            )?;
        }
        Ok(space)
    }

    /// The existing DM with `who` on the relay, if any.
    pub fn direct_with(&self, who: &str) -> Option<SpaceId> {
        let me = self.me.clone()?;
        self.space_order
            .iter()
            .filter(|s| self.net.synced.contains(*s))
            .find(|s| {
                self.state.spaces.get(*s).is_some_and(|st| {
                    st.kind == SpaceKind::Direct
                        && st.members.len() == 2
                        && st.members.iter().any(|(m, _)| *m == me)
                        && st.members.iter().any(|(m, _)| m == who)
                })
            })
            .cloned()
    }

    pub fn start_direct(&mut self, who: &str) -> R<SpaceId> {
        if let Some(s) = self.direct_with(who) {
            return Ok(s);
        }
        if !self.identities.contains_key(who) {
            return Err(CoreError::NotFound {
                what: t("pessoa", "person"),
            });
        }
        self.create_synced_space("", SpaceKind::Direct, Privacy::EndToEnd, &[who.to_string()])
    }

    /// Makes a relay-readable chat or group end-to-end from here on (ADR 0027). Earlier
    /// messages stay as they were; the relay reads nothing written after this.
    pub fn encrypt_space(&mut self, space: &str) -> R<()> {
        let me = self.me_id()?;
        let s = self
            .state
            .spaces
            .get(space)
            .ok_or_else(|| CoreError::NotFound {
                what: t("conversa", "chat"),
            })?;
        if s.privacy == Privacy::EndToEnd {
            return Ok(());
        }
        let refuse = |pt: &str, en: &str| Err(CoreError::Invalid { reason: t(pt, en) });
        if !self.net.synced.contains(space) {
            return refuse(
                "Esta conversa só existe neste aparelho.",
                "This chat lives only on this device.",
            );
        }
        if s.privacy == Privacy::Public || !matches!(s.kind, SpaceKind::Direct | SpaceKind::Group) {
            return refuse(
                "Espaços públicos continuam legíveis.",
                "Public spaces stay readable.",
            );
        }
        let role = s.members.iter().find(|(m, _)| *m == me).map(|(_, r)| *r);
        let allowed = matches!(role, Some(Role::Owner | Role::Admin))
            || (role == Some(Role::Member) && s.kind == SpaceKind::Direct);
        if !allowed {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Só quem administra o grupo ativa a criptografia.",
                    "Only the group's owners and admins can turn on encryption.",
                ),
            });
        }
        self.append(space, &me, EventBody::SpaceEncrypted)?;
        Ok(())
    }

    pub fn add_member(&mut self, space: &str, who: &str, role: Role) -> R<()> {
        let me = self.me_id()?;
        self.append(
            space,
            &me,
            EventBody::MemberAdded {
                identity: who.to_string(),
                role,
            },
        )?;
        Ok(())
    }

    /// Takes someone out of a chat. In an end-to-end chat they read nothing written after
    /// the commit that removes them, and nobody seals a message before it lands.
    pub fn remove_member(&mut self, space: &str, who: &str) -> R<()> {
        let me = self.me_id()?;
        let s = self
            .state
            .spaces
            .get(space)
            .ok_or_else(|| CoreError::NotFound {
                what: t("conversa", "chat"),
            })?;
        let role_of = |id: &str| s.members.iter().find(|(m, _)| m == id).map(|(_, r)| *r);
        let allowed = match (role_of(&me), role_of(who)) {
            (_, None) => {
                return Err(CoreError::NotFound {
                    what: t("membro", "member"),
                });
            }
            (Some(Role::Owner), _) => true,
            (Some(Role::Admin), Some(target)) => target != Role::Owner,
            _ => false,
        };
        if !allowed {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Só quem administra a conversa remove pessoas.",
                    "Only the chat's owners and admins remove people.",
                ),
            });
        }
        self.append(
            space,
            &me,
            EventBody::MemberRemoved {
                identity: who.to_string(),
            },
        )?;
        Ok(())
    }

    pub fn leave_space(&mut self, space: &str) -> R<()> {
        let me = self.me_id()?;
        self.append(
            space,
            &me,
            EventBody::MemberRemoved {
                identity: me.clone(),
            },
        )?;
        Ok(())
    }

    /// Queues "I'm joining" for a Space we don't have yet; the relay checks the code.
    pub fn queue_join(&mut self, space: &str, role: Role, code: &str) -> R<()> {
        let me = self.me_id()?;
        let author = self.author_for(&me)?;
        let at = crate::engine::now_ms();
        let client_id = new_ulid(at);
        let e = author.sign_event(
            space,
            &client_id,
            at,
            None,
            EventBody::MemberAdded { identity: me, role },
        );
        self.store.set_meta(&format!("invite:{client_id}"), code)?;
        self.store.outbox_put(&e)?;
        self.store.set_synced(space)?;
        self.net.synced.insert(space.to_string());
        self.net.pending.insert(client_id, space.to_string());
        self.net.wake();
        Ok(())
    }

    /// ✓, ✓✓ or read for one of my messages: the lowest watermark across everyone else in
    /// the Space (a group shows read only when all have read, like WhatsApp).
    fn receipt_for(&self, e: &Entry) -> Delivery {
        let me = self.me.as_deref().unwrap_or_default();
        let Some(space) = self.state.spaces.get(&e.space) else {
            return Delivery::Sent;
        };
        if e.author != me {
            return Delivery::Sent;
        }
        let marks = self.net.receipts.get(&e.space);
        let mut others = space
            .members
            .iter()
            .map(|(m, _)| m)
            .filter(|m| *m != me)
            .peekable();
        if others.peek().is_none() {
            return Delivery::Sent;
        }
        let (mut delivered, mut read) = (true, true);
        for m in others {
            let (d, r) = marks.and_then(|x| x.get(m)).copied().unwrap_or((0, 0));
            read &= r >= e.seq;
            delivered &= d.max(r) >= e.seq;
        }
        match (read, delivered) {
            (true, _) => Delivery::Read,
            (_, true) => Delivery::Delivered,
            _ => Delivery::Sent,
        }
    }

    fn receipt_members(&self, space: &str) -> R<Cow<'_, [(IdentityId, Role)]>> {
        let pending_membership = self.net.pending.values().any(|s| s == space)
            && self.store.outbox()?.iter().any(|pending| {
                !pending.failed
                    && pending.event.space == space
                    && matches!(
                        pending.event.body,
                        EventBody::SpaceCreated { .. }
                            | EventBody::MemberAdded { .. }
                            | EventBody::MemberRemoved { .. }
                    )
                    && !self.logs.get(space).is_some_and(|log| {
                        log.events()
                            .iter()
                            .any(|e| e.client_id == pending.event.client_id)
                    })
            });
        if !pending_membership {
            return Ok(Cow::Borrowed(
                self.state
                    .spaces
                    .get(space)
                    .map(|s| s.members.as_slice())
                    .unwrap_or_default(),
            ));
        }
        // Optimistic membership is for UI feedback, not admission of relay marks.
        // Reuse the confirmed authority projection only while such an overlay exists.
        let mut confirmed = State::default();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        if let Some(log) = self.logs.get(space) {
            for (end, event) in log.events().iter().enumerate() {
                let context = plans.before(&confirmed, &self.identities, event, end)?;
                confirmed.apply_checked(event, &self.identities, &context);
            }
        }
        Ok(Cow::Owned(
            confirmed
                .spaces
                .remove(space)
                .map(|s| s.members)
                .unwrap_or_default(),
        ))
    }

    pub(crate) fn receipt_scope(&self, space: &str, from: &str) -> R<bool> {
        let Some(me) = self.me.as_deref() else {
            return Ok(false);
        };
        let members = self.receipt_members(space)?;
        Ok(members.iter().any(|(member, _)| member == me)
            && members.iter().any(|(member, _)| member == from))
    }

    /// A signed, checked canonical membership transition has removed the reader
    /// or subject. Outbox/UI optimism cannot erase this cut or resurrect a mark.
    /// This is only evaluated for unresolved/unauthorized arrivals, not hot valid
    /// receipts. It reuses the existing checked projection and PlanReplay.
    pub(crate) fn receipt_confirmed_removed(&self, space: &str, from: &str) -> R<bool> {
        let Some(me) = self.me.as_deref() else {
            return Ok(false);
        };
        let Some(log) = self.logs.get(space) else {
            return Ok(false);
        };
        let mut confirmed = State::default();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        let mut removed = [false, false];
        for (end, event) in log.events().iter().enumerate() {
            let present = |state: &State, who: &str| {
                state
                    .spaces
                    .get(space)
                    .is_some_and(|room| room.members.iter().any(|(id, _)| id == who))
            };
            let before = [present(&confirmed, me), present(&confirmed, from)];
            let context = plans.before(&confirmed, &self.identities, event, end)?;
            confirmed.apply_checked(event, &self.identities, &context);
            for (index, who) in [me, from].into_iter().enumerate() {
                let after = present(&confirmed, who);
                if after {
                    removed[index] = false;
                } else if before[index] {
                    removed[index] = true;
                }
            }
        }
        Ok(removed.into_iter().any(|value| value))
    }

    fn restore_peer_receipts(&mut self) -> R<()> {
        // Older demo databases kept sample ticks in one special metadata blob.
        // Migrate only canonical members, atomically, then use the ordinary cache.
        if let Some(legacy) = self.store.meta("seed_receipts")? {
            let storage_error = |error: rusqlite::Error| CoreError::Storage {
                message: error.to_string(),
            };
            let tx = self
                .store
                .conn()
                .unchecked_transaction()
                .map_err(storage_error)?;
            if let Ok(legacy) =
                serde_json::from_str::<HashMap<SpaceId, HashMap<IdentityId, (u64, u64)>>>(&legacy)
            {
                for (space, peers) in legacy {
                    for (peer, (delivered, read)) in peers {
                        if self.me.as_deref() != Some(&peer)
                            && self.receipt_scope(&space, &peer)?
                            && delivered.max(read) > 0
                        {
                            self.store
                                .raise_peer_receipt(&space, &peer, delivered, read)?;
                        }
                    }
                }
            }
            self.store.meta_delete("seed_receipts")?;
            tx.commit().map_err(storage_error)?;
        }
        let saved = self.store.peer_receipts()?;
        let mut receipts: HashMap<SpaceId, HashMap<IdentityId, (u64, u64)>> = HashMap::new();
        for (space, peers) in &self.net.receipts {
            for (peer, mark) in peers {
                if self.me.as_deref() != Some(peer) && self.receipt_scope(space, peer)? {
                    receipts
                        .entry(space.clone())
                        .or_default()
                        .insert(peer.clone(), *mark);
                }
            }
        }
        for row in saved {
            if self.me.as_deref() != Some(&row.identity)
                && self.receipt_scope(&row.space, &row.identity)?
            {
                let mark = receipts
                    .entry(row.space)
                    .or_default()
                    .entry(row.identity)
                    .or_insert((0, 0));
                *mark = (
                    mark.0.max(row.delivered).max(row.read),
                    mark.1.max(row.read),
                );
            }
        }
        self.net.receipts = receipts;
        Ok(())
    }

    /// A current member's receipt arrived (live or replayed). Persist before publishing
    /// a higher mark so reconnect or cold/offline reopen cannot lower observed ticks.
    /// From my own identity, a read on another device is a read here too.
    pub fn receipt(&mut self, space: &str, from: &str, delivered: u64, read: u64) -> R<bool> {
        if !self.receipt_scope(space, from)? {
            return Ok(false);
        }
        if self.me.as_deref() == Some(from) {
            if read == 0 {
                return Ok(false);
            }
            let key = format!("read:{space}");
            let previous = self.store.meta(&key)?.and_then(|v| v.parse::<u64>().ok());
            if previous.is_some_and(|seq| seq >= read) {
                return Ok(false);
            }
            self.store.set_meta(&key, &read.to_string())?;
            return Ok(true);
        }
        let previous = self
            .net
            .receipts
            .get(space)
            .and_then(|peers| peers.get(from))
            .copied()
            .unwrap_or((0, 0));
        let next = (previous.0.max(delivered).max(read), previous.1.max(read));
        if next == previous {
            return Ok(false);
        }
        let saved = self.store.raise_peer_receipt(space, from, next.0, next.1)?;
        self.net
            .receipts
            .entry(space.to_string())
            .or_default()
            .insert(from.to_string(), saved);
        Ok(saved != previous)
    }

    /// The `Delivered` watermark to announce for `space`, when its head moved past the last
    /// one announced and the head is someone else's message.
    pub fn delivered_to_announce(&self, space: &str) -> Option<u64> {
        let me = self.me.clone().unwrap_or_default();
        let s = self.state.spaces.get(space)?;
        let head = s
            .entries
            .iter()
            .filter(|e| e.author != me)
            .map(|e| e.seq)
            .max()?;
        let last = self.net.delivered_sent.get(space).copied().unwrap_or(0);
        (head > last).then_some(head)
    }

    /// Advance only after the socket accepted the frame. Reconnect still replays it:
    /// a successful write alone cannot prove the relay received it.
    pub fn delivered_announced(&mut self, space: &str, seq: u64) {
        let last = self
            .net
            .delivered_sent
            .entry(space.to_string())
            .or_default();
        *last = (*last).max(seq);
    }

    /// Reconstruct ephemeral receipts from local history after authenticated catch-up.
    /// Read marks are durable, including reads made while the network task was stopped.
    pub fn receipts_to_replay(&mut self) -> Vec<(String, protocol::EphemeralKind)> {
        self.net.delivered_sent.clear();
        let mut frames = Vec::new();
        for space in &self.net.synced {
            if !self.state.spaces.contains_key(space) {
                continue;
            }
            if let Some(seq) = self.delivered_to_announce(space) {
                frames.push((space.clone(), protocol::EphemeralKind::Delivered { seq }));
            }
            if let Some(seq) = self.read_up_to(space).filter(|seq| *seq > 0) {
                frames.push((space.clone(), protocol::EphemeralKind::Read { seq }));
            }
        }
        frames
    }

    pub fn set_presence(&mut self, who: &str, online: bool) {
        self.net.presence.insert(who.to_string(), online);
    }

    pub fn is_online(&self, who: &str) -> bool {
        self.net.presence.get(who).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    fn agent(owner: &Author, id: String) -> Identity {
        Identity {
            owner_proof: Some(Box::new(
                event_log::agent_owner::authorize(owner, &id).unwrap(),
            )),
            id,
            kind: IdentityKind::Agent,
            name: "Agent".into(),
            handle: "agent".into(),
            tint_hex: "#fff".into(),
            glyph: None,
            owner: Some(owner.identity.clone()),
            bio: String::new(),
        }
    }

    #[test]
    fn remote_agents_need_authorization_and_cannot_change_their_owner() {
        let mut e = Engine::open(":memory:").unwrap();
        let owner = Author::device(&Signer::generate(), Signer::generate());
        let original = agent(&owner, Signer::generate().id());
        let mut unsigned = original.clone();
        unsigned.owner_proof = None;
        e.put_profiles(vec![unsigned]).unwrap();
        assert!(!e.identities.contains_key(&original.id));
        e.put_profiles(vec![original.clone()]).unwrap();
        let other = Author::device(&Signer::generate(), Signer::generate());
        let transferred = agent(&other, original.id.clone());
        e.put_profiles(vec![transferred]).unwrap();
        assert_eq!(e.identities[&original.id], original);
        let mut updated = original.clone();
        updated.name = "New name".into();
        e.put_profiles(vec![updated.clone()]).unwrap();
        assert_eq!(e.identities[&original.id], updated);
        e.reload().unwrap();
        assert_eq!(e.identities[&original.id], updated);
    }

    #[test]
    fn authorization_requires_an_unlocked_account_device_and_does_not_issue_a_grant() {
        let mut e = Engine::open(":memory:").unwrap();
        let agent = Signer::generate().id();
        assert!(e.authorize_agent(&agent).is_err());
        e.create_account("Owner", "owner", "http://127.0.0.1:9")
            .unwrap();
        let grants = e.state.grants.len();
        let proof = e.authorize_agent(&agent).unwrap();
        assert!(event_log::agent_owner::verify(
            &e.account().unwrap().identity,
            &agent,
            &proof
        ));
        assert_eq!(e.state.grants.len(), grants);
        assert!(e.authorize_agent("invalid").is_err());
        e.net.author = None;
        assert!(e.authorize_agent(&agent).is_err());
    }

    #[test]
    fn unsigned_legacy_metadata_cannot_authorize_trust_after_reload() {
        let mut e = Engine::open(":memory:").unwrap();
        e.create_account("Owner", "owner", "http://127.0.0.1:9")
            .unwrap();
        let owner = e.net.author.clone().unwrap();
        let mut legacy = agent(&owner, Signer::generate().id());
        legacy.owner_proof = None;
        e.store.put_identity(&legacy, None).unwrap();
        let space = e
            .create_synced_space(
                "Group",
                SpaceKind::Group,
                Privacy::Closed,
                std::slice::from_ref(&legacy.id),
            )
            .unwrap();
        e.reload().unwrap();
        assert!(!e.agents().iter().any(|a| a.persona.id == legacy.id));
        assert!(e.set_trust(&legacy.id, &space, TrustLevel::Act).is_err());
        assert!(e.net.unknown.contains(&legacy.id));

        let mut correct = legacy;
        correct.kind = IdentityKind::Person;
        correct.owner = None;
        e.put_profiles(vec![correct.clone()]).unwrap();
        e.reload().unwrap();
        assert_eq!(e.identities[&correct.id].kind, IdentityKind::Person);
        assert_eq!(e.identities[&correct.id].owner, None);
        assert_eq!(e.identities[&correct.id].owner_proof, None);
        assert!(e.set_trust(&correct.id, &space, TrustLevel::Act).is_err());

        let me = e.me_id().unwrap();
        let original = e.identities[&me].clone();
        let other = Author::device(&Signer::generate(), Signer::generate());
        e.put_profiles(vec![agent(&other, me.clone())]).unwrap();
        assert_eq!(e.identities[&me], original);
    }
}
