use super::*;
use crate::sync::Ingest;
use event_log::{Author, Signer, SpaceLog};
use protocol::{Envelope, Sequenced};
use types::{Identity, IdentityKind, Privacy, ReplyRef, Role, SpaceKind};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const SPACE: &str = "quote-proof-space";
const SOURCE: &str = "😀 one **two** three";
fn replica(path: &str, owner: &Signer) -> TestResult<Engine> {
    let mut engine = Engine::open(path)?;
    engine.store.put_identity(
        &Identity {
            id: owner.id(),
            kind: IdentityKind::Person,
            name: "Owner".into(),
            handle: "owner".into(),
            tint_hex: "#246844".into(),
            glyph: None,
            owner: None,
            owner_proof: None,
            bio: String::new(),
        },
        Some(&owner.secret()),
    )?;
    engine.store.set_meta("me", &owner.id())?;
    engine.reload()?;
    Ok(engine)
}
fn post(text: &str, reply: Option<ReplyRef>) -> EventBody {
    EventBody::MessagePosted {
        message: "body".into(),
        text: text.into(),
        attaches: None,
        reply,
        media: None,
        forwarded: false,
    }
}
fn append(log: &mut SpaceLog, by: &Author, id: &str, body: EventBody) -> TestResult<Event> {
    let signed = by.sign_event(
        SPACE,
        id,
        1_700_000_000_000 + i64::try_from(log.len())?,
        log.head(),
        body,
    )?;
    Ok(log.sequence(signed)?.clone())
}
fn receive(engine: &mut Engine, event: &Event) -> TestResult<Ingest> {
    Ok(engine.ingest(Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: Envelope::plain(event)?,
    }))
}
fn quote(source: &str, revision: &str) -> ReplySelection {
    ReplySelection {
        source_id: source.into(),
        revision: revision.into(),
        spans: vec![
            QuoteSpan {
                start_utf16: 3,
                end_utf16: 7,
            },
            QuoteSpan {
                start_utf16: 9,
                end_utf16: 12,
            },
            QuoteSpan {
                start_utf16: 14,
                end_utf16: 20,
            },
        ],
        text: "one two three".into(),
    }
}
fn quote_for(log: &SpaceLog, id: &str) -> TestResult<ReplySelection> {
    let event = log
        .events()
        .iter()
        .find(|e| logical_id(e) == id)
        .ok_or("source event")?;
    Ok(quote(id, &event_log::content_hash_of(event)))
}
fn base(log: &mut SpaceLog, owner: &Author) -> TestResult {
    append(
        log,
        owner,
        "create",
        EventBody::SpaceCreated {
            title: "Quote proof".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?;
    append(log, owner, "root", post(SOURCE, None))?;
    Ok(())
}

#[test]
fn independent_receive_replay_keeps_historical_excerpt_after_reordered_edit_and_delete(
) -> TestResult {
    let owner = Signer::from_secret(&[47; 32]);
    let author = Author::root(owner.clone());
    let mut log = SpaceLog::new(SPACE);
    base(&mut log, &author)?;
    append(
        &mut log,
        &author,
        "edit-before-reply",
        EventBody::MessageAnnotated {
            target: "root".into(),
            note: types::Annotation::Edit {
                text: "Changed before delayed reply arrives".into(),
            },
        },
    )?;
    let mut reference = ReplyRef::inline("root");
    reference.quote = Some(Box::new(quote_for(&log, "root")?.signed()));
    append(&mut log, &author, "quoted", post("Answer", Some(reference)))?;
    append(
        &mut log,
        &author,
        "delete-after-reply",
        EventBody::MessageAnnotated {
            target: "root".into(),
            note: types::Annotation::Delete,
        },
    )?;
    let directory = std::env::temp_dir().join(types::new_id("selected-replay")?);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("receiver.sqlite");
    let pathstr = path.to_str().ok_or("path")?;
    let mut receiver = replica(pathstr, &owner)?;
    for event in log.events() {
        assert_eq!(receive(&mut receiver, event)?, Ingest::Applied);
    }
    let snapshot = receiver.timeline(SPACE)?;
    let quoted = snapshot
        .iter()
        .find(|e| e.id == "quoted")
        .ok_or("reply missing")?;
    let selected = quoted.reply_to.as_ref().ok_or("selected missing")?;
    assert_eq!(selected.text, "one two three");
    assert_eq!(selected.selection, Some(quote_for(&log, "root")?));
    for event in log.events() {
        assert_eq!(receive(&mut receiver, event)?, Ingest::Duplicate);
    }
    assert_eq!(receiver.timeline(SPACE)?, snapshot);
    drop(receiver);
    let reopened = replica(pathstr, &owner)?;
    assert_eq!(reopened.timeline(SPACE)?, snapshot);
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn thread_child_quote_keeps_exact_source_and_root_and_refuses_other_root() -> TestResult {
    let owner = Signer::from_secret(&[48; 32]);
    let author = Author::root(owner.clone());
    let mut log = SpaceLog::new(SPACE);
    base(&mut log, &author)?;
    append(
        &mut log,
        &author,
        "child",
        post(SOURCE, Some(ReplyRef::thread("root"))),
    )?;
    let mut sender = replica(":memory:", &owner)?;
    for event in log.events() {
        receive(&mut sender, event)?;
    }
    let selected = quote_for(&log, "child")?;
    let sent =
        sender.reply_selected(SPACE, &owner.id(), "Answer", "child", true, Some(&selected))?;
    assert_eq!(sent.in_thread.as_deref(), Some("root"));
    let face = sent.reply_to.as_ref().ok_or("quote")?;
    assert_eq!(face.id, "child");
    assert_eq!(face.selection, Some(selected.clone()));
    let pending = sender
        .store
        .outbox()?
        .into_iter()
        .find(|e| e.event.client_id == sent.id)
        .ok_or("queued reply")?;
    let confirmed = log.sequence(pending.event)?.clone();
    assert_eq!(receive(&mut sender, &confirmed)?, Ingest::Confirmed);
    let mut receiver = replica(":memory:", &owner)?;
    for event in log.events() {
        assert_eq!(receive(&mut receiver, event)?, Ingest::Applied);
    }
    let visible = receiver
        .timeline(SPACE)?
        .into_iter()
        .find(|e| e.id == sent.id)
        .ok_or("received reply")?;
    assert_eq!(visible.reply_to, sent.reply_to);
    assert_eq!(visible.in_thread, sent.in_thread);
    let mut wrong = selected;
    wrong.source_id = "root".into();
    let count = sender.store.event_count()?;
    assert!(sender
        .reply_selected(SPACE, &owner.id(), "Bad", "child", true, Some(&wrong))
        .is_err());
    assert_eq!(sender.store.event_count()?, count);
    Ok(())
}

#[test]
fn signed_forgery_wrong_revision_and_reader_do_not_project_quotes() -> TestResult {
    let owner = Signer::from_secret(&[49; 32]);
    let author = Author::root(owner.clone());
    let reader = Signer::from_secret(&[50; 32]);
    let mut log = SpaceLog::new(SPACE);
    base(&mut log, &author)?;
    append(
        &mut log,
        &author,
        "reader",
        EventBody::MemberAdded {
            identity: reader.id(),
            role: Role::Reader,
        },
    )?;
    for case in ["forgery", "revision", "reader"] {
        let mut q = quote_for(&log, "root")?;
        if case == "forgery" {
            q.text = "not what was selected".into();
        }
        if case == "revision" {
            q.revision = "not-a-source-revision".into();
        }
        let mut reference = ReplyRef::inline("root");
        reference.quote = Some(Box::new(q.signed()));
        let by = if case == "reader" {
            Author::root(reader.clone())
        } else {
            author.clone()
        };
        append(
            &mut log,
            &by,
            case,
            post("Must not project", Some(reference)),
        )?;
    }
    let mut receiver = replica(":memory:", &owner)?;
    for event in log.events() {
        receive(&mut receiver, event)?;
    }
    assert_eq!(
        receiver
            .timeline(SPACE)?
            .iter()
            .filter(|e| matches!(e.id.as_str(), "forgery" | "revision" | "reader"))
            .count(),
        0
    );
    Ok(())
}
