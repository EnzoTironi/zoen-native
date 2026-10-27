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

#[cfg(test)]
use types::TrustLevel;

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use event_log::content::SealedKind;
use event_log::{chain_hash, content_hash_of, Author, Signer, SpaceLog};
use protocol::{normalize_handle, Cursor, Envelope, Sequenced};
use serde::{Deserialize, Serialize};
use types::{
    new_id, new_ulid, Event, EventBody, FileDoc, Identity, IdentityId, IdentityKind, ItemContent,
    ItemKind, PageContent, Privacy, Role, Seen, SpaceId, SpaceKind, GENESIS_PREV,
};

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
    pub(crate) profile_key: Option<&'a event_log::profile::ProfileKey>,
    pub(crate) personal_agent_key: Option<&'a Signer>,
    pub(crate) name: &'a str,
    pub(crate) handle: &'a str,
    pub(crate) relay: &'a str,
    pub(crate) agent: Option<(&'a str, &'a types::AgentOwnerProof)>,
}
pub(crate) const EVENT_FORMAT_META: &str = "event_format";
/// Format v3 (signed protobuf bytes). Older logs were signed over JSON and restart.
pub(crate) const EVENT_FORMAT_VALUE: &str = "3";

/// The author's authenticated intent, before relay sequencing. A client ID alone
/// cannot acknowledge an outbox row, even when another valid member reuses it.
/// The verified log authenticates these bytes, not the hex spelling of a signature.
/// Borrow existing signed bytes; this is a disposable index, not another receipt store.
#[derive(Hash, PartialEq, Eq)]
struct PendingCommitment<'a> {
    space: &'a str,
    client_id: &'a str,
    author: &'a str,
    device: Option<&'a str>,
    content: &'a [u8],
}

impl<'a> PendingCommitment<'a> {
    fn of(event: &'a Event) -> Self {
        Self {
            space: &event.space,
            client_id: &event.client_id,
            author: &event.author,
            device: event.device.as_deref(),
            content: &event.content,
        }
    }
}

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
    /// Successful author checks over exact retained originals; never permission or epoch state.
    outbox_verification: std::sync::Mutex<outbox::VerificationCache>,
    pub(crate) file_demands: crate::file_demand::FileDemands,
    pub account: Option<AccountMeta>,
    /// The unlocked device key (loaded from the vault).
    pub author: Option<Author>,
    pub synced: HashSet<SpaceId>,
    /// `client_id` → Space, for events in the outbox.
    pub pending: HashMap<String, SpaceId>,
    /// Spaces whose pending resolutions overlay their ordered approval state.
    pub(crate) optimistic_approvals: HashSet<SpaceId>,
    /// `client_id` → why the relay refused it.
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
    matches!(
        body,
        EventBody::MessagePosted { .. } | EventBody::AppEditRequested { .. }
    ) || matches!(body, EventBody::ItemCreated {
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

mod account;
mod account_preparation;
mod ingestion;
mod member_controls;
mod member_roles;
mod outbox;
mod owned_member_candidates;
mod prepared_account;
mod profiles;
mod receipts;
mod recovery;
mod rejections;
#[cfg(test)]
pub(crate) mod replay_probe;
mod spaces;

impl Engine {
    /// `OpenMLS` `SenderRatchetConfiguration` default `out_of_order_tolerance`.
    /// A generation this far behind one already opened still decrypts; one step
    /// further is `TooDistantInThePast` and that page never opens on the device.
    const APPLICATION_IN_FLIGHT: usize = 5;
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
        let owner = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
        let original = agent(&owner, Signer::generate().unwrap().id());
        let mut unsigned = original.clone();
        unsigned.owner_proof = None;
        e.put_profiles(vec![unsigned]).unwrap();
        assert!(!e.identities.contains_key(&original.id));
        e.put_profiles(vec![original.clone()]).unwrap();
        let other = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
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
        let agent = Signer::generate().unwrap().id();
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
        let mut legacy = agent(&owner, Signer::generate().unwrap().id());
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
        let other = Author::device(&Signer::generate().unwrap(), Signer::generate().unwrap());
        e.put_profiles(vec![agent(&other, me.clone())]).unwrap();
        assert_eq!(e.identities[&me], original);
    }
}
