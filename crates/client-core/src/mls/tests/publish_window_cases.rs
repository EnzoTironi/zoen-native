//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

/// Twelve sealed pages, delivered in the worst order the publish window allows.
/// A seventh generation opened before the first is permanent loss; the outbox
/// never offers that sixth in-flight ciphertext.
pub(super) fn publish_window_keeps_a_reordered_burst_decryptable(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use ::mls::sealed::state_key;
    use event_log::Signer;
    use std::collections::{BTreeSet, HashMap};

    let mut engine = Engine::open(":memory:").unwrap();
    let (root_secret, _, _) = engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let author = engine.net.author.clone().unwrap();
    let space = "sp_ratchet_window";
    let genesis = Envelope::plain(&author.sign_event(
        space,
        "created",
        1,
        None,
        EventBody::SpaceCreated {
            title: "Window".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?)?;
    assert_eq!(engine.ingest(ordered(&engine, genesis)?), Ingest::Applied);
    engine.create_mls_group(space).unwrap();

    let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate()?);
    let mut peer_conn = Connection::open_in_memory().unwrap();
    ::mls::migrate(&mut peer_conn).unwrap();
    let peer = Device::new(
        &peer_conn,
        state_key(&peer_author.key.secret())?,
        &peer_author.identity,
        peer_author.key.secret(),
        peer_author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let packages = peer.key_packages(1, false).unwrap();
    let adding = engine
        .device()
        .unwrap()
        .commit(space, &packages, &Default::default())
        .unwrap();
    let commit = Envelope::sealed(
        &author,
        space,
        "add-peer",
        2,
        seen(&engine, space).as_ref(),
        Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
    )?;
    assert_eq!(engine.ingest(ordered(&engine, commit)?), Ingest::Applied);
    let roster: BTreeSet<String> = BTreeSet::from([author.identity.clone()]);
    let welcome_bytes = adding.welcome.unwrap();
    assert!(peer.join(space, &welcome_bytes, &roster).unwrap());
    let welcome = Envelope::sealed(
        &author,
        space,
        "welcome-peer",
        3,
        seen(&engine, space).as_ref(),
        Sealed::new(SealedKind::Welcome, SUITE_ID, welcome_bytes),
    )?;
    assert_eq!(engine.ingest(ordered(&engine, welcome)?), Ingest::Applied);
    let sender = engine.device().unwrap().leaf().clone();

    for i in 0..12 {
        engine
            .append_synced(
                space,
                &author,
                1_000 + i,
                EventBody::MessagePosted {
                    message: format!("m{i}"),
                    text: format!("page-{i}"),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                    forwarded_author: None,
                },
            )
            .unwrap();
    }
    assert!(engine.mls_seal_outbox().unwrap());
    let mut opened = 0u32;
    let mut remaining = 12usize;
    while remaining > 0 {
        let apps: Vec<_> = engine
            .outbox_envelopes()
            .into_iter()
            .filter(|env| env.sealed_kind() == Some(SealedKind::Application))
            .collect();
        assert!(
            !apps.is_empty() && apps.len() <= 5,
            "in flight {}, still queued {remaining}",
            apps.len()
        );
        assert!(apps.len() < remaining || remaining <= 5);
        let mut sent = HashMap::new();
        for env in &apps {
            sent.insert(env.client_id().to_string(), Some(0u64));
        }
        assert!(
            engine
                .outbox_envelopes_except(&sent)
                .iter()
                .all(|env| env.sealed_kind() != Some(SealedKind::Application)),
            "a later generation left while {} were still unaccepted",
            apps.len()
        );
        for env in apps.iter().rev() {
            let data = env.sealed_data().unwrap().1;
            match peer.open(space, data, &roster, &sender).unwrap() {
                Opened::Application { plaintext, .. } => {
                    assert!(
                        plaintext.windows(b"page-".len()).any(|w| w == b"page-"),
                        "opened bytes are not a page"
                    );
                }
                other => panic!("expected an application message, got {other:?}"),
            }
            opened += 1;
        }
        for env in &apps {
            assert_eq!(
                engine.ingest(ordered(&engine, env.clone())?),
                Ingest::Confirmed
            );
        }
        remaining -= apps.len();
    }
    assert_eq!(opened, 12);

    let mut late = Vec::new();
    for i in 0..7 {
        late.push(
            engine
                .device()
                .unwrap()
                .seal(space, format!("gap-{i}").as_bytes())
                .unwrap(),
        );
    }
    peer.open(space, &late[6], &roster, &sender).unwrap();
    let err = peer.open(space, &late[0], &roster, &sender).unwrap_err();
    assert!(
        format!("{err:?}").contains("TooDistantInThePast"),
        "{err:?}"
    );

    Ok(())
}

/// Space A already has five application generations in flight. Its sixth
/// application, and anything newer in that same Space, stays queued. Space B
/// still publishes.
pub(super) fn publish_window_lets_the_next_space_through(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use ::mls::sealed::state_key;
    use event_log::Signer;
    use std::collections::{BTreeSet, HashMap};

    let mut engine = Engine::open(":memory:").unwrap();
    let (root_secret, _, _) = engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let author = engine.net.author.clone().unwrap();
    let space_a = "sp_cap_a";
    assert_eq!(
        engine.ingest(ordered(
            &engine,
            Envelope::plain(&author.sign_event(
                space_a,
                "created-a",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "A".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            )?)?,
        )?),
        Ingest::Applied
    );
    engine.create_mls_group(space_a).unwrap();

    let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate()?);
    let mut peer_conn = Connection::open_in_memory().unwrap();
    ::mls::migrate(&mut peer_conn).unwrap();
    let peer = Device::new(
        &peer_conn,
        state_key(&peer_author.key.secret())?,
        &peer_author.identity,
        peer_author.key.secret(),
        peer_author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let packages = peer.key_packages(1, false).unwrap();
    let adding = engine
        .device()
        .unwrap()
        .commit(space_a, &packages, &Default::default())
        .unwrap();
    assert_eq!(
        engine.ingest(ordered(
            &engine,
            Envelope::sealed(
                &author,
                space_a,
                "add-peer",
                2,
                seen(&engine, space_a).as_ref(),
                Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
            )?,
        )?),
        Ingest::Applied
    );
    let roster: BTreeSet<String> = BTreeSet::from([author.identity.clone()]);
    let welcome_bytes = adding.welcome.unwrap();
    assert!(peer.join(space_a, &welcome_bytes, &roster).unwrap());
    assert_eq!(
        engine.ingest(ordered(
            &engine,
            Envelope::sealed(
                &author,
                space_a,
                "welcome-peer",
                3,
                seen(&engine, space_a).as_ref(),
                Sealed::new(SealedKind::Welcome, SUITE_ID, welcome_bytes),
            )?,
        )?),
        Ingest::Applied
    );

    let space_b = "sp_cap_b";
    assert_eq!(
        engine.ingest(ordered(
            &engine,
            Envelope::plain(&author.sign_event(
                space_b,
                "created-b",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "B".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )?)?,
        )?),
        Ingest::Applied
    );

    let mut a_messages = Vec::new();
    for i in 0..6 {
        a_messages.push(
            engine
                .append_synced(
                    space_a,
                    &author,
                    1_000 + i,
                    EventBody::MessagePosted {
                        message: format!("a{i}"),
                        text: format!("page-{i}"),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                        forwarded_author: None,
                    },
                )
                .unwrap(),
        );
    }
    let b_message = engine
        .append_synced(
            space_b,
            &author,
            2_000,
            EventBody::MessagePosted {
                message: "b".into(),
                text: "other-space".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )
        .unwrap();
    assert!(engine.mls_seal_outbox().unwrap());
    let handshake = engine
        .handshake_event(
            space_a,
            Sealed::new(SealedKind::Commit, SUITE_ID, b"later-commit".to_vec()),
        )
        .unwrap();
    assert!(matches!(
        &handshake.body,
        EventBody::Sealed { kind } if kind == "Commit"
    ));
    engine.store.outbox_put(&handshake).unwrap();
    assert!(a_messages[5].at_ms < b_message.at_ms);
    assert!(b_message.at_ms < handshake.at_ms);

    let order: Vec<String> = engine
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|pending| pending.event.client_id)
        .collect();
    let sixth_at = order
        .iter()
        .position(|id| id == &a_messages[5].client_id)
        .unwrap();
    let b_at = order
        .iter()
        .position(|id| id == &b_message.client_id)
        .unwrap();
    let handshake_at = order
        .iter()
        .position(|id| id == &handshake.client_id)
        .unwrap();
    assert!(sixth_at < b_at && b_at < handshake_at);

    let mut sent = HashMap::new();
    for message in a_messages.iter().take(5) {
        sent.insert(message.client_id.clone(), Some(0u64));
    }
    let offered = engine.outbox_envelopes_except(&sent);
    assert!(
        offered
            .iter()
            .any(|env| env.client_id() == b_message.client_id),
        "space B is missing from the publish batch"
    );
    assert!(
        offered
            .iter()
            .all(|env| env.client_id() != a_messages[5].client_id),
        "A's sixth application left while five were in flight"
    );
    assert!(
        offered
            .iter()
            .all(|env| env.client_id() != handshake.client_id),
        "A's later handshake overtook the held application"
    );
    assert!(offered.iter().all(|env| env.space() != space_a));

    Ok(())
}
