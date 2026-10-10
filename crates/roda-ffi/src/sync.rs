//! # Engine side of Zoen Sync
//!
//! - **Account:** an Ed25519 identity key and a device key certified by it. Secrets live
//!   in the platform vault (iOS Keychain); SQLite keeps only public keys and the cert.
//! - **Outbox:** writing to a relay-ordered Space signs the event, queues it in SQLite
//!   and applies it to the projection at once (shown as *Sending*). The relay assigns the
//!   sequence number when the device is online; the confirmed copy replaces the pending
//!   one in place (same `client_id`, so the UI row never jumps).
//! - **Ingest:** events from the relay must be the next in their Space, link to the head
//!   and carry a valid author signature, or they are refused.
//! - **Reopen:** confirmed events are re-verified from disk, then the outbox is replayed
//!   on top. Kill the app mid-send and nothing is lost or duplicated.

use std::collections::{HashMap, HashSet};

use roda_log::content::SealedKind;
use roda_log::{chain_hash, content_hash_of, Author, Signer, SpaceLog};
use roda_proto::{normalize_handle, Cursor, Envelope, Sequenced};
use roda_types::*;
use serde::{Deserialize, Serialize};

use crate::dto::Delivery;
use crate::engine::{Engine, Entry, State};
use crate::i18n::t;
use crate::CoreError;

type R<T> = Result<T, CoreError>;

pub(crate) const VAULT_IDENTITY: &str = "zoen.identity.v1";
pub(crate) const VAULT_DEVICE: &str = "zoen.device.v1";
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
    /// Someone else's event, now in the projection.
    Applied,
    /// One of ours: the pending copy is now confirmed.
    Confirmed,
    /// Already had it.
    Duplicate,
    /// There's a hole before it: sync this Space from `next`.
    Gap { next: u64 },
    /// Bad signature, broken chain…: refused and not stored.
    Invalid(String),
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

