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

#[derive(Serialize, Deserialize, Clone)]
struct PendingEnvelope {
    content: Vec<u8>,
    sig: String,
    cert: String,
    inner: Option<Event>,
    epoch: Option<u64>,
    failure: Option<String>,
    #[serde(default)]
    invite: Option<String>,
}

impl PendingEnvelope {
    fn envelope(&self) -> Result<Envelope, String> {
        Envelope::new(
            self.content.clone(),
            self.sig.clone(),
            Some(self.cert.clone()),
            self.invite.clone(),
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
}

impl<S: Signer> Client<S> {
    pub fn new(account: LinkedAccount, signer: S, snapshot: &str) -> Result<Self, String> {
        let saved = if snapshot.is_empty() {
            SavedClient {
                version: 1,
                ..Default::default()
            }
        } else {
            serde_json::from_str::<SavedClient>(snapshot).map_err(|e| e.to_string())?
        };
        if saved.version != 1 {
            return Err("unsupported browser session version".into());
        }
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
        let client = Self {
            account,
            signer,
            provider,
            saved,
            logs,
            synced: false,
            sent: BTreeSet::new(),
            packages_in_flight: false,
        };
        client.device()?;
        Ok(client)
    }

    fn device(&self) -> Result<ExternalDevice<'_, OpenMlsRustCrypto, S>, String> {
        let leaf = Leaf {
            identity: self.account.identity.clone(),
            device: self.account.device.clone(),
            cert: self.account.cert.clone(),
        };
        let public = hex::decode(&leaf.device).map_err(|e| e.to_string())?;
        ExternalDevice::new(&self.provider, &self.signer, leaf, public).map_err(|e| e.to_string())
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

    fn ready_epoch(&self, space: &str) -> Result<u64, String> {
        if !self.synced {
            return Err("Catching up with the conversation…".into());
        }
        if let Some(error) = self.saved.integrity_errors.get(space) {
            return Err(error.clone());
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

    fn sign_content(&self, content: Vec<u8>, sealed: bool) -> Result<PendingEnvelope, String> {
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
            invite: None,
        };
        pending.envelope()?.verify().map_err(|e| e.to_string())?;
        Ok(pending)
    }

    fn clear_event(
        &self,
        space: &str,
        body: EventBody,
        now_ms: i64,
    ) -> Result<PendingEnvelope, String> {
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

    fn seal_inner(&self, event: &Event, epoch: u64) -> Result<PendingEnvelope, String> {
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
    ) -> Result<Update, String> {
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
                forwarded: false,
            },
            now_ms,
        )?;
        let envelope = inner.envelope()?;
        let event = envelope.plain_event().ok_or("invalid inner message")?;
        let pending = self.seal_inner(&event, epoch)?;
        self.saved.outbox.insert(event.client_id.clone(), pending);
        self.update(Vec::new())
    }

    pub fn bootstrap(&mut self) -> Result<Update, String> {
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

    pub fn queue_join(
        &mut self,
        space: &str,
        role: Role,
        code: &str,
        now_ms: i64,
    ) -> Result<Update, String> {
        if !roda_proto::valid_invite_code(code) || space.is_empty() {
            return Err("Invalid invitation.".into());
        }
        if self.roster(space).contains_key(&self.account.identity) {
            return self.update(Vec::new());
        }
        let mut pending = self.clear_event(
            space,
            EventBody::MemberAdded {
                identity: self.account.identity.clone(),
                role,
            },
            now_ms,
        )?;
        pending.invite = Some(code.to_string());
        let id = pending.envelope()?.client_id().to_string();
        self.saved.outbox.insert(id, pending);
        self.update(Vec::new())
    }

    fn prepare_packages(&mut self, first: bool) -> Result<(), String> {
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

    pub fn receive(&mut self, bytes: &[u8], now_ms: i64) -> Result<Update, String> {
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
            } => return Err(format!("Could not publish encrypted device keys: {error}")),
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
                if let Some(pending) = self.saved.outbox.get_mut(&client_id) {
                    if permanent {
                        pending.failure = Some(reason);
                    } else if let Some(epoch) = pending.epoch {
                        self.saved
                            .blocked_epochs
                            .insert(pending.envelope()?.space().to_string(), epoch);
                    }
                }
                self.sent.remove(&client_id);
            }
            _ => {}
        }
        self.update(extra)
    }

    fn ask_rejoin(&mut self, now_ms: i64) -> Result<(), String> {
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

    fn ingest(&mut self, sequenced: Sequenced, now_ms: i64) -> Result<(), String> {
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
                                Err(roda_mls::MlsError::Storage(error)) => return Err(error),
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
                            Err(roda_mls::MlsError::Storage(error)) => return Err(error),
                            Err(_) => {}
                        }
                    }
                    SealedKind::Commit => {
                        match device.open(&space, ciphertext, &roster, &admitted) {
                            Ok(Opened::Commit { .. }) => landed = true,
                            Ok(_) => {}
                            Err(roda_mls::MlsError::Storage(error)) => return Err(error),
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
        if let EventBody::Checkpoint { epoch, digest, .. } = &event.body {
            if let Ok((ours, expected)) = self.device()?.checkpoint(&space) {
                if ours == *epoch && expected != *digest {
                    self.saved.integrity_errors.insert(space.clone(), "The encrypted conversation has conflicting membership. Open it on your phone.".into());
                }
            }
        }
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

    pub fn update(&mut self, mut frames: Vec<Vec<u8>>) -> Result<Update, String> {
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
        for (id, pending) in self
            .saved
            .outbox
            .iter()
            .filter(|(_, pending)| pending.failure.is_none())
        {
            if self.sent.contains(id) {
                continue;
            }
            if let Some(inner) = &pending.inner {
                if self.ready_epoch(&inner.space).ok() != pending.epoch {
                    continue;
                }
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
        })
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
            let waiting = self.ready_epoch(space).err();
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
