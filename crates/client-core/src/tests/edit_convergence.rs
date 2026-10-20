//! Fixed signed edits converge through the local outbox, own confirmation and remote replay.
use super::*;
use crate::engine::Engine;
use crate::sync::Ingest;
use event_log::{Author, Signer, SpaceLog};
use protocol::{Envelope, Sequenced};
use std::path::PathBuf;
use types::{
    Annotation, Event, EventBody, Identity, IdentityKind, Privacy, ReplyRef, Role, SpaceKind,
};

const SPACE: &str = "sp_edit_convergence";
const ROOT: &str = "c16-root";
const INLINE: &str = "c16-inline";
const CHILD: &str = "c16-child";
const NEIGHBOR: &str = "c16-neighbor";
const AT: i64 = 1_700_000_000_000;
const ROOT_FINAL: &str = "Plano final — sábado às 08:30 🌄";
const CHILD_FINAL: &str = "Levar água 💧\n+ lanche";

struct Database(PathBuf);

impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("edit-convergence"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn replica(path: &str, owner: &Signer, peer: &Signer) -> Engine {
    let mut engine = Engine::open(path).unwrap();
    for (signer, name, secret) in [(owner, "Owner", Some(owner.secret())), (peer, "Peer", None)] {
        engine
            .store
            .put_identity(
                &Identity {
                    id: signer.id(),
                    kind: IdentityKind::Person,
                    name: name.into(),
                    handle: name.to_lowercase(),
                    tint_hex: "#246844".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: "Synthetic edit convergence fixture".into(),
                },
                secret.as_ref(),
            )
            .unwrap();
    }
    engine.store.set_meta("me", &owner.id()).unwrap();
    engine.reload().unwrap();
    engine
}

fn post(id: &str, text: &str, reply: Option<ReplyRef>) -> EventBody {
    EventBody::MessagePosted {
        message: format!("body-{id}"),
        text: text.into(),
        attaches: None,
        reply,
        forwarded: false,
    }
}

fn frame(event: &Event) -> Sequenced {
    Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: Envelope::plain(event),
    }
}

fn text(entry: &TimelineEntry) -> &str {
    match &entry.kind {
        EntryKind::Message { text, card: None } => text,
        other => panic!("expected ordinary text, got {other:?}"),
    }
}

fn assert_edited(entries: &[TimelineEntry]) {
    assert_eq!(
        entries.len(),
        4,
        "edits fold over originals without adding rows"
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [ROOT, INLINE, CHILD, NEIGHBOR]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.seq).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.at_ms).collect::<Vec<_>>(),
        [AT + 2, AT + 3, AT + 4, AT + 5],
        "an edit cannot replace original message timestamps"
    );
    assert_eq!(
        entries.iter().map(text).collect::<Vec<_>>(),
        [
            ROOT_FINAL,
            "Obrigado, combinado.",
            CHILD_FINAL,
            "Mensagem vizinha"
        ]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.edited).collect::<Vec<_>>(),
        [true, false, true, false]
    );
    assert!(entries
        .iter()
        .all(|entry| !entry.deleted && entry.voice.is_none()));
    let quote = entries[1].reply_to.as_ref().unwrap();
    assert_eq!(quote.id, ROOT);
    assert_eq!(
        quote.text, ROOT_FINAL,
        "inline quotes follow the latest target edit"
    );
    assert_eq!(entries[0].thread_replies, 1);
    assert_eq!(entries[2].in_thread.as_deref(), Some(ROOT));
    assert!(entries[2].reply_to.is_none());
}

fn assert_replica(engine: &Engine, expected: &[TimelineEntry]) {
    let entries = engine.timeline(SPACE).unwrap();
    assert_edited(&entries);
    assert_eq!(
        entries, expected,
        "compare the full ordered DTO, not zipped prefixes"
    );
    let thread = engine.thread(SPACE, ROOT).unwrap();
    assert_eq!(thread.len(), 2);
    assert_eq!(
        thread
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [ROOT, CHILD]
    );
    assert_eq!(thread, [expected[0].clone(), expected[2].clone()]);
}

