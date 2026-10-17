//! Cross-client references must resolve chain hashes before optional client aliases.
use super::*;

use crate::engine::Engine;
use roda_types::{Event, EventBody, ReplyRef};

fn append_message(
    engine: &mut Engine,
    space: &str,
    author: &str,
    client_id: &str,
    text: &str,
    reply: Option<ReplyRef>,
) -> Event {
    let author = engine.author_for(author).unwrap();
    let signed = author.sign_event(
        space,
        client_id,
        crate::engine::now_ms(),
        engine.logs.get(space).and_then(roda_log::SpaceLog::head),
        EventBody::MessagePosted {
            message: roda_types::new_id("msg"),
            text: text.into(),
            attaches: None,
            reply,
            forwarded: false,
        },
    );
    let event = engine.logs.get_mut(space).unwrap().sequence(signed).clone();
    engine.store.append_event(&event).unwrap();
    engine.reload().unwrap();
    event
}

fn root_event(engine: &RodaEngine, space: &str, id: &str) -> Event {
    engine
        .lock()
        .store
        .events(space)
        .unwrap()
        .into_iter()
        .find(|event| event.client_id == id)
        .unwrap()
}

#[test]
fn hash_referenced_replies_share_native_thread_counts_and_watermarks() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let root = e.send_message(space.id.clone(), "Root".into()).unwrap();
    let hash = root_event(&e, &space.id, &root.id).hash;
    e.mark_read(space.id.clone()).unwrap();
    let reply = append_message(
        &mut e.lock(),
        &space.id,
        &other,
        "web-thread-reply",
        "From the web",
        Some(ReplyRef::thread(&hash)),
    );
    let timeline = e.timeline(space.id.clone()).unwrap();
    let root_dto = timeline.iter().find(|entry| entry.id == root.id).unwrap();
    assert_eq!(root_dto.thread_replies, 1);
    assert_eq!(root_dto.thread_unread, 1);
    let reply_dto = timeline
        .iter()
        .find(|entry| entry.id == reply.client_id)
        .unwrap();
    assert_eq!(reply_dto.in_thread.as_deref(), Some(root.id.as_str()));
    assert_eq!(
        e.thread(space.id.clone(), root.id.clone()).unwrap().len(),
        2
    );
    assert_eq!(e.thread(space.id.clone(), hash.clone()).unwrap().len(), 2);
    let summary = e.lock().space_summary(&space.id).unwrap();
    assert_eq!(
        summary.unread, 0,
        "thread-only replies stay out of chat unread"
    );
    assert_eq!(summary.threads_unread, 1);

    e.set_thread_follow(space.id.clone(), hash.clone(), Some(false))
        .unwrap();
    assert!(!e.thread(space.id.clone(), root.id.clone()).unwrap()[0].thread_following);
    e.set_thread_follow(space.id.clone(), root.id.clone(), Some(true))
        .unwrap();
    assert!(e.thread(space.id.clone(), hash.clone()).unwrap()[0].thread_following);
    e.mark_thread_read(space.id.clone(), hash).unwrap();
    assert_eq!(
        e.thread(space.id.clone(), root.id).unwrap()[0].thread_unread,
        0
    );
    assert!(e.verify_all().iter().all(|report| report.valid));
}

#[test]
fn hash_referenced_inline_quotes_and_native_sends_resolve_the_same_message() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let root = e
        .send_message(space.id.clone(), "Original text".into())
        .unwrap();
    let hash = root_event(&e, &space.id, &root.id).hash;
    let reply = append_message(
        &mut e.lock(),
        &space.id,
        &other,
        "web-inline-reply",
        "Quoted",
        Some(ReplyRef::inline(&hash)),
    );
    let timeline = e.timeline(space.id.clone()).unwrap();
    let quote = timeline
        .iter()
        .find(|entry| entry.id == reply.client_id)
        .unwrap()
        .reply_to
        .as_ref()
        .expect("hash-referenced quote");
    assert_eq!(quote.id, root.id);
    assert_eq!(quote.text, "Original text");
    let sent = e
        .send_thread_reply(space.id.clone(), "Native reply".into(), hash, true)
        .unwrap();
    assert_eq!(sent.in_thread.as_deref(), Some(root.id.as_str()));
    assert!(sent.also_in_chat);
}

