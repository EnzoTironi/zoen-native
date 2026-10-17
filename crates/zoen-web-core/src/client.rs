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

mod agents;
mod apps;
mod documents;
mod history;
mod membership;
mod profile;
pub use documents::DocumentResponse;

const PACKAGES_REQUEST: u64 = 0x8000_0001;
const PROFILES_REQUEST: u64 = 0x8000_0002;

// The host supplies time explicitly: SystemTime is unavailable in browser WASM.
fn browser_id(prefix: &str, now: i64) -> String {
    format!(
        "{prefix}_{}",
        roda_types::new_ulid(now).to_ascii_lowercase()
    )
}

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
    #[serde(default)]
    blob_uploads: Vec<String>,
    #[serde(default)]
    order: Option<u64>,
    #[serde(default)]
    after_commit: Option<String>,
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
    #[serde(default)]
    local_chunks: BTreeMap<String, Vec<u8>>,
    packages: Option<Packages>,
    packages_published: bool,
    joining: BTreeSet<String>,
    blocked_epochs: BTreeMap<String, u64>,
    integrity_errors: BTreeMap<String, String>,
    /// Per Space, each member's (delivered, read) watermarks, as the relay replays them.
    #[serde(default)]
    receipts: BTreeMap<String, BTreeMap<String, (u64, u64)>>,
    #[serde(default)]
    documents: documents::Journal,
    #[serde(default)]
    outbox_sequence: u64,
    #[serde(default)]
    profile: profile::Journal,
    #[serde(default)]
    history: history::Journal,
    #[serde(default)]
    own_handle: Option<String>,
    #[serde(default)]
    membership: membership::Journal,
}

#[derive(Serialize)]
pub struct Reaction {
    emoji: String,
    count: usize,
    mine: bool,
    participants: Vec<ReactionParticipant>,
}

#[derive(Serialize)]
pub struct ReactionParticipant {
    identity: String,
    name: String,
}

#[derive(Serialize)]
pub struct Quote {
    id: String,
    name: String,
    text: String,
}

#[derive(Serialize)]
pub struct MessageView {
    id: String,
    /// Timeline entry id (event hash) once the relay ordered it: the target of replies,
    /// threads, reactions, edits and deletes.
    entry: Option<String>,
    #[serde(serialize_with = "serialize_message_sequence")]
    seq: Option<u64>,
    author: String,
    name: String,
    text: String,
    at_ms: i64,
    delivery: &'static str,
    failure: Option<String>,
    /// For my own ordered messages: "sent", "delivered" or "read" by everyone else.
    ticks: Option<&'static str>,
    edited: bool,
    deleted: bool,
    forwarded: bool,
    reactions: Vec<Reaction>,
    /// Inline reply: the quoted message.
    quote: Option<Quote>,
    /// Thread reply: the root it belongs to (shown only in the thread panel).
    thread: Option<String>,
    replies: usize,
    repliers: Vec<String>,
    last_reply_ms: Option<i64>,
    /// A shared file, photo, video or voice note (the chunks to fetch and open).
    file: Option<roda_types::FileDoc>,
    file_item_id: Option<String>,
    file_version: Option<u32>,
    card: Option<serde_json::Value>,
}

#[derive(Default)]
struct Folded {
    text: Option<String>,
    deleted: bool,
    reactions: BTreeMap<String, String>,
}

#[derive(Serialize)]
pub struct ChatPreferencesView {
    pinned: bool,
    archived: bool,
    // i64::MAX means forever; keep every bit across the JavaScript boundary.
    muted_until_ms: Option<String>,
}

#[derive(Serialize)]
pub struct ConversationView {
    id: String,
    title: String,
    kind: &'static str,
    privacy: Privacy,
    roster: BTreeMap<String, Role>,
    profiles: Vec<Identity>,
    prefs: ChatPreferencesView,
    event_count: String,
    can_send: bool,
    waiting: Option<String>,
    unreadable: usize,
    unread: usize,
    mentions: usize,
    members: usize,
    messages: Vec<MessageView>,
}

fn serialize_message_sequence<S: serde::Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(sequence) => serializer.serialize_some(&sequence.to_string()),
        None => serializer.serialize_none(),
    }
}

