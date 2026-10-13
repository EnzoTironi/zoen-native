use std::collections::{BTreeMap, BTreeSet};

use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::Signer;
use roda_log::{
    content::{InnerEvent, Payload, Sealed, SealedKind, SignedContent},
    SpaceLog,
};
use roda_mls::{ExternalDevice, GroupState, Leaf, Opened, SUITE_ID};
use roda_proto::{ClientFrame, Cursor, Envelope, Op, Reply, Sequenced, ServerFrame};
use roda_types::{Event, EventBody, Identity, Privacy, Role, Seen, SpaceKind};
use serde::{Deserialize, Serialize};

use crate::LinkedAccount;

const PACKAGES_REQUEST: u64 = 0x8000_0001;
const PROFILES_REQUEST: u64 = 0x8000_0002;
const SESSION_VERSION: u32 = 2;
const RETRY_START_MS: u64 = 1_000;
const RETRY_CEILING_MS: u64 = 300_000;
const PENDING_CHECKPOINT_CEILING: usize = 4096;
const INTEGRITY_ERROR: &str =
    "The encrypted conversation has conflicting membership. Open it on your phone.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    DuplicateInvite,
    OutOfDate,
    Other(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateInvite => f.write_str("This invitation is already pending."),
            Self::OutOfDate => {
                f.write_str("This browser session is out of date. Pair this browser again.")
            }
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl From<String> for ClientError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for ClientError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

/// A JSON field that must be present. `null` stays empty. A missing key is refused.
/// `Option` accepts a missing key as `None`, so it cannot express that rule.
#[derive(Serialize, Deserialize, Clone)]
struct JsonField<T>(Option<T>);

impl JsonField<String> {
    fn as_str(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct PendingEnvelope {
    content: Vec<u8>,
    sig: String,
    cert: String,
    inner: Option<Event>,
    epoch: Option<u64>,
    failure: Option<String>,
    invite: JsonField<String>,
    retry_not_before_ms: i64,
    retry_delay_ms: u64,
}

impl PendingEnvelope {
    fn envelope(&self) -> Result<Envelope, String> {
        Envelope::new(
            self.content.clone(),
            self.sig.clone(),
            Some(self.cert.clone()),
            self.invite.0.clone(),
        )
        .ok_or_else(|| "invalid saved envelope".into())
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Packages {
    single: Vec<Vec<u8>>,
    last_resort: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Default)]
struct SavedClient {
    version: u32,
    storage: Vec<(Vec<u8>, Vec<u8>)>,
    logs: BTreeMap<String, Vec<Event>>,
    outbox: BTreeMap<String, PendingEnvelope>,
    profiles: BTreeMap<String, Identity>,
    packages: Option<Packages>,
    packages_published: bool,
    joining: BTreeSet<String>,
    blocked_epochs: BTreeMap<String, u64>,
    integrity_errors: BTreeMap<String, String>,
    epoch_digests: BTreeMap<String, BTreeMap<u64, String>>,
    pending_checkpoints: BTreeMap<String, BTreeMap<String, PendingCheckpoint>>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
struct PendingCheckpoint {
    epoch: u64,
    digest: String,
    upto_seq: u64,
    upto_hash: String,
}

#[derive(Serialize)]
pub struct MessageView {
    id: String,
    author: String,
    name: String,
    text: String,
    at_ms: i64,
    delivery: &'static str,
    failure: Option<String>,
}

#[derive(Serialize)]
pub struct ConversationView {
    id: String,
    title: String,
    kind: &'static str,
    can_send: bool,
    waiting: Option<String>,
    unreadable: usize,
    messages: Vec<MessageView>,
}

#[derive(Serialize)]
pub struct Update {
    pub snapshot: String,
    pub frames: Vec<Vec<u8>>,
    pub conversations: Vec<ConversationView>,
    pub retry_at_ms: Option<i64>,
}

/// The same OpenMLS crypto, log verification and binary protocol used by native clients.
/// Its host encrypts and durably writes each Update before sending any of its frames.
pub struct Client<S> {
    account: LinkedAccount,
    signer: S,
    provider: OpenMlsRustCrypto,
    saved: SavedClient,
    logs: BTreeMap<String, SpaceLog>,
    synced: bool,
    sent: BTreeSet<String>,
    packages_in_flight: bool,
    now_ms: i64,
}

impl<S: Signer> Client<S> {
    pub fn new(account: LinkedAccount, signer: S, snapshot: &str) -> Result<Self, ClientError> {
        let saved = load_snapshot(snapshot)?;
        let logs = saved
            .logs
            .iter()
            .map(|(space, events)| {
                SpaceLog::from_events(space, events.clone())
                    .map(|log| (space.clone(), log))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        for pending in saved.outbox.values() {
            let envelope = pending.envelope()?;
            envelope.verify().map_err(|e| e.to_string())?;
            if envelope.author() != account.identity
                || envelope.device() != Some(account.device.as_str())
            {
                return Err("outbox belongs to another browser device".into());
            }
            if let Some(inner) = &pending.inner {
                roda_log::verify_author(inner).map_err(|e| e.to_string())?;
            }
        }
        let provider = OpenMlsRustCrypto::default();
        *provider
            .storage()
            .values
            .write()
            .map_err(|_| "MLS storage lock failed")? = saved.storage.iter().cloned().collect();
        let mut client = Self {
            account,
            signer,
            provider,
            saved,
            logs,
            synced: false,
            sent: BTreeSet::new(),
            packages_in_flight: false,
            now_ms: 0,
        };
        client.device()?;
        client.restore_checkpoints();
        Ok(client)
    }

    fn device(&self) -> Result<ExternalDevice<'_, OpenMlsRustCrypto, S>, ClientError> {
        let leaf = Leaf {
            identity: self.account.identity.clone(),
            device: self.account.device.clone(),
            cert: self.account.cert.clone(),
        };
        let public = hex::decode(&leaf.device).map_err(|e| e.to_string())?;
        ExternalDevice::new(&self.provider, &self.signer, leaf, public)
            .map_err(|error| ClientError::from(error.to_string()))
    }

    fn roster(&self, space: &str) -> BTreeMap<String, Role> {
        let mut roster = BTreeMap::new();
        for event in self
            .logs
            .get(space)
            .into_iter()
            .flat_map(|log| log.events())
        {
            match &event.body {
                EventBody::SpaceCreated { .. } => {
                    roster.insert(event.author.clone(), Role::Owner);
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

    fn head(&self, space: &str) -> Option<Seen> {
        self.logs.get(space).and_then(SpaceLog::head)
    }

    fn metadata(&self, space: &str) -> Option<(String, SpaceKind, Privacy)> {
        let mut metadata = None;
        for event in self.logs.get(space)?.events() {
            match &event.body {
                EventBody::SpaceCreated {
                    title,
                    kind,
                    privacy,
                } => metadata = Some((title.clone(), *kind, *privacy)),
                EventBody::SpaceEncrypted => {
                    if let Some((_, _, privacy)) = &mut metadata {
                        *privacy = Privacy::EndToEnd;
                    }
                }
                _ => {}
            }
        }
        metadata
    }

    fn ready_epoch(&self, space: &str) -> Result<u64, ClientError> {
        if let Some(error) = self.saved.integrity_errors.get(space) {
            return Err(error.clone().into());
        }
        if !self.synced {
            return Err("Catching up with the conversation…".into());
        }
        let (_, _, privacy) = self.metadata(space).ok_or("Conversation not found.")?;
        if privacy != Privacy::EndToEnd {
            return Err("Open this conversation in the native app.".into());
        }
        let roster = self.roster(space);
        match roster.get(&self.account.identity) {
            Some(Role::Owner | Role::Admin | Role::Member) => {}
            Some(Role::Reader) => return Err("You can read this conversation.".into()),
            None => return Err("You are no longer a member of this conversation.".into()),
        }
        let device = self.device()?;
        if device.group_state(space).map_err(|e| e.to_string())? != GroupState::Ready {
            return Err(
                "Waiting for your phone to add this browser to the encrypted conversation…".into(),
            );
        }
        let members = device.roster(space).map_err(|e| e.to_string())?;
        if members != roster.keys().cloned().collect() {
            return Err("Waiting for the encrypted membership update…".into());
        }
        let epoch = device.epoch(space).map_err(|e| e.to_string())?;
        if self.saved.blocked_epochs.get(space) == Some(&epoch) {
            return Err("Waiting to catch up with the encrypted conversation…".into());
        }
        Ok(epoch)
    }

    fn sign_content(&self, content: Vec<u8>, sealed: bool) -> Result<PendingEnvelope, ClientError> {
        let hash = if sealed {
            roda_log::content::signed_hash(&content)
        } else {
            roda_log::content::content_hash(&content)
        };
        let message = if sealed {
            roda_log::content::signature_message(&content, &hash)
        } else {
            hash.into_bytes()
        };
        let sig = hex::encode(self.signer.sign(&message).map_err(|e| format!("{e:?}"))?);
        let pending = PendingEnvelope {
            content,
            sig,
            cert: self.account.cert.clone(),
            inner: None,
            epoch: None,
            failure: None,
            invite: JsonField(None),
            retry_not_before_ms: 0,
            retry_delay_ms: 0,
        };
        pending.envelope()?.verify().map_err(|e| e.to_string())?;
        Ok(pending)
    }

    fn clear_event(
        &self,
        space: &str,
        body: EventBody,
        now_ms: i64,
    ) -> Result<PendingEnvelope, ClientError> {
        let content = SignedContent {
            v: 3,
            space: space.into(),
            client_id: roda_types::new_ulid(now_ms),
            author: self.account.identity.clone(),
            device: Some(self.account.device.clone()),
            at_ms: now_ms,
            seen: self.head(space).as_ref().map(Into::into),
            payload: Some(Payload::Body(roda_log::content::encode_body(&body))),
        }
        .encode();
        self.sign_content(content, false)
    }

    fn seal_inner(&self, event: &Event, epoch: u64) -> Result<PendingEnvelope, ClientError> {
        let plaintext = InnerEvent {
            content: event.content.clone(),
            sig: event.sig.clone(),
        }
        .encode();
        let ciphertext = self
            .device()?
            .seal(&event.space, &plaintext)
            .map_err(|e| e.to_string())?;
        let content = SignedContent {
            v: roda_log::content::SEALED_CONTENT_VERSION,
            space: event.space.clone(),
            client_id: event.client_id.clone(),
            author: event.author.clone(),
            device: event.device.clone(),
            at_ms: event.at_ms,
            seen: event.seen.as_ref().map(Into::into),
            payload: Some(Payload::Sealed(Sealed::new(
                SealedKind::Application,
                SUITE_ID,
                ciphertext,
            ))),
        }
        .encode();
        let mut pending = self.sign_content(content, true)?;
        pending.inner = Some(event.clone());
        pending.epoch = Some(epoch);
        Ok(pending)
    }

    pub fn queue_message(
        &mut self,
        space: &str,
        text: &str,
        now_ms: i64,
    ) -> Result<Update, ClientError> {
        self.now_ms = now_ms;
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Write a message of up to 32 KB.".into());
        }
        let epoch = self.ready_epoch(space)?;
        let inner = self.clear_event(
            space,
            EventBody::MessagePosted {
                message: roda_types::new_secret_id("msg"),
                text: text.into(),
                attaches: None,
                reply: None,
            },
            now_ms,
        )?;
        let envelope = inner.envelope()?;
        let event = envelope.plain_event().ok_or("invalid inner message")?;
        let pending = self.seal_inner(&event, epoch)?;
        self.saved.outbox.insert(event.client_id.clone(), pending);
        self.update(Vec::new())
    }

    pub fn bootstrap(&mut self) -> Result<Update, ClientError> {
        self.synced = false;
        self.sent.clear();
        self.packages_in_flight = false;
        if !self.saved.packages_published && self.saved.packages.is_none() {
            self.prepare_packages(true)?;
        }
        let cursors = self
            .logs
            .iter()
            .map(|(space, log)| Cursor {
                space: space.clone(),
                next_seq: log.next_seq(),
            })
            .collect();
        self.update(vec![ClientFrame::Sync { cursors, all: true }.encode()])
    }

    pub fn advance(&mut self, now_ms: i64) -> Result<Update, ClientError> {
        self.now_ms = now_ms;
        self.update(Vec::new())
    }

    pub fn queue_join(
        &mut self,
        space: &str,
        role: Role,
        code: &str,
        now_ms: i64,
    ) -> Result<Update, ClientError> {
        self.now_ms = now_ms;
        if !roda_proto::valid_invite_code(code) || space.is_empty() {
            return Err("Invalid invitation.".into());
        }
        if self.roster(space).contains_key(&self.account.identity) {
            return self.update(Vec::new());
        }
        if self
            .saved
            .outbox
            .values()
            .any(|pending| pending.invite.as_str() == Some(code))
        {
            return Err(ClientError::DuplicateInvite);
        }
        let mut pending = self.clear_event(
            space,
            EventBody::MemberAdded {
                identity: self.account.identity.clone(),
                role,
            },
            now_ms,
        )?;
        pending.invite = JsonField(Some(code.to_string()));
        let id = pending.envelope()?.client_id().to_string();
        self.saved.outbox.insert(id, pending);
        self.update(Vec::new())
    }

    fn prepare_packages(&mut self, first: bool) -> Result<(), ClientError> {
        let mut single = self
            .device()?
            .key_packages(32, first)
            .map_err(|e| e.to_string())?;
        let last_resort = if first { single.pop() } else { None };
        self.saved.packages = Some(Packages {
            single,
            last_resort,
        });
        Ok(())
    }

    pub fn receive(&mut self, bytes: &[u8], now_ms: i64) -> Result<Update, ClientError> {
        self.now_ms = now_ms;
        let mut extra = Vec::new();
        match ServerFrame::decode(bytes).map_err(|e| e.to_string())? {
            ServerFrame::Event { ev } => {
                let space = ev.env.space().to_string();
                let next = self.logs.get(&space).map(SpaceLog::next_seq).unwrap_or(0);
                if ev.seq > next {
                    return self.update(vec![ClientFrame::Sync {
                        cursors: vec![Cursor {
                            space,
                            next_seq: next,
                        }],
                        all: false,
                    }
                    .encode()]);
                }
                self.ingest(ev, now_ms)?;
            }
            ServerFrame::SyncDone => {
                self.synced = true;
                self.ask_rejoin(now_ms)?;
                let ids = self
                    .logs
                    .values()
                    .flat_map(|log| log.events().iter().map(|event| event.author.clone()))
                    .filter(|identity| !self.saved.profiles.contains_key(identity))
                    .collect::<BTreeSet<_>>();
                if !ids.is_empty() {
                    extra.push(
                        ClientFrame::Req {
                            id: PROFILES_REQUEST,
                            op: Op::Profiles {
                                ids: ids.into_iter().collect(),
                            },
                        }
                        .encode(),
                    );
                }
            }
            ServerFrame::Joined { space } => extra.push(
                ClientFrame::Sync {
                    cursors: vec![Cursor { space, next_seq: 0 }],
                    all: false,
                }
                .encode(),
            ),
            ServerFrame::KeyPackagesLow { device, .. }
                if device == self.account.device && self.saved.packages.is_none() =>
            {
                self.prepare_packages(!self.saved.packages_published)?
            }
            ServerFrame::Res {
                id: PACKAGES_REQUEST,
                result: Ok(Reply::Done),
            } => {
                self.saved.packages = None;
                self.saved.packages_published = true;
                self.packages_in_flight = false;
            }
            ServerFrame::Res {
                id: PACKAGES_REQUEST,
                result: Err(error),
            } => return Err(format!("Could not publish encrypted device keys: {error}").into()),
            ServerFrame::Res {
                result: Ok(Reply::Profiles(profiles)),
                ..
            } => {
                for profile in profiles {
                    self.saved.profiles.insert(profile.id.clone(), profile);
                }
            }
            ServerFrame::Rejected {
                client_id,
                reason,
                permanent,
                ..
            } => {
                let stale_seal = reason == roda_proto::STALE_SEAL;
                let blocked = if stale_seal {
                    match self.saved.outbox.get(&client_id) {
                        Some(pending) => match pending.epoch {
                            Some(epoch) => Some((pending.envelope()?.space().to_string(), epoch)),
                            None => None,
                        },
                        None => None,
                    }
                } else {
                    None
                };
                if let Some((space, epoch)) = blocked {
                    self.saved.blocked_epochs.insert(space, epoch);
                } else if !stale_seal {
                    if let Some(pending) = self.saved.outbox.get_mut(&client_id) {
                        if permanent {
                            pending.failure = Some(reason);
                        } else {
                            let delay = next_retry_delay(pending.retry_delay_ms);
                            pending.retry_delay_ms = delay;
                            pending.retry_not_before_ms = self
                                .now_ms
                                .saturating_add(i64::try_from(delay).unwrap_or(i64::MAX));
                        }
                    }
                }
                self.sent.remove(&client_id);
            }
            _ => {}
        }
        self.update(extra)
    }

    fn ask_rejoin(&mut self, now_ms: i64) -> Result<(), ClientError> {
        let mut missing = Vec::new();
        for space in self.logs.keys() {
            if self.metadata(space).is_some_and(|(_, kind, privacy)| {
                privacy == Privacy::EndToEnd && kind != SpaceKind::Personal
            }) && self.roster(space).contains_key(&self.account.identity)
                && !self.saved.joining.contains(space)
                && self
                    .device()?
                    .group_state(space)
                    .map_err(|e| e.to_string())?
                    == GroupState::Missing
            {
                missing.push(space.clone());
            }
        }
        for space in missing {
            let pending = self.clear_event(
                &space,
                EventBody::DeviceJoining {
                    device: self.account.device.clone(),
                },
                now_ms,
            )?;
            let id = pending.envelope()?.client_id().to_string();
            self.saved.outbox.insert(id, pending);
            self.saved.joining.insert(space);
        }
        Ok(())
    }

    fn ingest(&mut self, sequenced: Sequenced, now_ms: i64) -> Result<(), ClientError> {
        let envelope = &sequenced.env;
        let space = envelope.space().to_string();
        let mut log = self
            .logs
            .get(&space)
            .cloned()
            .unwrap_or_else(|| SpaceLog::new(&space));
        if sequenced.seq < log.next_seq() {
            let retained = log
                .events()
                .get(sequenced.seq as usize)
                .ok_or("invalid event sequence")?;
            if retained.hash != sequenced.hash {
                return Err("The relay sent a conflicting conversation history.".into());
            }
            return Ok(());
        }
        envelope.verify().map_err(|e| e.to_string())?;
        let outer = roda_log::event_from_content(
            envelope.content().to_vec(),
            envelope.sig.clone(),
            envelope.cert.clone(),
            sequenced.seq,
            sequenced.prev.clone(),
            sequenced.hash.clone(),
        )
        .map_err(|e| e.to_string())?;
        let mut outer_check = log.clone();
        outer_check
            .accept(outer.clone())
            .map_err(|e| e.to_string())?;
        let roster = self.roster(&space).keys().cloned().collect();
        let admitted = Leaf {
            identity: envelope.author().into(),
            device: envelope.device().unwrap_or(envelope.author()).into(),
            cert: envelope.cert.clone().unwrap_or_default(),
        };
        let own = self
            .saved
            .outbox
            .get(envelope.client_id())
            .filter(|pending| {
                pending.content == envelope.content()
                    && pending.sig == envelope.sig
                    && envelope.cert.as_deref() == Some(pending.cert.as_str())
            })
            .cloned();
        let mut opened = None;
        let mut landed = false;
        if !envelope.is_pruned() {
            if let Some((kind, ciphertext)) = envelope.sealed_data() {
                let device = self.device()?;
                match kind {
                    SealedKind::Application => {
                        if let Some(inner) = own.as_ref().and_then(|pending| pending.inner.clone())
                        {
                            opened = Some(inner);
                        } else {
                            match device.open(&space, ciphertext, &roster, &admitted) {
                                Ok(Opened::Application { plaintext, from }) => {
                                    opened = inner_event(&sequenced, &plaintext, &from)
                                }
                                Ok(_) => {}
                                Err(roda_mls::MlsError::Storage(error)) => return Err(error.into()),
                                Err(_) => {}
                            }
                        }
                    }
                    SealedKind::Welcome
                        if device.group_state(&space).map_err(|e| e.to_string())?
                            == GroupState::Missing =>
                    {
                        match device.join(&space, ciphertext, &roster) {
                            Ok(joined) => landed = joined,
                            Err(roda_mls::MlsError::Storage(error)) => return Err(error.into()),
                            Err(_) => {}
                        }
                    }
                    SealedKind::Commit => {
                        match device.open(&space, ciphertext, &roster, &admitted) {
                            Ok(Opened::Commit { .. }) => landed = true,
                            Ok(_) => {}
                            Err(roda_mls::MlsError::Storage(error)) => return Err(error.into()),
                            Err(_) => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        let event = if let Some(mut inner) = opened {
            inner.seq = sequenced.seq;
            inner.prev = sequenced.prev.clone();
            inner.hash = sequenced.hash.clone();
            inner.sealed_wire = Some(envelope.wire_hash());
            inner
        } else {
            outer
        };
        log.accept(event.clone()).map_err(|e| e.to_string())?;
        self.logs.insert(space.clone(), log);
        if own.is_some() {
            self.saved.outbox.remove(envelope.client_id());
            self.sent.remove(envelope.client_id());
        }
        if matches!(&event.body, EventBody::MemberRemoved { identity } if identity == &self.account.identity)
        {
            self.device()?.forget(&space).map_err(|e| e.to_string())?;
            self.saved.joining.remove(&space);
        }
        self.observe_checkpoint(&event);
        self.remember_current_epoch(&space);
        self.settle_pending(&space);
        if landed {
            self.saved.joining.remove(&space);
            if let Ok((epoch, digest)) = self.device()?.checkpoint(&space) {
                let checkpoint = self.clear_event(
                    &space,
                    EventBody::Checkpoint {
                        upto: Seen {
                            seq: sequenced.seq,
                            hash: sequenced.hash,
                        },
                        epoch,
                        digest,
                    },
                    now_ms,
                )?;
                self.saved
                    .outbox
                    .insert(checkpoint.envelope()?.client_id().to_string(), checkpoint);
            }
        }
        Ok(())
    }

    pub fn update(&mut self, mut frames: Vec<Vec<u8>>) -> Result<Update, ClientError> {
        let reseal = self
            .saved
            .outbox
            .iter()
            .filter_map(|(id, pending)| {
                let event = pending.inner.as_ref()?;
                let epoch = self.ready_epoch(&event.space).ok()?;
                (pending.failure.is_none() && pending.epoch != Some(epoch))
                    .then(|| (id.clone(), event.clone(), epoch))
            })
            .collect::<Vec<_>>();
        for (id, event, epoch) in reseal {
            let pending = self.seal_inner(&event, epoch)?;
            self.sent.remove(&id);
            self.saved.outbox.insert(id, pending);
        }
        if let Some(packages) = self
            .saved
            .packages
            .as_ref()
            .filter(|_| !self.packages_in_flight)
        {
            frames.insert(
                0,
                ClientFrame::Req {
                    id: PACKAGES_REQUEST,
                    op: Op::PublishKeyPackages {
                        packages: packages.single.clone(),
                        last_resort: packages.last_resort.clone(),
                    },
                }
                .encode(),
            );
            self.packages_in_flight = true;
        }
        let queued = self
            .saved
            .outbox
            .iter()
            .map(|(id, pending)| (id.clone(), pending.clone()))
            .collect::<Vec<_>>();
        for (id, pending) in queued {
            if !self.should_publish(&id, &pending)? {
                continue;
            }
            frames.push(
                ClientFrame::Publish {
                    env: pending.envelope()?,
                }
                .encode(),
            );
            self.sent.insert(id.clone());
        }
        self.saved.logs = self
            .logs
            .iter()
            .map(|(space, log)| (space.clone(), log.events().to_vec()))
            .collect();
        self.saved.storage = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let snapshot = serde_json::to_string(&self.saved).map_err(|e| e.to_string())?;
        Ok(Update {
            snapshot,
            frames,
            conversations: self.conversations(),
            retry_at_ms: self.next_retry_at_ms(),
        })
    }

    fn next_retry_at_ms(&self) -> Option<i64> {
        self.saved
            .outbox
            .values()
            .filter(|pending| {
                pending.failure.is_none() && pending.retry_not_before_ms > self.now_ms
            })
            .map(|pending| pending.retry_not_before_ms)
            .min()
    }

    pub fn conversations(&self) -> Vec<ConversationView> {
        self.logs.keys().filter_map(|space| {
            let (title, kind, _) = self.metadata(space)?;
            if kind == SpaceKind::Personal || !self.roster(space).contains_key(&self.account.identity) { return None; }
            let mut messages = self.logs.get(space)?.events().iter().filter_map(|event| self.message_view(event, "confirmed", None)).collect::<Vec<_>>();
            messages.extend(self.saved.outbox.values().filter_map(|pending| {
                let inner = pending.inner.as_ref()?;
                if inner.space != *space { return None; }
                self.message_view(inner, if pending.failure.is_some() { "failed" } else { "sending" }, pending.failure.clone())
            }));
            let waiting = self.ready_epoch(space).err().map(|error| error.to_string());
            let unreadable = self.logs.get(space)?.events().iter().filter(|event| matches!(&event.body, EventBody::Sealed { kind } if kind == "Application")).count();
            Some(ConversationView { id: space.clone(), title, kind: match kind { SpaceKind::Direct=>"direct",SpaceKind::Group=>"group",SpaceKind::Community=>"community",SpaceKind::Personal=>"personal" },
                can_send: waiting.is_none(), waiting, unreadable, messages })
        }).collect()
    }

    fn message_view(
        &self,
        event: &Event,
        delivery: &'static str,
        failure: Option<String>,
    ) -> Option<MessageView> {
        let EventBody::MessagePosted { text, .. } = &event.body else {
            return None;
        };
        let name = if event.author == self.account.identity {
            self.account.name.clone()
        } else {
            self.saved
                .profiles
                .get(&event.author)
                .map(|profile| profile.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| event.author.chars().take(12).collect())
        };
        Some(MessageView {
            id: event.client_id.clone(),
            author: event.author.clone(),
            name,
            text: text.clone(),
            at_ms: event.at_ms,
            delivery,
            failure,
        })
    }

    fn should_publish(&self, id: &str, pending: &PendingEnvelope) -> Result<bool, ClientError> {
        if pending.failure.is_some()
            || self.sent.contains(id)
            || self.now_ms < pending.retry_not_before_ms
        {
            return Ok(false);
        }
        let space = if let Some(event) = &pending.inner {
            event.space.clone()
        } else {
            pending.envelope()?.space().to_string()
        };
        if self.saved.integrity_errors.contains_key(&space) {
            return Ok(false);
        }
        if let Some(event) = &pending.inner {
            if self.ready_epoch(&event.space).ok() != pending.epoch {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn restore_checkpoints(&mut self) {
        let spaces = self.logs.keys().cloned().collect::<Vec<_>>();
        for space in spaces {
            self.remember_current_epoch(&space);
            self.audit_checkpoints(&space);
            self.settle_pending(&space);
        }
    }

    fn current_checkpoint(&self, space: &str) -> Option<(u64, String)> {
        self.device().ok()?.checkpoint(space).ok()
    }

    fn remember_current_epoch(&mut self, space: &str) {
        let Some((epoch, digest)) = self.current_checkpoint(space) else {
            return;
        };
        if let Some(existing) = self
            .saved
            .epoch_digests
            .get(space)
            .and_then(|known| known.get(&epoch).cloned())
        {
            if existing != digest {
                self.mark_integrity(space);
            }
            return;
        }
        self.saved
            .epoch_digests
            .entry(space.to_string())
            .or_default()
            .insert(epoch, digest);
        self.audit_checkpoints(space);
    }

    fn audit_checkpoints(&mut self, space: &str) {
        let checkpoints = self
            .logs
            .get(space)
            .map(|log| {
                log.events()
                    .iter()
                    .filter(|event| matches!(event.body, EventBody::Checkpoint { .. }))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for event in checkpoints {
            if self.saved.integrity_errors.contains_key(space) {
                return;
            }
            self.observe_checkpoint(&event);
        }
    }

    fn observe_checkpoint(&mut self, event: &Event) {
        let EventBody::Checkpoint {
            epoch,
            digest,
            upto,
        } = &event.body
        else {
            return;
        };
        if event.author == self.account.identity
            || self.saved.integrity_errors.contains_key(&event.space)
        {
            return;
        }
        let Some(device) = self.member_device(event) else {
            return;
        };
        let digest_conflict = self
            .saved
            .epoch_digests
            .get(&event.space)
            .and_then(|known| known.get(epoch))
            .is_some_and(|known| known != digest);
        if digest_conflict {
            self.mark_integrity(&event.space);
            return;
        }
        self.note_prefix(&event.space, &device, *epoch, digest.clone(), upto);
    }

    fn member_device(&self, event: &Event) -> Option<String> {
        if !self.roster(&event.space).contains_key(&event.author) {
            return None;
        }
        Some(
            event
                .device
                .clone()
                .filter(|device| !device.is_empty())
                .unwrap_or_else(|| event.author.clone()),
        )
    }

    fn note_prefix(&mut self, space: &str, device: &str, epoch: u64, digest: String, upto: &Seen) {
        let local = self.hash_at(space, upto.seq).map(str::to_string);
        match local.as_deref() {
            Some(hash) if hash == upto.hash => {}
            Some(_) => self.mark_integrity(space),
            None => self.hold_checkpoint(
                space,
                device,
                PendingCheckpoint {
                    epoch,
                    digest,
                    upto_seq: upto.seq,
                    upto_hash: upto.hash.clone(),
                },
            ),
        }
    }

    fn hash_at(&self, space: &str, seq: u64) -> Option<&str> {
        let event = self
            .logs
            .get(space)?
            .events()
            .get(usize::try_from(seq).ok()?)?;
        (event.seq == seq).then_some(event.hash.as_str())
    }

    fn hold_checkpoint(&mut self, space: &str, device: &str, item: PendingCheckpoint) {
        if self.saved.integrity_errors.contains_key(space) {
            return;
        }
        let pending = self
            .saved
            .pending_checkpoints
            .entry(space.to_string())
            .or_default();
        if let Some(existing) = pending.get(device) {
            if item.upto_seq <= existing.upto_seq {
                return;
            }
        } else if pending.len() >= PENDING_CHECKPOINT_CEILING {
            eprintln!("pending checkpoint ceiling reached for {space}");
            return;
        }
        pending.insert(device.to_string(), item);
    }

    fn settle_pending(&mut self, space: &str) {
        let Some(pending) = self.saved.pending_checkpoints.remove(space) else {
            return;
        };
        let mut keep = BTreeMap::new();
        for (device, item) in pending {
            if self.saved.integrity_errors.contains_key(space) {
                return;
            }
            let digest_conflict = self
                .saved
                .epoch_digests
                .get(space)
                .and_then(|known| known.get(&item.epoch))
                .is_some_and(|known| known != &item.digest);
            if digest_conflict {
                self.mark_integrity(space);
                return;
            }
            let local = self.hash_at(space, item.upto_seq).map(str::to_string);
            match local.as_deref() {
                Some(hash) if hash == item.upto_hash => {}
                Some(_) => {
                    self.mark_integrity(space);
                    return;
                }
                None => {
                    keep.insert(device, item);
                }
            }
        }
        if !keep.is_empty() && !self.saved.integrity_errors.contains_key(space) {
            self.saved
                .pending_checkpoints
                .insert(space.to_string(), keep);
        }
    }

    fn mark_integrity(&mut self, space: &str) {
        self.saved
            .integrity_errors
            .entry(space.to_string())
            .or_insert_with(|| INTEGRITY_ERROR.to_string());
        self.saved.pending_checkpoints.remove(space);
    }
}

fn load_snapshot(snapshot: &str) -> Result<SavedClient, ClientError> {
    if snapshot.is_empty() {
        return Ok(SavedClient {
            version: SESSION_VERSION,
            ..Default::default()
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(snapshot).map_err(|error| error.to_string())?;
    match value.get("version").and_then(|version| version.as_u64()) {
        Some(1) => return Err(ClientError::OutOfDate),
        Some(2) => {}
        _ => return Err("unsupported browser session version".into()),
    }
    serde_json::from_value(value).map_err(|error| error.to_string().into())
}

fn next_retry_delay(current: u64) -> u64 {
    if current == 0 {
        RETRY_START_MS
    } else {
        current.saturating_mul(2).min(RETRY_CEILING_MS)
    }
}

fn inner_event(sequenced: &Sequenced, plaintext: &[u8], from: &Leaf) -> Option<Event> {
    let inner = InnerEvent::decode(plaintext)?;
    let event = roda_log::event_from_content(
        inner.content,
        inner.sig,
        sequenced.env.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .ok()?;
    let outer = &sequenced.env;
    (event.space == outer.space()
        && event.client_id == outer.client_id()
        && event.author == outer.author()
        && event.device.as_deref() == outer.device()
        && event.at_ms == outer.at_ms()
        && event.seen == outer.seen()
        && from.identity == event.author
        && Some(from.device.as_str()) == event.device.as_deref()
        && !matches!(
            event.body,
            EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::MemberRemoved { .. }
                | EventBody::ProfileKeyShared { .. }
                | EventBody::SpaceEncrypted
                | EventBody::Checkpoint { .. }
                | EventBody::DeviceJoining { .. }
                | EventBody::Sealed { .. }
        ))
    .then_some(event)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "client_tests.rs"]
mod tests;