#[test]
fn a_client_alias_cannot_shadow_another_messages_authoritative_hash() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let root = e
        .send_message(space.id.clone(), "Authoritative root".into())
        .unwrap();
    let root_hash = root_event(&e, &space.id, &root.id).hash;
    let shadow = append_message(
        &mut e.lock(),
        &space.id,
        &other,
        &root_hash,
        "Shadow root",
        None,
    );
    let sent = e
        .send_thread_reply(
            space.id.clone(),
            "For the authoritative root".into(),
            root_hash.clone(),
            false,
        )
        .unwrap();
    assert_eq!(sent.in_thread.as_deref(), Some(root.id.as_str()));
    let timeline = e.timeline(space.id.clone()).unwrap();
    let shadow_dto = timeline
        .iter()
        .find(
            |entry| matches!(&entry.kind, EntryKind::Message { text, .. } if text == "Shadow root"),
        )
        .unwrap();
    assert_eq!(
        shadow_dto.id, shadow.hash,
        "unsafe aliases must not escape as UI ids"
    );
    assert_eq!(
        e.thread(space.id.clone(), root_hash).unwrap()[0].id,
        root.id
    );
    assert_eq!(
        e.thread(space.id.clone(), shadow.hash.clone()).unwrap()[0].id,
        shadow.hash
    );
    e.set_thread_follow(space.id.clone(), shadow.hash.clone(), Some(true))
        .unwrap();
    e.set_thread_follow(space.id.clone(), root.id.clone(), Some(false))
        .unwrap();
    assert!(!e.thread(space.id.clone(), root.id).unwrap()[0].thread_following);
    assert!(e.thread(space.id.clone(), shadow.hash).unwrap()[0].thread_following);
    assert!(e.verify_all().iter().all(|report| report.valid));
}

#[test]
fn client_aliases_cannot_claim_known_non_timeline_hashes() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let reserved: Vec<_> = e
        .lock()
        .store
        .events(&space.id)
        .unwrap()
        .into_iter()
        .filter(|event| {
            matches!(
                event.body,
                EventBody::SpaceCreated { .. } | EventBody::MemberAdded { .. }
            )
        })
        .map(|event| event.hash)
        .collect();
    assert!(reserved.len() >= 2, "genesis and membership control hashes");
    for reserved_hash in reserved {
        let shadow = append_message(
            &mut e.lock(),
            &space.id,
            &other,
            &reserved_hash,
            "Cannot claim a control hash",
            None,
        );
        let timeline = e.timeline(space.id.clone()).unwrap();
        let shadow_dto = timeline
            .iter()
            .find(|entry| entry.seq == shadow.seq)
            .unwrap();
        assert_eq!(
            shadow_dto.id, shadow.hash,
            "a known non-timeline hash is not a usable message alias"
        );
        assert!(e.thread(space.id.clone(), reserved_hash.clone()).is_err());
        assert!(e
            .send_thread_reply(
                space.id.clone(),
                "Cannot reply to a control event".into(),
                reserved_hash.clone(),
                false,
            )
            .is_err());
        assert!(e
            .lock()
            .annotate(
                &space.id,
                &reserved_hash,
                roda_types::Annotation::React { emoji: "x".into() },
            )
            .is_err());
        let sent = e
            .send_thread_reply(
                space.id.clone(),
                "For the actual message".into(),
                shadow.hash.clone(),
                false,
            )
            .unwrap();
        assert_eq!(sent.in_thread.as_deref(), Some(shadow.hash.as_str()));
        let panel = e.thread(space.id.clone(), shadow.hash).unwrap();
        assert_eq!(panel.len(), 2);
        assert!(panel[0].reactions.is_empty());
    }
    let unknown_hash_like_alias = "0123456789abcdef".repeat(4);
    let safe = append_message(
        &mut e.lock(),
        &space.id,
        &other,
        &unknown_hash_like_alias,
        "A hash-shaped alias without a real event",
        None,
    );
    let panel = e.thread(space.id, unknown_hash_like_alias.clone()).unwrap();
    assert_eq!(panel[0].id, safe.client_id);
    assert_eq!(panel[0].id, unknown_hash_like_alias);
    assert!(e.verify_all().iter().all(|report| report.valid));
}

