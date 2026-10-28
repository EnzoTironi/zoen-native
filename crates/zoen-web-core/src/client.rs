mod sequence;
use sequence::MessageSequence;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use event_log::{
    content::{InnerEvent, Payload, Sealed, SealedKind, SignedContent},
    SpaceLog,
};
use mls::{ExternalDevice, GroupState, Leaf, Opened, SUITE_ID};
use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::signatures::Signer;
use protocol::{ClientFrame, Cursor, Envelope, Op, Reply, Sequenced, ServerFrame};
use serde::{Deserialize, Serialize};
use types::{Event, EventBody, Identity, Privacy, Role, Seen, SpaceKind};

use crate::LinkedAccount;

mod agents;
mod apps;
mod attachments;
mod conversation_view;
mod crypto_outbox;
mod cursor_repair;
mod documents;
mod event_stream;
mod file_outbox;
mod history;
mod integrity;
mod linking;
mod membership;
mod message_view;
mod profile;
mod profile_photo;
mod publication;
mod receipts;
pub use cursor_repair::{CursorRepairConfig, CursorRepairStatus, CursorRepairTick};
pub use documents::DocumentResponse;

const PACKAGES_REQUEST: u64 = 0x8000_0001;
const PROFILES_REQUEST: u64 = 0x8000_0002;

// The host supplies time explicitly: SystemTime is unavailable in browser WASM.
fn browser_id(prefix: &str, now: i64) -> Result<String, String> {
    Ok(format!(
        "{prefix}_{}",
        types::new_ulid(now)
            .map_err(|error| error.to_string())?
            .to_ascii_lowercase()
    ))
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
    linking: linking::Journal,
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

/// Typed activity from an accepted Item version in this conversation's signed log.
#[derive(Clone, Serialize)]
pub(super) struct ItemEditView {
    space_id: String,
    item_id: String,
    title: String,
    version: u32,
    note: String,
    is_undo: bool,
}

type ItemEditKey = (String, String, String, String);
fn item_edit_key(event: &Event) -> ItemEditKey {
    (
        event.space.clone(),
        event.author.clone(),
        event.client_id.clone(),
        event.hash.clone(),
    )
}

#[derive(Serialize)]
pub struct MessageView {
    id: String,
    /// Timeline entry id (event hash) once the relay ordered it: the target of replies,
    /// threads, reactions, edits and deletes.
    entry: Option<String>,
    seq: Option<MessageSequence>,
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
    file: Option<types::FileDoc>,
    media: Option<types::MessageMedia>,
    file_item_id: Option<String>,
    file_version: Option<u32>,
    card: Option<serde_json::Value>,
    item_edit: Option<ItemEditView>,
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
struct ConversationProfile {
    #[serde(flatten)]
    identity: Identity,
    photo_sha256: Option<String>,
    photo_ready: bool,
    photo_mime: Option<String>,
}

#[derive(Serialize)]
pub struct ConversationView {
    id: String,
    title: String,
    kind: &'static str,
    privacy: Privacy,
    roster: BTreeMap<String, Role>,
    profiles: Vec<ConversationProfile>,
    prefs: ChatPreferencesView,
    event_count: String,
    can_send: bool,
    waiting: Option<String>,
    integrity_error: Option<String>,
    unreadable: usize,
    unread: usize,
    mentions: usize,
    members: usize,
    messages: Vec<MessageView>,
}

#[derive(Serialize)]
pub struct Update {
    pub snapshot: String,
    pub frames: Vec<Vec<u8>>,
    pub conversations: Vec<ConversationView>,
    pub maintenance_ms: Option<u32>,
    pub history_ready: bool,
    pub cursor_repair: Option<CursorRepairStatus>,
}

/// The same `OpenMLS` crypto, log verification and binary protocol used by native clients.
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
    /// Unresolved live marks are connection-local, never a durable authority.
    deferred_receipts: BTreeMap<(String, String), (u64, u64)>,
    profile_net: profile::Network,
    membership_net: membership::Network,
    linking_net: linking::Network,
    history_ready: bool,
    attachment_producer: attachments::Producer,
    // Connection-local scheduling only; saved logs/outbox remain the authority.
    cursor_repair: Option<protocol::cursor_repair::CursorRepair<Duration>>,
    repair_now: Duration,
}

impl<S: Signer> Client<S> {
    ///
    /// # Errors
    /// Returns an error for an invalid or unsupported snapshot, unverified log or
    /// outbox events, outbox ownership mismatches, unavailable MLS storage,
    /// or failure to initialize the attachment producer.
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
                event_log::verify_author(inner).map_err(|e| e.to_string())?;
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
            deferred_receipts: BTreeMap::new(),
            profile_net: profile::Network::default(),
            membership_net: membership::Network::default(),
            linking_net: linking::Network::default(),
            history_ready: false,
            attachment_producer: attachments::Producer::new()?,
            cursor_repair: None,
            repair_now: Duration::ZERO,
        };
        client.device()?;
        client.load_linked_profile();
        for event in client
            .logs
            .values()
            .flat_map(event_log::SpaceLog::events)
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
            .flat_map(event_log::SpaceLog::events)
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
    /// offline. Publishing still waits for `SyncDone` and reseals after an epoch
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
}

fn is_post(body: &EventBody) -> bool {
    matches!(body, EventBody::MessagePosted { .. })
        || matches!(body,EventBody::ItemCreated{content:types::ItemContent::File(doc),..} if doc.is_voice())
        || matches!(
            body,
            EventBody::ItemCreated {
                kind: types::ItemKind::Page,
                content: types::ItemContent::Page(_),
                ..
            }
        )
}

fn inner_event(sequenced: &Sequenced, plaintext: &[u8], from: &Leaf) -> Option<Event> {
    let inner = InnerEvent::decode(plaintext)?;
    let event = event_log::event_from_content(
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