impl Engine {
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
                if matches!(e.body, EventBody::MessagePosted { .. })
                    && self.state.spaces.contains_key(&e.space)
                {
                    self.net
                        .failed
                        .insert(e.client_id.clone(), p.last_error.unwrap_or_default());
                    self.state.apply(&e, &self.identities);
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
            self.state.apply(&e, &self.identities);
        }
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
        self.net.optimistic_approvals.contains(&e.space)
            && matches!(
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

    /// Reconcile only when an ordered dependency can invalidate an optimistic decision.
    /// Use the verified log cache; do not reload keys, MLS state or open page sessions.
    pub(crate) fn reproject_approval_overlay(&mut self) -> R<()> {
        let mut order = self.store.space_ids()?;
        let pending = self.store.outbox()?;
        let mut state = State::default();
        let mut confirmed = HashSet::new();
        for space in &order {
            if let Some(log) = self.logs.get(space) {
                for e in log.events() {
                    confirmed.insert(e.client_id.clone());
                    state.apply(e, &self.identities);
                }
            } else if let Some(broken) = self.state.spaces.get(space) {
                let mut broken = broken.clone();
                broken.entries.clear();
                state.spaces.insert(space.clone(), broken);
            }
        }
        let mut optimistic = HashSet::new();
        for p in pending {
            let e = p.event;
            if confirmed.contains(&e.client_id) {
                continue;
            }
            if p.failed {
                if matches!(e.body, EventBody::MessagePosted { .. })
                    && state.spaces.contains_key(&e.space)
                {
                    state.apply(&e, &self.identities);
                }
                continue;
            }
            if matches!(e.body, EventBody::SpaceCreated { .. }) && !order.contains(&e.space) {
                order.push(e.space.clone());
            }
            if matches!(e.body, EventBody::RequestResolved { .. }) {
                optimistic.insert(e.space.clone());
            }
            state.apply(&e, &self.identities);
        }
        self.state = state;
        self.space_order = order;
        self.net.optimistic_approvals = optimistic;
        self.index_dirty = true;
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
        } else if self.net.synced.contains(&e.space) {
            Delivery::Sent
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
        if !self.state.approval_event_valid(&e, &self.identities) {
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
        self.state.apply(&e, &self.identities);
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
            self.compare_checkpoint(&e);
            self.mls_device_joining(&e);
        }
        r
    }

    fn ingest_plain(&mut self, e: Event) -> Ingest {
        let space = e.space.clone();
        let next = self.logs.get(&space).map(|l| l.next_seq()).unwrap_or(0);
        if e.seq < next {
            return Ingest::Duplicate;
        }
        if e.seq > next {
            return Ingest::Gap { next };
        }
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
            // Disk and memory must agree: drop the in-memory copy, it'll come back on sync.
            let _ = self.reload();
            return Ingest::Invalid(err.to_string());
        }
        let _ = self.store.set_synced(&space);
        self.net.synced.insert(space.clone());
        if !self.space_order.contains(&space) {
            self.space_order.push(space.clone());
        }
        let replay_approval = self.approval_projection_needs_replay(&e);
        let own = self.net.pending.remove(&e.client_id).is_some();
        if own {
            let _ = self.store.outbox_remove(&e.client_id);
        }
        if replay_approval {
            if let Err(err) = self.reproject_approval_overlay() {
                let _ = self.reload();
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
            if let Some(s) = self.state.spaces.get_mut(&space) {
                for entry in s.entries.iter_mut().filter(|x| {
                    x.client_id == e.client_id
                        || x.client_id.strip_suffix(":result") == Some(e.client_id.as_str())
                }) {
                    entry.seq = e.seq;
                    entry.hash = e.hash.clone();
                }
            }
            // A join we sent before we had the Space's history: project it now.
            if !self.state.spaces.contains_key(&space)
                || matches!(e.body, EventBody::MemberAdded { .. })
                    && !self.is_member_of(&space, &e.author)
            {
                self.state.apply(&e, &self.identities);
            }
            return Ingest::Confirmed;
        }
        self.state.apply(&e, &self.identities);
        self.index_dirty = true;
        self.note_unknown(&e);
        self.note_profile_event(&e);
        Ingest::Applied
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
    pub fn reject(
        &mut self,
        client_id: &str,
        reason: &str,
        permanent: bool,
        sent_epoch: Option<u64>,
    ) -> bool {
        if permanent && self.mls_handshake_refused(client_id, reason) {
            return false;
        }
        // Written in the clear just before the Space went end-to-end: it waits for the
        // group and goes out sealed, it doesn't fail.
        // Sealed before this device caught up with a commit: it seals again once it has.
        if let (roda_proto::STALE_SEAL, Some(epoch)) = (reason, sent_epoch) {
            self.mls_sealed_stale(client_id, epoch);
        }
        let permanent =
            permanent && reason != roda_proto::SEAL_REQUIRED && reason != roda_proto::STALE_SEAL;
        let rejected_creation = if permanent {
            self.store.outbox_get(client_id).ok().flatten().filter(|e| {
                matches!(e.body, EventBody::SpaceCreated { .. }) && self.next_seq(&e.space) == 0
            })
        } else {
            None
        };
        let noted = if let Some(creation) = rejected_creation {
            self.store.outbox_refuse_space(&creation.space, reason)
        } else {
            self.store.outbox_note(client_id, reason, permanent)
        };
        if noted.is_err() {
            return false;
        }
        if !permanent {
            return false;
        }
        let _ = self.store.meta_delete(&format!("invite:{client_id}"));
        // Re-project without the refused event (messages stay visible as failed).
        let _ = self.reload();
        true
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

    /// Envelopes still waiting for the relay, oldest first.
    pub fn outbox_envelopes(&self) -> Vec<Envelope> {
        self.outbox_envelopes_except(&HashMap::new())
    }

    /// The outbox as envelopes, leaving out the ones already sent on this connection.
    /// Called after every write, so it skips sent entries before any sealing work and asks
    /// each Space's group for its epoch once: a long import stays linear, not quadratic.
    pub fn outbox_envelopes_except(&self, sent: &HashMap<String, Option<u64>>) -> Vec<Envelope> {
        let mut ready = HashMap::new();
        // A Welcome goes out only once its commit is in: sent together, a commit held back
        // by the relay (rate limit) would let the Welcome arrive first and be refused.
        let held = self.welcomes_behind_commits();
        self.store
            .outbox_heads()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.failed && !sent.contains_key(&p.client_id))
            .filter(|p| !held.contains(&p.client_id))
            // Refused for being clear in a Space that went end-to-end: it waits until this
            // device has caught up with that, then goes out sealed.
            .filter(|p| {
                p.last_error.as_deref() != Some(roda_proto::SEAL_REQUIRED) || self.is_e2e(&p.space)
            })
            .filter_map(|p| {
                let event = self.store.outbox_get(&p.client_id).ok().flatten()?;
                // An offline creation has a predicted genesis link. Its descendants
                // must wait for confirmation: a retryable creation failure can otherwise
                // let them reach the relay before the Space exists.
                if event
                    .seen
                    .as_ref()
                    .is_some_and(|seen| self.next_seq(&event.space) <= seen.seq)
                {
                    return None;
                }
                let mut env = self.outgoing_envelope_with(&event, &mut ready)?;
                if let Some(bytes) = env.recovery() {
                    let reference = roda_log::recovery::RecoveryRef::parse(bytes)?;
                    if !self
                        .store
                        .recovery_upload_confirmed(&reference.blob)
                        .unwrap_or(false)
                    {
                        return None;
                    }
                }
                if !env.is_sealed() {
                    env.invite = self
                        .store
                        .meta(&format!("invite:{}", p.client_id))
                        .ok()
                        .flatten();
                }
                Some(env)
            })
            .collect()
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
            if p.id.len() != 64 || !roda_log::agent_owner::profile_authorized(&p) {
                continue;
            }
            // Never let the directory rename someone this device holds keys for.
            if self.signers.contains_key(&p.id) || self.me.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            if self.identities.get(&p.id).is_some_and(|known| {
                // Unsigned legacy directory metadata cannot establish an owner pin.
                known.owner_proof.is_some()
                    && roda_log::agent_owner::profile_authorized(known)
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
    pub fn authorize_agent(&self, agent: &str) -> R<roda_types::AgentOwnerProof> {
        self.net
            .author
            .as_ref()
            .filter(|author| {
                self.account()
                    .is_some_and(|a| a.identity == author.identity)
            })
            .and_then(|author| roda_log::agent_owner::authorize(author, agent))
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
        let root = Signer::generate();
        let device = Signer::generate();
        let author = Author::device(&root, device.clone());
        let me = Identity {
            owner_proof: None,
            id: root.id(),
            kind: IdentityKind::Person,
            name: name.to_string(),
            handle: handle.clone(),
            tint_hex: tint_for(&root.id()),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        self.store.put_identity(&me, None)?;
        self.identity_order.push(me.id.clone());
        self.identities.insert(me.id.clone(), me.clone());
        self.store.set_meta("me", &me.id)?;
        self.me = Some(me.id.clone());
        self.save_account(AccountMeta {
            identity: me.id.clone(),
            device: device.id(),
            cert: author.cert.clone().unwrap_or_default(),
            relay_url: relay_url.trim_end_matches('/').to_string(),
            registered: false,
        })?;
        self.net.author = Some(author);
        let agreement = self.init_profile(&me.id, &me.name)?;
        self.seed_personal_agent(&me.id)?;
        Ok((root.secret(), device.secret(), agreement))
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
        let personal = new_id("sp");
        self.append_at(
            &personal,
            me,
            at,
            EventBody::SpaceCreated {
                title: "Personal".into(),
                kind: SpaceKind::Personal,
                privacy: Privacy::EndToEnd,
            },
        )?;
        let zoen = self.create_identity(
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
        )?;
        let dm = new_id("sp");
        self.append_at(
            &dm,
            me,
            at,
            EventBody::SpaceCreated {
                title: "Zoen".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            },
        )?;
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
                })
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
                roda_log::agent_owner::authorize(owner, &id).unwrap(),
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
        assert!(roda_log::agent_owner::verify(
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