#[test]
fn duplicate_client_aliases_keep_distinct_native_thread_roots() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let first = e
        .send_message(space.id.clone(), "First root".into())
        .unwrap();
    let second = append_message(
        &mut e.lock(),
        &space.id,
        &other,
        &first.id,
        "Second root",
        None,
    );
    let timeline = e.timeline(space.id.clone()).unwrap();
    let second_dto = timeline
        .iter()
        .find(
            |entry| matches!(&entry.kind, EntryKind::Message { text, .. } if text == "Second root"),
        )
        .unwrap();
    assert_eq!(second_dto.id, second.hash);
    let sent = e
        .send_thread_reply(
            space.id.clone(),
            "Only the second".into(),
            second.hash.clone(),
            false,
        )
        .unwrap();
    assert_eq!(sent.in_thread.as_deref(), Some(second.hash.as_str()));
    assert_eq!(e.thread(space.id.clone(), first.id).unwrap().len(), 1);
    assert_eq!(
        e.thread(space.id.clone(), second.hash.clone())
            .unwrap()
            .len(),
        2
    );
    e.lock()
        .annotate(
            &space.id,
            &second.hash,
            roda_types::Annotation::React { emoji: "x".into() },
        )
        .unwrap();
    let panel = e.thread(space.id, second.hash).unwrap();
    assert_eq!(
        panel[0].reactions.len(),
        1,
        "canonical fallback ids remain actionable"
    );
    assert!(e.verify_all().iter().all(|report| report.valid));
}

#[test]
fn personal_hash_roots_merge_with_native_alias_follow_and_read_state() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let root = e.send_message(space.id.clone(), "Root".into()).unwrap();
    let hash = root_event(&e, &space.id, &root.id).hash;
    e.lock()
        .reply_with(&space.id, &other, "Unread", None, &root.id, true)
        .unwrap();
    let read = e.thread(space.id.clone(), root.id.clone()).unwrap()[1].seq;
    let personal = e.lock().personal_space().unwrap();
    let me = e.me().unwrap().id;
    e.lock()
        .append(
            &personal,
            &me,
            EventBody::ThreadStateSet {
                space: space.id.clone(),
                root: hash.clone(),
                follow: Some(false),
                read_up_to: read,
                read_only: false,
            },
        )
        .unwrap();
    let panel = e.thread(space.id.clone(), root.id.clone()).unwrap();
    assert!(
        !panel[0].thread_following,
        "a web hash-root override applies to the native alias"
    );
    assert_eq!(panel[0].thread_unread, 0);
    e.set_thread_follow(space.id.clone(), root.id.clone(), Some(true))
        .unwrap();
    assert!(e.thread(space.id.clone(), hash.clone()).unwrap()[0].thread_following);
    e.lock()
        .append(
            &personal,
            &me,
            EventBody::ThreadStateSet {
                space: space.id.clone(),
                root: hash,
                follow: Some(false),
                read_up_to: 0,
                read_only: true,
            },
        )
        .unwrap();
    let panel = e.thread(space.id, root.id).unwrap();
    assert!(
        panel[0].thread_following,
        "read-only events never replace the follow register"
    );
    assert_eq!(panel[0].thread_unread, 0);
}

