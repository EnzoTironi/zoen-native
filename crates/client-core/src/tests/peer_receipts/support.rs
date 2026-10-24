//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn person(name: &str) -> Identity {
    Identity {
        id: String::new(),
        kind: IdentityKind::Person,
        name: name.into(),
        handle: name.to_lowercase(),
        tint_hex: "#246844".into(),
        glyph: None,
        owner: None,
        owner_proof: None,
        bio: "Synthetic receipt test".into(),
    }
}

pub(super) fn fixture(
) -> std::result::Result<(Database, Arc<Client>, Conversation), Box<dyn std::error::Error>> {
    let db = Database::new()?;
    let e = db.open();
    let (space, me, peer, outsider) = {
        let mut engine = e.lock();
        let me = engine.create_identity(person("Owner"), true).unwrap();
        let peer = engine.create_identity(person("Peer"), true).unwrap();
        let outsider = engine.create_identity(person("Outsider"), true).unwrap();
        engine.store.set_meta("me", &me).unwrap();
        engine.me = Some(me.clone());
        let space = types::new_id("sp")?;
        engine
            .append(
                &space,
                &me,
                EventBody::SpaceCreated {
                    title: "Synthetic receipt conversation".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )
            .unwrap();
        engine
            .append(
                &space,
                &me,
                EventBody::MemberAdded {
                    identity: peer.clone(),
                    role: Role::Member,
                },
            )
            .unwrap();
        (space, me, peer, outsider)
    };
    // Confirmed signed history is already on disk before this device goes offline.
    let text = e
        .send_message(space.clone(), "A retained message".into())
        .unwrap();
    let voice = e
        .send_voice_note(
            space.clone(),
            vec![7; 900],
            "audio/mp4".into(),
            1500,
            vec![0.2, 0.8],
            "A retained voice transcript".into(),
            None,
            false,
        )
        .unwrap();
    {
        let mut engine = e.lock();
        engine.store.set_synced(&space).unwrap();
        engine.net.synced.insert(space.clone());
        assert!(engine.store.meta("seed_receipts").unwrap().is_none());
        assert!(engine.store.outbox().unwrap().is_empty());
    }
    assert!(text.seq > 0 && voice.seq > text.seq);
    let conversation = Conversation {
        space,
        me,
        peer,
        outsider,
        text,
        voice,
    };
    Ok((db, e, conversation))
}

pub(super) fn entry(e: &Client, c: &Conversation, id: &str) -> TimelineEntry {
    e.timeline(c.space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap()
}

pub(super) fn assert_delivery(e: &Client, c: &Conversation, delivery: Delivery) {
    for original in [&c.text, &c.voice] {
        let current = entry(e, c, &original.id);
        assert_eq!(current.delivery, delivery, "{}", original.id);
        assert_eq!(current.voice, original.voice);
        assert_eq!(current.kind, original.kind);
        assert!(current.author.is_me);
    }
    assert_eq!(
        e.space(c.space.clone()).unwrap().last_delivery,
        Some(delivery)
    );
}

pub(super) fn membership(
    e: &Client,
    c: &Conversation,
    body: EventBody,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = e.lock();
    let signed = engine.author_for(&c.me).unwrap().sign_event(
        &c.space,
        &types::new_id("membership")?,
        crate::engine::now_ms(),
        engine.logs[&c.space].head(),
        body,
    );
    let mut relay = engine.logs[&c.space].clone();
    let event = relay.sequence(signed).clone();
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(&event),
        }),
        crate::sync::Ingest::Applied
    );

    Ok(())
}

pub(super) fn snapshot(e: &Client) -> ReceiptSnapshot {
    let engine = e.lock();
    let mut query = engine
        .store
        .conn()
        .prepare(
            "SELECT space, identity, delivered, read FROM peer_receipts ORDER BY space, identity",
        )
        .unwrap();
    let rows = query
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let memory = engine
        .net
        .receipts
        .iter()
        .map(|(space, peers)| {
            (
                space.clone(),
                peers
                    .iter()
                    .map(|(peer, mark)| (peer.clone(), *mark))
                    .collect(),
            )
        })
        .collect();
    let mut query = engine
        .store
        .conn()
        .prepare("SELECT key, value FROM meta ORDER BY key")
        .unwrap();
    let meta = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    ReceiptSnapshot { rows, memory, meta }
}