#[test]
fn pending_signed_edits_converge_with_confirmed_remote_history_across_cold_reopen() {
    let database = Database::new();
    let local_path = database.path("local.sqlite");
    let remote_path = database.path("remote.sqlite");
    let owner = Signer::from_secret(&[41; 32]);
    let peer = Signer::from_secret(&[42; 32]);
    let author = Author::root(owner.clone());
    let peer_author = Author::root(peer.clone());
    let mut local = replica(&local_path, &owner, &peer);
    let mut remote = replica(&remote_path, &owner, &peer);
    let mut relay = SpaceLog::new(SPACE);
    for (id, by, body) in [
        (
            "c16-create",
            &author,
            EventBody::SpaceCreated {
                title: "Edit convergence".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "c16-member",
            &author,
            EventBody::MemberAdded {
                identity: peer.id(),
                role: Role::Member,
            },
        ),
        (ROOT, &author, post(ROOT, "Plano original", None)),
        (
            INLINE,
            &author,
            post(
                INLINE,
                "Obrigado, combinado.",
                Some(ReplyRef {
                    to: ROOT.into(),
                    thread: false,
                }),
            ),
        ),
        (
            CHILD,
            &author,
            post(
                CHILD,
                "Levar lanche",
                Some(ReplyRef {
                    to: ROOT.into(),
                    thread: true,
                }),
            ),
        ),
        (
            NEIGHBOR,
            &peer_author,
            post(NEIGHBOR, "Mensagem vizinha", None),
        ),
    ] {
        let signed = by.sign_event(SPACE, id, AT + relay.len() as i64, relay.head(), body);
        let event = relay.sequence(signed);
        assert_eq!(local.ingest(frame(event)), Ingest::Applied);
        assert_eq!(remote.ingest(frame(event)), Ingest::Applied);
    }
    assert_eq!(
        local.timeline(SPACE).unwrap(),
        remote.timeline(SPACE).unwrap()
    );
    let seen = relay.head();
    let mut confirmed_edits = Vec::new();
    for (id, target, edited, offset) in [
        (
            "c16-edit-root-first",
            ROOT,
            "  Plano revisto 🧭\n08:30  ",
            10,
        ),
        ("c16-edit-child", CHILD, "  Levar água 💧\n+ lanche  ", 11),
        (
            "c16-edit-root-final",
            ROOT,
            "  Plano final — sábado às 08:30 🌄  ",
            12,
        ),
    ] {
        let signed = author.sign_event(
            SPACE,
            id,
            AT + offset,
            seen.clone(),
            EventBody::MessageAnnotated {
                target: target.into(),
                note: Annotation::Edit {
                    text: edited.into(),
                },
            },
        );
        local.store.outbox_put(&signed).unwrap();
        confirmed_edits.push(relay.sequence(signed).clone());
    }
    // Stage real durable signed intents; this fixture does not test public edit-command admission.
    local.reload().unwrap();
    assert_eq!(local.store.outbox().unwrap().len(), 3);
    let expected = local.timeline(SPACE).unwrap();
    assert_edited(&expected);
    assert_ne!(remote.timeline(SPACE).unwrap(), expected);
    drop(local);
    let mut local = Engine::open(&local_path).unwrap();
    assert_replica(&local, &expected);
    for event in &confirmed_edits {
        assert_eq!(local.ingest(frame(event)), Ingest::Confirmed);
        assert_replica(&local, &expected);
        assert_eq!(remote.ingest(frame(event)), Ingest::Applied);
    }
    assert_replica(&remote, &expected);
    assert!(local.store.outbox().unwrap().is_empty());
    assert_eq!(local.store.event_count().unwrap(), 9);
    assert_eq!(remote.store.event_count().unwrap(), 9);
    for event in relay.events() {
        assert_eq!(local.ingest(frame(event)), Ingest::Duplicate);
        assert_eq!(remote.ingest(frame(event)), Ingest::Duplicate);
    }
    assert_replica(&local, &expected);
    assert_replica(&remote, &expected);
    drop(local);
    drop(remote);
    for path in [&local_path, &remote_path] {
        let reopened = Engine::open(path).unwrap();
        assert_replica(&reopened, &expected);
        assert_eq!(reopened.store.event_count().unwrap(), 9);
        assert!(reopened.store.outbox().unwrap().is_empty());
        reopened.logs[SPACE].verify().unwrap();
    }
}