#[test]
fn reading_observed_thread_entries_does_not_consume_a_later_reply() {
    let e = seeded();
    let space = find_space(&e, "Marina");
    let other = space.counterpart.as_ref().unwrap().id.clone();
    let root = e.send_message(space.id.clone(), "Root".into()).unwrap();
    e.lock()
        .reply_with(&space.id, &other, "Observed", None, &root.id, true)
        .unwrap();
    let observed = e
        .thread(space.id.clone(), root.id.clone())
        .unwrap()
        .iter()
        .map(|entry| entry.seq)
        .max()
        .unwrap();
    e.lock()
        .reply_with(
            &space.id,
            &other,
            "Arrived after the snapshot",
            None,
            &root.id,
            true,
        )
        .unwrap();
    e.mark_thread_read_up_to(space.id.clone(), root.id.clone(), observed)
        .unwrap();
    let panel = e.thread(space.id.clone(), root.id.clone()).unwrap();
    assert_eq!(
        panel[0].thread_unread, 1,
        "a reply newer than observed seq {observed} stays unread"
    );
    e.mark_thread_read_up_to(space.id.clone(), root.id.clone(), 0)
        .unwrap();
    assert_eq!(
        e.thread(space.id.clone(), root.id.clone()).unwrap()[0].thread_unread,
        1,
        "an older observed boundary cannot lower the watermark"
    );
    e.mark_thread_read_up_to(space.id.clone(), root.id.clone(), u64::MAX)
        .unwrap();
    assert_eq!(
        e.thread(space.id.clone(), root.id.clone()).unwrap()[0].thread_unread,
        0
    );
    e.lock()
        .reply_with(
            &space.id,
            &other,
            "After the clamped read",
            None,
            &root.id,
            true,
        )
        .unwrap();
    assert_eq!(
        e.thread(space.id, root.id).unwrap()[0].thread_unread,
        1,
        "a future boundary was clamped to replies that had actually arrived"
    );
}

#[test]
fn later_pending_personal_state_survives_same_millisecond_confirmations() {
    let engine = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    let mut e = engine.lock();
    e.create_account("Alice", "alice", "https://relay.example")
        .unwrap();
    let personal = e.personal_space().unwrap();
    let me = e.me.clone().unwrap();
    let confirm = |engine: &mut Engine, event: &Event| {
        let env = roda_proto::Envelope::plain(event);
        let seq = engine
            .logs
            .get(&personal)
            .map_or(0, roda_log::SpaceLog::next_seq);
        let prev = engine
            .logs
            .get(&personal)
            .and_then(|log| log.events().last())
            .map(|event| event.hash.clone())
            .unwrap_or_else(|| roda_types::GENESIS_PREV.into());
        let frame = roda_proto::Sequenced {
            seq,
            hash: roda_log::chain_hash(&personal, seq, &prev, &env.wire_hash()),
            prev,
            env,
        };
        assert_eq!(engine.ingest(frame), crate::sync::Ingest::Confirmed);
    };
    let genesis = e
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|queued| {
            queued.event.space == personal
                && matches!(queued.event.body, EventBody::SpaceCreated { .. })
        })
        .unwrap()
        .event;
    confirm(&mut e, &genesis);
    let at = crate::engine::now_ms();
    for body in [
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(true),
            read_up_to: 0,
            read_only: false,
        },
        EventBody::ThreadStateSet {
            space: "chat".into(),
            root: "root".into(),
            follow: Some(false),
            read_up_to: 0,
            read_only: false,
        },
        EventBody::ChatPrefsSet {
            space: "chat".into(),
            prefs: roda_types::ChatPrefs {
                pinned: true,
                ..Default::default()
            },
        },
        EventBody::ChatPrefsSet {
            space: "chat".into(),
            prefs: roda_types::ChatPrefs {
                pinned: false,
                ..Default::default()
            },
        },
    ] {
        e.append_at(&personal, &me, at, body).unwrap();
    }
    let queued: Vec<_> = e
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|queued| queued.event.space == personal)
        .map(|queued| queued.event)
        .collect();
    assert_eq!(queued.len(), 4);
    confirm(&mut e, &queued[0]);
    e.reload().unwrap();
    assert_eq!(
        e.state.thread_state[&("chat".into(), "root".into())].0,
        Some(false),
        "a later pending unfollow must win over an earlier confirmed follow"
    );
    confirm(&mut e, &queued[1]);
    confirm(&mut e, &queued[2]);
    e.reload().unwrap();
    assert!(
        !e.state.chat_prefs["chat"].pinned,
        "a later pending preference must win over an earlier confirmed preference"
    );
    confirm(&mut e, &queued[3]);
    assert!(!e.state.chat_prefs["chat"].pinned);
    e.reload().unwrap();
    assert!(!e.state.chat_prefs["chat"].pinned);
    assert_eq!(
        e.state.thread_state[&("chat".into(), "root".into())].0,
        Some(false)
    );
}