#[derive(Serialize)]
pub struct Update {
    pub snapshot: String,
    pub frames: Vec<Vec<u8>>,
    pub conversations: Vec<ConversationView>,
    pub maintenance_ms: Option<u32>,
    pub history_ready: bool,
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
    view_now_ms: i64,
    /// Ephemeral receipt announcements are connection-local, never durable acknowledgements.
    announced_receipts: BTreeMap<String, (u64, u64)>,
    profile_net: profile::Network,
    membership_net: membership::Network,
    history_ready: bool,
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
        let mut client = Self {
            account,
            signer,
            provider,
            saved,
            logs,
            synced: false,
            sent: BTreeSet::new(),
            packages_in_flight: false,
            view_now_ms: 0,
            announced_receipts: BTreeMap::new(),
            profile_net: profile::Network::default(),
            membership_net: membership::Network::default(),
            history_ready: false,
        };
        client.device()?;
        client.load_linked_profile();
        for event in client
            .logs
            .values()
            .flat_map(|log| log.events())
            .cloned()
            .collect::<Vec<_>>()
        {
            client.note_profile_event(&event);
        }
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
        self.queue_epoch(space)
    }

    /// An already provisioned MLS group can seal a local outbox entry while
    /// offline. Publishing still waits for SyncDone and reseals after an epoch
    /// change, so no cached membership state authorizes a network publication.
    fn queue_epoch(&self, space: &str) -> Result<u64, String> {
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
        if self.membership_waiting(space) {
            return Err("Waiting for the encrypted membership update…".into());
        }
        if device.group_state(space).map_err(|e| e.to_string())? != GroupState::Ready {
            return Err(
                "Waiting for your phone to add this browser to the encrypted conversation…".into(),
            );
        }
        let members = device.roster(space).map_err(|e| e.to_string())?;
        if members
            .iter()
            .any(|identity| !roster.contains_key(identity))
            || (matches!(
                roster.get(&self.account.identity),
                Some(Role::Owner | Role::Admin)
            ) && !self.membership_net.stuck.contains(space)
                && members != roster.keys().cloned().collect())
        {
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
            blob_uploads: Vec::new(),
            order: None,
            after_commit: None,
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
        self.queue_post(space, text, None, false, now_ms)
    }

    /// A message, optionally an inline reply (`thread == false`) or a thread reply to
    /// `reply`, or forwarded. Thread replies always point at the root, like native.
    pub fn queue_post(
        &mut self,
        space: &str,
        text: &str,
        reply: Option<(String, bool)>,
        forwarded: bool,
        now_ms: i64,
    ) -> Result<Update, String> {
        let reply = match reply {
            Some((to, thread)) => {
                let target = self
                    .posted(space, &to)
                    .ok_or("That message is no longer available.")?;
                let to = match target {
                    Some(root) if thread && root.thread => root.to,
                    _ => to,
                };
                let to = self
                    .resolve_post(space, &to)
                    .map(|event| event.hash.clone())
                    .unwrap_or(to);
                Some(roda_types::ReplyRef { to, thread })
            }
            None => None,
        };
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Write a message of up to 32 KB.".into());
        }
        self.queue_domain_body(
            space,
            EventBody::MessagePosted {
                message: roda_types::new_secret_id("msg"),
                text: text.into(),
                attaches: None,
                reply,
                forwarded,
            },
            now_ms,
            Vec::new(),
        )?;
        self.update(Vec::new())
    }

    /// A file, photo, video or voice note whose chunks the host already uploaded.
    pub fn queue_file(
        &mut self,
        space: &str,
        doc: roda_types::FileDoc,
        now_ms: i64,
    ) -> Result<Update, String> {
        self.queue_file_inner(space, doc, now_ms)
    }

    /// Explicit signed item/message identities; callers never guess from a filename
    /// or search the latest timeline entry after an asynchronous upload.
    pub fn queue_file_record(
        &mut self,
        space: &str,
        doc: roda_types::FileDoc,
        now_ms: i64,
    ) -> Result<(Update, String, String), String> {
        let before = self.saved.outbox.keys().cloned().collect::<BTreeSet<_>>();
        let update = self.queue_file(space, doc, now_ms)?;
        let (item, message) = self
            .saved
            .outbox
            .iter()
            .filter(|(id, _)| !before.contains(*id))
            .filter_map(|(_, pending)| pending.inner.as_ref())
            .find_map(|event| match &event.body {
                EventBody::ItemCreated {
                    item,
                    content: roda_types::ItemContent::File(_),
                    ..
                } => Some((item.clone(), event.client_id.clone())),
                _ => None,
            })
            .ok_or("The shared file was not queued.")?;
        let message = self
            .saved
            .outbox
            .iter()
            .filter(|(id, _)| !before.contains(*id))
            .filter_map(|(_, pending)| pending.inner.as_ref())
            .find_map(|event| match &event.body {
                EventBody::MessagePosted {
                    attaches: Some(target),
                    ..
                } if *target == item => Some(event.client_id.clone()),
                _ => None,
            })
            .unwrap_or(message);
        Ok((update, item, message))
    }

    /// Persist failed media atomically without ever publishing its incomplete manifest.
    pub fn queue_failed_file(
        &mut self,
        space: &str,
        doc: roda_types::FileDoc,
        now_ms: i64,
        reason: String,
        chunks: BTreeMap<String, Vec<u8>>,
    ) -> Result<Update, String> {
        for chunk in &doc.chunks {
            let sealed = chunks.get(&chunk.blob).ok_or("missing local chunk")?;
            crate::media::open_chunk(chunk, sealed)?;
        }
        if doc.chunks.is_empty() {
            return Err("file empty or too large".into());
        }
        // Failed media is local-only: retain it even while disconnected or before
        // MLS catches up. It must never need encryption or become publishable.
        let mut pending = self.clear_event(
            space,
            EventBody::ItemCreated {
                item: roda_types::new_secret_id("it"),
                kind: roda_types::ItemKind::File,
                content: roda_types::ItemContent::File(doc),
                origin: "Adicionado".into(),
            },
            now_ms,
        )?;
        let event = pending
            .envelope()?
            .plain_event()
            .ok_or("invalid inner message")?;
        pending.inner = Some(event.clone());
        pending.failure = Some(reason);
        self.saved.local_chunks.extend(chunks);
        self.saved.outbox.insert(event.client_id.clone(), pending);
        self.update(Vec::new())
    }

    pub fn local_chunk(&self, blob: &str) -> Vec<u8> {
        self.saved
            .local_chunks
            .get(blob)
            .cloned()
            .unwrap_or_default()
    }

    fn queue_file_inner(
        &mut self,
        space: &str,
        mut doc: roda_types::FileDoc,
        now_ms: i64,
    ) -> Result<Update, String> {
        if doc.chunks.is_empty() {
            return Err("file empty or too large".into());
        }
        if let Some(reply) = doc.voice.as_mut().and_then(|voice| voice.reply.as_mut()) {
            let reference = match self.posted(space, &reply.to) {
                Some(Some(root)) if reply.thread && root.thread => root.to,
                Some(_) => reply.to.clone(),
                None => return Err("That message is no longer available.".into()),
            };
            reply.to = self
                .resolve_post(space, &reference)
                .ok_or("That message is no longer available.")?
                .client_id
                .clone();
        }
        let item = roda_types::new_secret_id("it");
        let voice = doc.is_voice();
        let mut bodies = vec![EventBody::ItemCreated {
            item: item.clone(),
            kind: roda_types::ItemKind::File,
            content: roda_types::ItemContent::File(doc),
            origin: "Adicionado".into(),
        }];
        if !voice {
            bodies.push(EventBody::MessagePosted {
                message: format!("msg_attachment_{item}"),
                text: String::new(),
                attaches: Some(item),
                reply: None,
                forwarded: false,
            });
        }
        self.queue_domain_batch(space, bodies, now_ms)?;
        self.update(Vec::new())
    }

    /// Edit, delete for everyone, react or unreact. Same encrypted path as messages.
    /// Keep a queued file/voice in the timeline as failed so the clip is not lost.
    pub fn fail_outbox(
        &mut self,
        client_id: &str,
        reason: impl Into<String>,
    ) -> Result<Update, String> {
        if let Some(pending) = self.saved.outbox.get_mut(client_id) {
            pending.failure = Some(reason.into());
            self.sent.remove(client_id);
        }
        self.update(Vec::new())
    }

    pub fn queue_annotation(
        &mut self,
        space: &str,
        target: &str,
        note: roda_types::Annotation,
        now_ms: i64,
    ) -> Result<Update, String> {
        let target = self
            .resolve_post(space, target)
            .map(|event| event.hash.clone())
            .ok_or("That message is no longer available.")?;
        let author = self
            .logs
            .get(space)
            .and_then(|log| log.events().iter().find(|e| e.hash == target))
            .filter(|e| is_post(&e.body))
            .map(|e| e.author.clone())
            .ok_or("That message is no longer available.")?;
        if self.logs.get(space).is_some_and(|log| log.events().iter().any(|event| matches!(
            &event.body, EventBody::MessageAnnotated { target: to, note: roda_types::Annotation::Delete }
                if self.resolve_post(space, to).is_some_and(|post| post.hash == target) && event.author == author
        ))) {
            return Err("That message is no longer available.".into());
        }
        if let roda_types::Annotation::React { emoji } = &note {
            if emoji.trim().is_empty() || emoji.len() > 32 {
                return Err("Choose an emoji of up to 32 bytes.".into());
            }
        }
        if note.author_only() && author != self.account.identity {
            return Err("Only the person who sent this message can change it.".into());
        }
        // Deletion is terminal in the reader fold. Do not report a successful
        // author mutation that cannot take effect, including behind a queued delete.
        if note.author_only() {
            let deletes_target = |event: &Event| {
                event.author == author
                    && matches!(&event.body, EventBody::MessageAnnotated {
                        target: deleted, note: roda_types::Annotation::Delete,
                    } if self.resolve_post(space, deleted).is_some_and(|post| post.hash == target))
            };
            let deleted = self
                .logs
                .get(space)
                .is_some_and(|log| log.events().iter().any(deletes_target))
                || self
                    .saved
                    .outbox
                    .values()
                    .filter(|pending| pending.failure.is_none())
                    .any(|pending| {
                        pending
                            .inner
                            .as_ref()
                            .is_some_and(|event| event.space == space && deletes_target(event))
                    });
            if deleted {
                return Err("Essa mensagem foi apagada.".into());
            }
        }
        if let roda_types::Annotation::Edit { text } = &note {
            if text.trim().is_empty() || text.len() > 32_768 {
                return Err("Write a message of up to 32 KB.".into());
            }
        }
        self.queue_domain_body(
            space,
            EventBody::MessageAnnotated { target, note },
            now_ms,
            Vec::new(),
        )?;
        self.update(Vec::new())
    }

    /// I have read this conversation up to its newest event: tell my devices and the
    /// other members (one watermark, like native).
    pub fn mark_read(&mut self, space: &str) -> Result<Update, String> {
        let Some(last) = self
            .logs
            .get(space)
            .and_then(|log| log.next_seq().checked_sub(1))
        else {
            return self.update(Vec::new());
        };
        let me = self.account.identity.clone();
        let mine = self
            .saved
            .receipts
            .entry(space.into())
            .or_default()
            .entry(me)
            .or_default();
        *mine = (mine.0.max(last), mine.1.max(last));
        self.update(vec![ClientFrame::Ephemeral {
            space: space.into(),
            kind: roda_proto::EphemeralKind::Read { seq: last },
        }
        .encode()])
    }

    /// Delivered watermarks for every Space whose newest event this device now holds.
    fn delivered(&mut self) -> Vec<Vec<u8>> {
        let me = self.account.identity.clone();
        let mut frames = Vec::new();
        for (space, log) in &self.logs {
            let Some(last) = log.next_seq().checked_sub(1) else {
                continue;
            };
            let mine = self
                .saved
                .receipts
                .entry(space.clone())
                .or_default()
                .entry(me.clone())
                .or_default();
            mine.0 = mine.0.max(last);
            let announced = self.announced_receipts.get(space).copied();
            if announced.is_none_or(|mark| mark.0 < mine.0) {
                frames.push(
                    ClientFrame::Ephemeral {
                        space: space.clone(),
                        kind: roda_proto::EphemeralKind::Delivered { seq: mine.0 },
                    }
                    .encode(),
                );
            }
            if mine.1 > 0 && announced.is_none_or(|mark| mark.1 < mine.1) {
                frames.push(
                    ClientFrame::Ephemeral {
                        space: space.clone(),
                        kind: roda_proto::EphemeralKind::Read { seq: mine.1 },
                    }
                    .encode(),
                );
            }
            self.announced_receipts.insert(space.clone(), *mine);
        }
        frames
    }

    // Hashes are authoritative. An untrusted client id must never shadow one.
    fn resolve_post(&self, space: &str, reference: &str) -> Option<&Event> {
        let events = self.logs.get(space)?.events();
        events
            .iter()
            .find(|e| e.hash == reference)
            .or_else(|| events.iter().find(|e| e.client_id == reference))
            .filter(|e| is_post(&e.body))
    }

    /// The reply link of the message with this entry id in `space`, if it is a message.
    fn posted(&self, space: &str, entry: &str) -> Option<Option<roda_types::ReplyRef>> {
        self.logs
            .get(space)?
            .events()
            .iter()
            .find(|e| e.hash == entry)
            .or_else(|| self.resolve_post(space, entry))
            .and_then(|e| match &e.body {
                EventBody::MessagePosted { reply, .. } => Some(reply.clone()),
                EventBody::ItemCreated {
                    content: roda_types::ItemContent::File(doc),
                    ..
                } => Some(doc.voice.as_ref().and_then(|v| v.reply.clone())),
                _ => None,
            })
    }

    pub fn bootstrap(&mut self) -> Result<Update, String> {
        self.synced = false;
        self.sent.clear();
        self.announced_receipts.clear();
        self.packages_in_flight = false;
        self.profile_net = profile::Network::default();
        self.membership_net.restart();
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

    fn put_directory_profiles(&mut self, profiles: Vec<Identity>) {
        for mut profile in profiles {
            if profile.id == self.account.identity {
                continue;
            }
            if roda_log::parse_device_key(&profile.id).is_none()
                || !roda_log::agent_owner::profile_authorized(&profile)
            {
                continue;
            }
            if self.saved.profiles.get(&profile.id).is_some_and(|known| {
                known.owner_proof.is_some()
                    && roda_log::agent_owner::profile_authorized(known)
                    && (known.kind != profile.kind
                        || known.owner != profile.owner
                        || known.owner_proof != profile.owner_proof)
            }) {
                continue;
            }
            // Directory entries never reveal a person's private fields. Those are
            // overlaid only after the shared encrypted-profile codec authenticates them.
            if profile.kind == roda_types::IdentityKind::Person {
                profile.name.clear();
                profile.bio.clear();
            }
            self.saved.profiles.insert(profile.id.clone(), profile);
        }
        self.overlay_saved_profiles();
    }

    pub fn directory_profiles(&mut self, encoded: &str) -> Result<DocumentResponse, String> {
        if encoded.len() > 1 << 20 {
            return Err("Resposta de perfis grande demais.".into());
        }
        let profiles: Vec<Identity> =
            serde_json::from_str(encoded).map_err(|error| error.to_string())?;
        if profiles.len() > 500 {
            return Err("Perfis demais em uma consulta.".into());
        }
        let ids = profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect::<Vec<_>>();
        self.put_directory_profiles(profiles);
        let result = serde_json::Value::Array(
            ids.iter()
                .filter(|id| **id == self.account.identity || self.saved.profiles.contains_key(*id))
                .map(|id| self.persona_value(id))
                .collect(),
        );
        Ok(DocumentResponse {
            result,
            update: Some(self.update(Vec::new())?),
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }

    pub fn typing_frame(&self, space: &str, typing: bool) -> Result<Vec<u8>, String> {
        if !self.synced
            || !self
                .roster(space)
                .get(&self.account.identity)
                .is_some_and(|role| *role != Role::Reader)
        {
            return Ok(Vec::new());
        }
        Ok(ClientFrame::Ephemeral {
            space: space.into(),
            kind: if typing {
                roda_proto::EphemeralKind::Typing
            } else {
                roda_proto::EphemeralKind::StoppedTyping
            },
        }
        .encode())
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
        self.view_now_ms = now_ms;
        let mut extra = Vec::new();
        match ServerFrame::decode(bytes).map_err(|e| e.to_string())? {
            ServerFrame::Res { id, result } if self.membership_reply(id, &result)? => {}
            ServerFrame::Res { id, result } if self.profile_reply(id, &result)? => {}
            ServerFrame::DeviceMessage { from, to, sealed } => {
                self.history_ready |= self.history_hint(&from, &to, &sealed);
            }
            ServerFrame::ProfileChanged { identity, .. } => {
                self.profile_changed(&identity);
            }
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
                let author = ev.env.author().to_string();
                self.ingest(ev, now_ms)?;
                if self.synced {
                    extra.extend(self.delivered());
                    if !self.saved.profiles.contains_key(&author) {
                        extra.push(
                            ClientFrame::Req {
                                id: PROFILES_REQUEST,
                                op: Op::Profiles { ids: vec![author] },
                            }
                            .encode(),
                        );
                    }
                }
            }
            ServerFrame::Ephemeral { space, from, kind } => {
                let mark = match kind {
                    roda_proto::EphemeralKind::Read { seq } => Some((seq, seq)),
                    roda_proto::EphemeralKind::Delivered { seq } => Some((seq, 0)),
                    _ => None,
                };
                if let Some((delivered, read)) = mark {
                    let entry = self
                        .saved
                        .receipts
                        .entry(space)
                        .or_default()
                        .entry(from)
                        .or_default();
                    *entry = (entry.0.max(delivered), entry.1.max(read));
                }
            }
            ServerFrame::SyncDone => {
                self.synced = true;
                extra.extend(self.delivered());
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
                self.put_directory_profiles(profiles);
            }
            ServerFrame::Rejected {
                client_id,
                reason,
                permanent,
                ..
            } => {
                if permanent && self.membership_refused(&client_id)? {
                    return self.update(extra);
                }
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
        self.note_profile_event(&event);
        self.note_membership_event(&event)?;
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

    pub fn maintenance(&mut self, now: i64) -> Result<Update, String> {
        self.view_now_ms = now;
        self.update(Vec::new())
    }

    pub fn update(&mut self, mut frames: Vec<Vec<u8>>) -> Result<Update, String> {
        self.profile_frames(&mut frames)?;
        self.membership_frames(&mut frames)?;
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
            let mut pending = self.seal_inner(&event, epoch)?;
            pending.blob_uploads = self
                .saved
                .outbox
                .get(&id)
                .map(|pending| pending.blob_uploads.clone())
                .unwrap_or_default();
            pending.order = self.saved.outbox.get(&id).and_then(|pending| pending.order);
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
        let mut outgoing = self
            .saved
            .outbox
            .iter()
            .filter(|(_, pending)| pending.failure.is_none() && pending.blob_uploads.is_empty())
            .filter(|(_, pending)| {
                pending
                    .after_commit
                    .as_ref()
                    .is_none_or(|id| !self.saved.outbox.contains_key(id))
            })
            .collect::<Vec<_>>();
        outgoing.sort_by_key(|(id, pending)| (pending.order.unwrap_or(0), *id));
        for (id, pending) in outgoing {
            if self.sent.contains(id)
                || (!self.synced
                    && matches!(
                        pending.envelope()?.body(),
                        Some(EventBody::ProfileKeyShared { .. })
                    ))
            {
                continue;
            }
            if let Some(inner) = &pending.inner {
                let encrypted = self
                    .metadata(&inner.space)
                    .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd);
                if !self.synced
                    || encrypted
                        && (pending.epoch.is_none()
                            || self.ready_epoch(&inner.space).ok() != pending.epoch)
                {
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
            history_ready: std::mem::take(&mut self.history_ready),
            conversations: self.conversations(),
            maintenance_ms: self
                .membership_deadline()
                .into_iter()
                .chain(self.profile_deadline())
                .min()
                .map(|deadline| deadline.saturating_sub(self.view_now_ms).clamp(50, 60_000) as u32),
        })
    }

    // Shared ownership-checked manifest fold for cards and attachment posts.
    fn folded_files<'a>(
        &'a self,
        space: &str,
    ) -> BTreeMap<&'a str, (&'a str, Vec<&'a roda_types::FileDoc>)> {
        let events = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(|log| log.events().iter())
            .chain(
                self.saved
                    .outbox
                    .values()
                    .filter_map(|pending| pending.inner.as_ref())
                    .filter(|event| event.space == space),
            );
        let mut files: BTreeMap<&str, (&str, Vec<&roda_types::FileDoc>)> = BTreeMap::new();
        for event in events {
            match &event.body {
                EventBody::ItemCreated {
                    item,
                    content: roda_types::ItemContent::File(doc),
                    ..
                } => {
                    files
                        .entry(item)
                        .or_insert((event.author.as_str(), vec![doc]));
                }
                EventBody::ItemVersioned {
                    item,
                    content: roda_types::ItemContent::File(doc),
                    ..
                } => {
                    if let Some((author, versions)) = files.get_mut(item.as_str()) {
                        // Like native, any voice version keeps the original author's ownership.
                        if *author == event.author
                            || !versions
                                .iter()
                                .copied()
                                .chain([doc])
                                .any(roda_types::FileDoc::is_voice)
                        {
                            versions.push(doc);
                        }
                    }
                }
                EventBody::ItemReverted {
                    item, to_version, ..
                } => {
                    if let Some((author, versions)) = files.get_mut(item.as_str()) {
                        if *author != event.author && versions.iter().any(|doc| doc.is_voice()) {
                            continue;
                        }
                        if let Some(doc) = to_version
                            .checked_sub(1)
                            .and_then(|v| versions.get(v as usize))
                            .copied()
                        {
                            versions.push(doc);
                        }
                    }
                }
                _ => {}
            }
        }
        files
    }

    pub fn conversations(&self) -> Vec<ConversationView> {
        let preferences = self.chat_preferences();
        let mut conversations = self.logs.keys().filter_map(|space| {
            let (title, kind, privacy) = self.metadata(space)?;
            let roster = self.roster(space);
            if kind == SpaceKind::Personal || !roster.contains_key(&self.account.identity) { return None; }
            let events = self.logs.get(space)?.events();
            let me = &self.account.identity;
            let files = self.folded_files(space);
            // A direct chat is named after the other person, like every messenger.
            let title = match roster.keys().find(|m| *m != me) {
                Some(other) if kind == SpaceKind::Direct => self.saved.profiles.get(other)
                    .map(|p| if p.name.is_empty() { format!("@{}", p.handle) } else { p.name.clone() })
                    .unwrap_or_else(|| if title.is_empty() { "Conversa".into() } else { title.clone() }),
                _ => title,
            };
            // Fold annotations over their targets. Readers enforce authorship.
            let mut authors: BTreeMap<&str, (&str, &str)> = events.iter().filter(|e| is_post(&e.body))
                .map(|e| (e.hash.as_str(), (e.author.as_str(), e.hash.as_str()))).collect();
            for event in events.iter().filter(|e| is_post(&e.body)) {
                authors.entry(event.client_id.as_str()).or_insert((event.author.as_str(), event.hash.as_str()));
            }
            let mut folded: BTreeMap<String, Folded> = BTreeMap::new();
            for event in events {
                let EventBody::MessageAnnotated { target, note } = &event.body else { continue };
                let Some((author, hash)) = authors.get(target.as_str()) else { continue };
                if note.author_only() && *author != event.author { continue; }
                let f = folded.entry((*hash).to_owned()).or_default();
                match note {
                    roda_types::Annotation::Edit { text } if !f.deleted => f.text = Some(text.clone()),
                    roda_types::Annotation::Delete => { f.deleted = true; f.text = None; f.reactions.clear(); }
                    roda_types::Annotation::React { emoji } if !f.deleted && !emoji.trim().is_empty() && emoji.len() <= 32 => { f.reactions.insert(event.author.clone(), emoji.clone()); }
                    roda_types::Annotation::Unreact => { f.reactions.remove(&event.author); }
                    _ => {}
                }
            }
            let receipts = self.saved.receipts.get(space);
            let others: Vec<&String> = roster.keys().filter(|m| *m != me).collect();
            let watermark = |pick: fn(&(u64, u64)) -> u64| -> u64 {
                others.iter().map(|m| receipts.and_then(|r| r.get(*m)).map(pick).unwrap_or(0)).min().unwrap_or(0)
            };
            let (all_delivered, all_read) = (watermark(|r| r.0.max(r.1)), watermark(|r| r.1));
            let my_read = receipts.and_then(|r| r.get(me)).map(|r| r.1).unwrap_or(0);
            let mut messages = events.iter().filter_map(|event| {
                let mut view = self.message_view(event, "confirmed", None)?;
                view.entry = Some(event.hash.clone());
                view.seq = Some(event.seq);
                if let EventBody::ItemCreated { item, .. } = &event.body {
                    if let Some((_, versions)) = files.get(item.as_str()) {
                        view.file = versions.last().map(|doc| (*doc).clone());
                        view.file_version = u32::try_from(versions.len()).ok();
                        view.edited = versions.len() > 1;
                    }
                }
                if event.author == *me {
                    view.ticks = Some(if others.is_empty() || event.seq > all_delivered { "sent" } else if event.seq > all_read { "delivered" } else { "read" });
                }
                if let Some(f) = folded.get(&event.hash) {
                    view.deleted = f.deleted;
                    if f.deleted { view.text.clear(); view.file = None; view.file_item_id = None; view.file_version = None; view.card = None; }
                    if let Some(text) = &f.text { view.text = text.clone(); view.edited = true; }
                    let mut groups: BTreeMap<&str, Vec<ReactionParticipant>> = BTreeMap::new();
                    for (who, emoji) in &f.reactions {
                        let name = if who == me { self.account.name.clone() } else {
                            self.saved.profiles.get(who).map(|profile| {
                                if profile.name.is_empty() { format!("@{}", profile.handle) } else { profile.name.clone() }
                            }).unwrap_or_else(|| "Alguém".into())
                        };
                        groups.entry(emoji.as_str()).or_default().push(ReactionParticipant { identity: who.clone(), name });
                    }
                    view.reactions = groups.into_iter().map(|(emoji, participants)| Reaction {
                        emoji: emoji.into(), count: participants.len(),
                        mine: participants.iter().any(|participant| participant.identity == *me), participants,
                    }).collect();
                }
                Some(view)
            }).collect::<Vec<_>>();
            messages.extend(self.saved.outbox.values().filter_map(|pending| {
                let inner = pending.inner.as_ref()?;
                if inner.space != *space { return None; }
                self.message_view(inner, if pending.failure.is_some() { "failed" } else { "sending" }, pending.failure.clone())
            }));
            // Native references may use client ids. Normalize before folding replies,
            // preserving real hash targets if an alias collides.
            for message in &mut messages {
                if let Some(quote) = &mut message.quote {
                    if let Some((_, hash)) = authors.get(quote.id.as_str()) { quote.id = (*hash).into(); }
                }
                if let Some(thread) = &mut message.thread {
                    if let Some((_, hash)) = authors.get(thread.as_str()) { *thread = (*hash).into(); }
                }
            }
            // Quotes and thread summaries resolve against the folded views.
            let by_entry: BTreeMap<String, (String, String, i64, String)> = messages.iter().filter_map(|m| m.entry.clone().map(|e| (e, (m.name.clone(), if m.deleted { String::new() } else { m.text.clone() }, m.at_ms, m.author.clone())))).collect();
            let mut threads: BTreeMap<String, (usize, Vec<String>, i64)> = BTreeMap::new();
            for m in &messages {
                if let Some(root) = &m.thread {
                    let t = threads.entry(root.clone()).or_default();
                    t.0 += 1; t.2 = t.2.max(m.at_ms);
                    if !t.1.contains(&m.name) { t.1.push(m.name.clone()); }
                }
            }
            for m in &mut messages {
                if let Some(Quote { id, name, text }) = &mut m.quote {
                    if let Some((n, t, _, _)) = by_entry.get(id) { *name = n.clone(); *text = t.clone(); }
                }
                if let Some(entry) = &m.entry {
                    if let Some((count, who, last)) = threads.get(entry) {
                        m.replies = *count; m.repliers = who.iter().take(3).cloned().collect(); m.last_reply_ms = Some(*last);
                    }
                }
            }
            let mention = format!("@{}", self.account.name.to_lowercase());
            let incoming = messages.iter().filter(|m| m.author != *me && !m.deleted && m.seq.is_some_and(|s| s > my_read) && (my_read > 0 || m.seq.is_some()));
            let (unread, mentions) = incoming.fold((0, 0), |(u, n), m| (u + 1, n + usize::from(!self.account.name.is_empty() && m.text.to_lowercase().contains(&mention))));
            let waiting = self.document_writer(space).err();
            let unreadable = events.iter().filter(|event| matches!(&event.body, EventBody::Sealed { kind } if kind == "Application")).count();
            let prefs = preferences.get(space).cloned().unwrap_or_default();
            let mut people: BTreeSet<_> = roster.keys().cloned().chain(events.iter().map(|event| event.author.clone())).collect();
            let owners: Vec<_> = people.iter().filter_map(|id| self.saved.profiles.get(id)?.owner.clone()).collect();
            people.extend(owners);
            let profiles = people.iter().filter_map(|id| self.saved.profiles.get(id).cloned()).collect();
            Some(ConversationView { id: space.clone(), title, kind: match kind { SpaceKind::Direct=>"direct",SpaceKind::Group=>"group",SpaceKind::Community=>"community",SpaceKind::Personal=>"personal" },
                privacy, profiles, prefs: ChatPreferencesView { pinned: prefs.pinned, archived: prefs.archived, muted_until_ms: prefs.muted_until_ms.map(|time| time.to_string()) },
                event_count: self.logs.get(space)?.next_seq().to_string(),
                can_send: waiting.is_none(), waiting, unreadable, unread, mentions, members: roster.len(), roster, messages })
        }).collect::<Vec<_>>();
        conversations.extend(self.creating_views());
        conversations
    }

    /// Preferences belong to my own personal log. A chat participant cannot set
    /// my preferences by posting the same body in another Space.
    fn chat_preferences(&self) -> BTreeMap<String, roda_types::ChatPrefs> {
        let mut preferences = BTreeMap::new();
        for (space, log) in &self.logs {
            if !matches!(self.metadata(space), Some((_, SpaceKind::Personal, _)))
                || !log
                    .events()
                    .first()
                    .is_some_and(|event| event.author == self.account.identity)
            {
                continue;
            }
            for event in log.events() {
                if event.author != self.account.identity {
                    continue;
                }
                if let EventBody::ChatPrefsSet { space, prefs } = &event.body {
                    preferences.insert(space.clone(), prefs.clone());
                }
            }
        }
        preferences
    }

    fn message_view(
        &self,
        event: &Event,
        delivery: &'static str,
        failure: Option<String>,
    ) -> Option<MessageView> {
        let (text, reply, forwarded, file) = match &event.body {
            EventBody::MessagePosted {
                text,
                reply,
                forwarded,
                attaches,
                ..
            } => {
                // Resolve only from this conversation's readable events, never the
                // global media cache (which may contain another space's files).
                let files = self.folded_files(&event.space);
                let file = attaches
                    .as_ref()
                    .and_then(|item| files.get(item.as_str()))
                    .and_then(|(_, versions)| versions.last())
                    .map(|doc| (*doc).clone());
                (text.clone(), reply.clone(), *forwarded, file)
            }
            EventBody::ItemCreated {
                content: roda_types::ItemContent::File(doc),
                ..
            } if doc.is_voice() || delivery == "failed" => (
                String::new(),
                doc.voice.as_ref().and_then(|v| v.reply.clone()),
                false,
                Some(doc.clone()),
            ),
            _ => return None,
        };
        let name = if event.author == self.account.identity {
            self.account.name.clone()
        } else {
            self.saved
                .profiles
                .get(&event.author)
                .map(|profile| {
                    if profile.name.is_empty() {
                        format!("@{}", profile.handle)
                    } else {
                        profile.name.clone()
                    }
                })
                .unwrap_or_else(|| "Alguém".into())
        };
        let file_item_id = match &event.body {
            EventBody::MessagePosted { attaches, .. } => attaches.clone(),
            EventBody::ItemCreated {
                item,
                content: roda_types::ItemContent::File(_),
                ..
            } => Some(item.clone()),
            _ => None,
        }
        .filter(|_| file.is_some());
        let file_version = file_item_id.as_ref().and_then(|item| {
            self.folded_files(&event.space)
                .get(item.as_str())
                .and_then(|(_, versions)| u32::try_from(versions.len()).ok())
        });
        let card = match &event.body {
            EventBody::MessagePosted {
                attaches: Some(item),
                ..
            } if file.is_none() => self.item_card_value(&event.space, item).ok().flatten(),
            _ => None,
        };
        Some(MessageView {
            id: event.client_id.clone(),
            entry: None,
            seq: None,
            author: event.author.clone(),
            name,
            text,
            at_ms: event.at_ms,
            delivery,
            failure,
            ticks: None,
            edited: false,
            deleted: false,
            forwarded,
            reactions: Vec::new(),
            quote: reply.as_ref().filter(|r| !r.thread).map(|r| Quote {
                id: r.to.clone(),
                name: String::new(),
                text: String::new(),
            }),
            thread: reply.as_ref().filter(|r| r.thread).map(|r| r.to.clone()),
            replies: 0,
            repliers: Vec::new(),
            last_reply_ms: None,
            file,
            file_item_id,
            file_version,
            card,
        })
    }
}

fn is_post(body: &EventBody) -> bool {
    matches!(body, EventBody::MessagePosted { .. })
        || matches!(body,EventBody::ItemCreated{content:roda_types::ItemContent::File(doc),..} if doc.is_voice())
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
