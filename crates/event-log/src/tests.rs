use super::*;
use types::{Privacy, Role, SpaceKind};

fn sample() -> Result<(SpaceLog, Signer, Signer), Box<dyn std::error::Error>> {
    let enzo = Signer::generate()?;
    let marina = Signer::generate()?;
    let mut log = SpaceLog::new("sp_paraty");
    log.append(
        &enzo,
        1,
        EventBody::SpaceCreated {
            title: "Paraty".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    log.append(
        &enzo,
        2,
        EventBody::MemberAdded {
            identity: marina.id(),
            role: Role::Member,
        },
    )?;
    log.append(
        &marina,
        3,
        EventBody::MessagePosted {
            message: "m1".into(),
            text: "Bora pra Paraty?".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    log.append(
        &enzo,
        4,
        EventBody::MessagePosted {
            message: "m2".into(),
            text: "Bora!".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    Ok((log, enzo, marina))
}

#[test]
fn valid_log_verifies_and_chains() {
    let (log, _, _) = sample().unwrap();
    assert_eq!(log.len(), 4);
    assert!(log.verify().is_ok());
    assert_eq!(log.events()[0].prev, GENESIS_PREV);
    for w in log.events().windows(2) {
        assert_eq!(w[1].prev, w[0].hash);
    }
    assert_eq!(log.head_hash(), log.events()[3].hash);
}

#[test]
fn roundtrip_through_json_still_verifies() {
    let (log, _, _) = sample().unwrap();
    let json = serde_json::to_string(log.events()).unwrap();
    let events: Vec<Event> = serde_json::from_str(&json).unwrap();
    assert!(SpaceLog::from_events("sp_paraty", events).is_ok());
}

/// Rewrites the signed bytes the way an attacker with the database would.
fn rewrite(e: &Event, edit: impl FnOnce(&mut SignedContent)) -> Event {
    let mut c = SignedContent::parse(&e.content).unwrap();
    edit(&mut c);
    event_from_content(
        c.encode(),
        e.sig.clone(),
        e.cert.clone(),
        e.seq,
        e.prev.clone(),
        e.hash.clone(),
    )
    .unwrap()
}

fn rehash(e: &mut Event) {
    e.hash = chain_hash(&e.space, e.seq, &e.prev, &wire_hash_of(e));
}

#[test]
fn tampered_content_is_detected() {
    let (log, _, _) = sample().unwrap();
    let mut events = log.events().to_vec();
    events[2] = rewrite(&events[2], |c| {
        c.payload = Some(Payload::Body(content::encode_body(
            &EventBody::MessagePosted {
                message: "m1".into(),
                text: "Bora pra Búzios?".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )));
    });
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadHash { seq: 2 }
    );
}

#[test]
fn a_view_that_disagrees_with_the_signed_bytes_is_rejected() {
    let (log, _, _) = sample().unwrap();
    let mut events = log.events().to_vec();
    events[2].body = EventBody::MessagePosted {
        message: "m1".into(),
        text: "Bora pra Búzios?".into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    };
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadContent { seq: 2 }
    );
}

#[test]
fn rehashed_tamper_fails_signature() {
    let (log, _, _) = sample().unwrap();
    let mut events = log.events().to_vec();
    events[3] = rewrite(&events[3], |c| {
        c.payload = Some(Payload::Body(content::encode_body(
            &EventBody::MessagePosted {
                message: "m2".into(),
                text: "Não vou.".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )));
    });
    rehash(&mut events[3]);
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadSignature { seq: 3 }
    );
}

#[test]
fn forged_author_fails_signature() {
    let (log, enzo, _) = sample().unwrap();
    let mut events = log.events().to_vec();
    events[2] = rewrite(&events[2], |c| c.author = enzo.id());
    rehash(&mut events[2]);
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadSignature { seq: 2 }
    );
}

#[test]
fn fields_and_kinds_from_newer_clients_survive_verbatim() {
    let author = Signer::generate().unwrap();
    let mut log = SpaceLog::new("sp_new");
    log.append(
        &author,
        1,
        EventBody::SpaceCreated {
            title: "t".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
    .unwrap();
    let mut content = SignedContent {
        v: 3,
        space: "sp_new".into(),
        client_id: "c_future".into(),
        author: author.id(),
        device: None,
        at_ms: 2,
        seen: log.head().as_ref().map(Into::into),
        payload: Some(Payload::Body(
            br#"{"PollOpened":{"question":"Praia?"}}"#.to_vec(),
        )),
    }
    .encode();
    content.extend_from_slice(&[0xA2, 0x06, 0x03, b'n', b'e', b'w']);
    let sig = author.sign(content::content_hash(&content).as_bytes());
    let e =
        event_from_content(content.clone(), sig, None, 0, String::new(), String::new()).unwrap();
    assert_eq!(
        e.body,
        EventBody::Unsupported {
            kind: "PollOpened".into()
        }
    );
    let sequenced = log.sequence(e).clone();
    assert_eq!(sequenced.content, content);
    let json = serde_json::to_string(log.events()).unwrap();
    assert!(SpaceLog::from_events("sp_new", serde_json::from_str(&json).unwrap()).is_ok());
}

#[test]
fn a_sequencer_cannot_move_an_event_before_what_its_author_saw() {
    let (log, _, marina) = sample().unwrap();
    let mut relay = SpaceLog::new("sp_paraty");
    for e in &log.events()[..2] {
        relay.accept(e.clone()).unwrap();
    }
    let reply = Author::root(marina).sign_event(
        "sp_paraty",
        "late",
        9,
        relay.head(),
        EventBody::MessagePosted {
            message: "m9".into(),
            text: "Concordo".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let mut forked = SpaceLog::new("sp_paraty");
    forked.accept(log.events()[0].clone()).unwrap();
    let mut moved = reply.clone();
    moved.seq = 1;
    moved.prev = forked.head_hash().to_string();
    rehash(&mut moved);
    assert_eq!(
        forked.accept(moved).unwrap_err(),
        LogError::BrokenCausalLink { seq: 1 }
    );

    let mut placed = reply;
    placed.seq = 2;
    placed.prev = relay.head_hash().to_string();
    rehash(&mut placed);
    assert!(relay.accept(placed).is_ok());
}

#[test]
fn words_signed_on_one_history_do_not_verify_on_another() {
    let enzo = Signer::generate().unwrap();
    let mut real = SpaceLog::new("sp_fork");
    real.append(
        &enzo,
        1,
        EventBody::SpaceCreated {
            title: "Real".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
    .unwrap();
    real.append(
        &enzo,
        2,
        EventBody::MessagePosted {
            message: "a".into(),
            text: "Vamos sábado".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )
    .unwrap();
    let mut fake = SpaceLog::new("sp_fork");
    fake.accept(real.events()[0].clone()).unwrap();
    fake.append(
        &enzo,
        3,
        EventBody::MessagePosted {
            message: "b".into(),
            text: "Cancelado".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )
    .unwrap();
    let answer = Author::root(enzo.clone()).sign_event(
        "sp_fork",
        "ans",
        4,
        real.head(),
        EventBody::MessagePosted {
            message: "c".into(),
            text: "Confirmado".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let mut shown = answer;
    shown.seq = fake.next_seq();
    shown.prev = fake.head_hash().to_string();
    rehash(&mut shown);
    assert_eq!(
        fake.accept(shown).unwrap_err(),
        LogError::BrokenCausalLink { seq: 2 }
    );
}

#[test]
fn only_creation_and_self_join_may_skip_the_causal_link() {
    let (log, enzo, _) = sample().unwrap();
    let mut client = SpaceLog::new("sp_paraty");
    for e in log.events() {
        client.accept(e.clone()).unwrap();
    }
    let blind = Author::root(enzo).sign_event(
        "sp_paraty",
        "blind",
        9,
        None,
        EventBody::MessagePosted {
            message: "m9".into(),
            text: "?".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    let mut e = blind;
    e.seq = client.next_seq();
    e.prev = client.head_hash().to_string();
    rehash(&mut e);
    assert_eq!(
        client.accept(e).unwrap_err(),
        LogError::MissingCausalLink { seq: 4 }
    );

    let newcomer = Signer::generate().unwrap();
    let join = Author::root(newcomer.clone()).sign_event(
        "sp_paraty",
        "join",
        10,
        None,
        EventBody::MemberAdded {
            identity: newcomer.id(),
            role: Role::Member,
        },
    );
    let mut j = join;
    j.seq = client.next_seq();
    j.prev = client.head_hash().to_string();
    rehash(&mut j);
    assert!(client.accept(j).is_ok());
}

#[test]
fn removed_or_reordered_events_are_detected() {
    let (log, _, _) = sample().unwrap();
    let mut removed = log.events().to_vec();
    removed.remove(1);
    assert_eq!(
        SpaceLog::from_events("sp_paraty", removed).unwrap_err(),
        LogError::BadSequence {
            expected: 1,
            got: 2
        }
    );

    let mut reordered = log.events().to_vec();
    reordered.swap(2, 3);
    assert!(SpaceLog::from_events("sp_paraty", reordered).is_err());

    // Renumerar depois de remover ainda quebra a cadeia.
    let mut renumbered = log.events().to_vec();
    renumbered.remove(1);
    for (i, e) in renumbered.iter_mut().enumerate() {
        e.seq = i as u64;
    }
    assert!(matches!(
        SpaceLog::from_events("sp_paraty", renumbered).unwrap_err(),
        LogError::BrokenChain { .. } | LogError::BadHash { .. }
    ));
}

#[test]
fn events_from_another_space_are_rejected() {
    let (log, _, _) = sample().unwrap();
    assert_eq!(
        SpaceLog::from_events("sp_outro", log.events().to_vec()).unwrap_err(),
        LogError::WrongSpace { seq: 0 }
    );
}

#[test]
fn device_keys_sign_for_their_identity() {
    let root = Signer::generate().unwrap();
    let phone = Author::device(&root, Signer::generate().unwrap());
    let mut log = SpaceLog::new("sp_dm");
    let e = phone.sign_event(
        "sp_dm",
        "c1",
        1,
        None,
        EventBody::SpaceCreated {
            title: "t".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::Closed,
        },
    );
    assert_eq!(e.author, root.id());
    log.sequence(e);
    assert!(log.verify().is_ok());

    // A device certified by someone else can't speak for this identity.
    let mallory = Signer::generate().unwrap();
    let fake = Author::certified(
        root.id(),
        Signer::generate().unwrap(),
        mallory.sign(b"zoen-device-v1:x"),
    );
    let e = fake.sign_event(
        "sp_dm",
        "c2",
        2,
        log.head(),
        EventBody::MessagePosted {
            message: "m".into(),
            text: "oi".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    log.sequence(e);
    assert_eq!(
        log.verify().unwrap_err(),
        LogError::BadDeviceCert { seq: 1 }
    );
}

#[test]
fn relay_sequenced_events_are_accepted_in_order_only() {
    let root = Signer::generate().unwrap();
    let me = Author::root(root);
    let mut relay = SpaceLog::new("sp_g");
    let a = relay
        .sequence(me.sign_event(
            "sp_g",
            "a",
            1,
            None,
            EventBody::SpaceCreated {
                title: "g".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .clone();
    let b = relay
        .sequence(me.sign_event(
            "sp_g",
            "b",
            2,
            relay.head(),
            EventBody::MessagePosted {
                message: "m".into(),
                text: "x".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        ))
        .clone();
    let mut client = SpaceLog::new("sp_g");
    assert_eq!(
        client.accept(b.clone()).unwrap_err(),
        LogError::BadSequence {
            expected: 0,
            got: 1
        }
    );
    client.accept(a).unwrap();
    client.accept(b).unwrap();
    assert_eq!(client.head_hash(), relay.head_hash());
}

#[test]
fn an_end_to_end_space_cannot_be_created_again_as_readable() {
    let me = Author::root(Signer::generate().unwrap());
    let created = |client_id: &str, privacy, seen| {
        let body = EventBody::SpaceCreated {
            title: "g".into(),
            kind: SpaceKind::Group,
            privacy,
        };
        me.sign_event("sp_g", client_id, 1, seen, body)
    };
    let mut relay = SpaceLog::new("sp_g");
    let first = relay
        .sequence(created("a", Privacy::EndToEnd, None))
        .clone();
    let seen = relay.head();
    let again = relay.sequence(created("b", Privacy::Closed, seen)).clone();
    assert_eq!(relay.verify().unwrap_err(), LogError::Recreated { seq: 1 });
    let mut client = SpaceLog::new("sp_g");
    client.accept(first).unwrap();
    assert_eq!(
        client.accept(again).unwrap_err(),
        LogError::Recreated { seq: 1 }
    );
}

#[test]
fn signer_roundtrips_secret() {
    let s = Signer::generate().unwrap();
    let again = Signer::from_secret(&s.secret());
    assert_eq!(s.id(), again.id());
    assert_eq!(s.id().len(), 64);
}

#[test]
fn json_hash_is_stable_and_sensitive() {
    assert_eq!(content_hash(&("a", 1)), content_hash(&("a", 1)));
    assert_ne!(content_hash(&("a", 1)), content_hash(&("a", 2)));
}

#[test]
fn a_causal_position_cannot_wrap_to_an_earlier_index() {
    let (log, _, _) = sample().unwrap();
    let mut event = log.events()[2].clone();
    event.seq = (1_u64 << 32) + 1;
    event.seen = Some(Seen {
        seq: 1_u64 << 32,
        hash: log.events()[0].hash.clone(),
    });
    assert_eq!(
        verify_causal_link(log.events(), &event),
        Err(LogError::BrokenCausalLink { seq: event.seq })
    );
}
