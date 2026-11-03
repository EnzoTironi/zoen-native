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
            forwarded_author: None,
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
            forwarded_author: None,
        },
    )?;
    Ok((log, enzo, marina))
}

#[test]
fn valid_log_verifies_and_chains() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
    assert_eq!(log.len(), 4);
    assert!(log.verify().is_ok());
    assert_eq!(log.events()[0].prev, GENESIS_PREV);
    for w in log.events().windows(2) {
        assert_eq!(w[1].prev, w[0].hash);
    }
    assert_eq!(log.head_hash(), log.events()[3].hash);
    Ok(())
}

#[test]
fn roundtrip_through_json_still_verifies() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
    let json = serde_json::to_string(log.events())?;
    let events: Vec<Event> = serde_json::from_str(&json)?;
    assert!(SpaceLog::from_events("sp_paraty", events).is_ok());
    Ok(())
}

/// Rewrites the signed bytes the way an attacker with the database would.
fn rewrite(
    e: &Event,
    edit: impl FnOnce(&mut SignedContent) -> Result<(), serde_json::Error>,
) -> Result<Event, Box<dyn std::error::Error>> {
    let mut c = SignedContent::parse(&e.content).ok_or("unreadable test signed content")?;
    edit(&mut c)?;
    Ok(event_from_content(
        c.encode(),
        e.sig.clone(),
        e.cert.clone(),
        e.seq,
        e.prev.clone(),
        e.hash.clone(),
    )?)
}

fn rehash(e: &mut Event) -> Result<(), serde_json::Error> {
    e.hash = chain_hash(&e.space, e.seq, &e.prev, &wire_hash_of(e))?;
    Ok(())
}

#[test]
fn tampered_content_is_detected() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
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
                forwarded_author: None,
            },
        )?));
        Ok(())
    })?;
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadHash { seq: 2 }
    );
    Ok(())
}

#[test]
fn a_view_that_disagrees_with_the_signed_bytes_is_rejected(
) -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
    let mut events = log.events().to_vec();
    events[2].body = EventBody::MessagePosted {
        message: "m1".into(),
        text: "Bora pra Búzios?".into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
        forwarded_author: None,
    };
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadContent { seq: 2 }
    );
    Ok(())
}

#[test]
fn rehashed_tamper_fails_signature() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
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
                forwarded_author: None,
            },
        )?));
        Ok(())
    })?;
    rehash(&mut events[3])?;
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadSignature { seq: 3 }
    );
    Ok(())
}

#[test]
fn forged_author_fails_signature() -> Result<(), Box<dyn std::error::Error>> {
    let (log, enzo, _) = sample()?;
    let mut events = log.events().to_vec();
    events[2] = rewrite(&events[2], |c| {
        c.author = enzo.id();
        Ok(())
    })?;
    rehash(&mut events[2])?;
    assert_eq!(
        SpaceLog::from_events("sp_paraty", events).unwrap_err(),
        LogError::BadSignature { seq: 2 }
    );
    Ok(())
}

#[test]
fn fields_and_kinds_from_newer_clients_survive_verbatim() -> Result<(), Box<dyn std::error::Error>>
{
    let author = Signer::generate()?;
    let mut log = SpaceLog::new("sp_new");
    log.append(
        &author,
        1,
        EventBody::SpaceCreated {
            title: "t".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?;
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
    let e = event_from_content(content.clone(), sig, None, 0, String::new(), String::new())?;
    assert_eq!(
        e.body,
        EventBody::Unsupported {
            kind: "PollOpened".into()
        }
    );
    let sequenced = log.sequence(e)?.clone();
    assert_eq!(sequenced.content, content);
    let json = serde_json::to_string(log.events())?;
    assert!(SpaceLog::from_events("sp_new", serde_json::from_str(&json)?).is_ok());
    Ok(())
}

#[test]
fn a_sequencer_cannot_move_an_event_before_what_its_author_saw(
) -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, marina) = sample()?;
    let mut relay = SpaceLog::new("sp_paraty");
    for e in &log.events()[..2] {
        relay.accept(e.clone())?;
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
            forwarded_author: None,
        },
    )?;
    let mut forked = SpaceLog::new("sp_paraty");
    forked.accept(log.events()[0].clone())?;
    let mut moved = reply.clone();
    moved.seq = 1;
    moved.prev = forked.head_hash().to_string();
    rehash(&mut moved)?;
    assert_eq!(
        forked.accept(moved).unwrap_err(),
        LogError::BrokenCausalLink { seq: 1 }
    );

    let mut placed = reply;
    placed.seq = 2;
    placed.prev = relay.head_hash().to_string();
    rehash(&mut placed)?;
    assert!(relay.accept(placed).is_ok());
    Ok(())
}

#[test]
fn words_signed_on_one_history_do_not_verify_on_another() -> Result<(), Box<dyn std::error::Error>>
{
    let enzo = Signer::generate()?;
    let mut real = SpaceLog::new("sp_fork");
    real.append(
        &enzo,
        1,
        EventBody::SpaceCreated {
            title: "Real".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?;
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
            forwarded_author: None,
        },
    )?;
    let mut fake = SpaceLog::new("sp_fork");
    fake.accept(real.events()[0].clone())?;
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
            forwarded_author: None,
        },
    )?;
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
            forwarded_author: None,
        },
    )?;
    let mut shown = answer;
    shown.seq = fake.next_seq();
    shown.prev = fake.head_hash().to_string();
    rehash(&mut shown)?;
    assert_eq!(
        fake.accept(shown).unwrap_err(),
        LogError::BrokenCausalLink { seq: 2 }
    );
    Ok(())
}

#[test]
fn only_creation_and_self_join_may_skip_the_causal_link() -> Result<(), Box<dyn std::error::Error>>
{
    let (log, enzo, _) = sample()?;
    let mut client = SpaceLog::new("sp_paraty");
    for e in log.events() {
        client.accept(e.clone())?;
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
            forwarded_author: None,
        },
    )?;
    let mut e = blind;
    e.seq = client.next_seq();
    e.prev = client.head_hash().to_string();
    rehash(&mut e)?;
    assert_eq!(
        client.accept(e).unwrap_err(),
        LogError::MissingCausalLink { seq: 4 }
    );

    let newcomer = Signer::generate()?;
    let join = Author::root(newcomer.clone()).sign_event(
        "sp_paraty",
        "join",
        10,
        None,
        EventBody::MemberAdded {
            identity: newcomer.id(),
            role: Role::Member,
        },
    )?;
    let mut j = join;
    j.seq = client.next_seq();
    j.prev = client.head_hash().to_string();
    rehash(&mut j)?;
    assert!(client.accept(j).is_ok());
    Ok(())
}

#[test]
fn removed_or_reordered_events_are_detected() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
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
    Ok(())
}

#[test]
fn events_from_another_space_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
    assert_eq!(
        SpaceLog::from_events("sp_outro", log.events().to_vec()).unwrap_err(),
        LogError::WrongSpace { seq: 0 }
    );
    Ok(())
}

#[test]
fn device_keys_sign_for_their_identity() -> Result<(), Box<dyn std::error::Error>> {
    let root = Signer::generate()?;
    let phone = Author::device(&root, Signer::generate()?);
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
    )?;
    assert_eq!(e.author, root.id());
    log.sequence(e)?;
    assert!(log.verify().is_ok());

    // A device certified by someone else can't speak for this identity.
    let mallory = Signer::generate()?;
    let fake = Author::certified(
        root.id(),
        Signer::generate()?,
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
            forwarded_author: None,
        },
    )?;
    log.sequence(e)?;
    assert_eq!(
        log.verify().unwrap_err(),
        LogError::BadDeviceCert { seq: 1 }
    );
    Ok(())
}

#[test]
fn relay_sequenced_events_are_accepted_in_order_only() -> Result<(), Box<dyn std::error::Error>> {
    let root = Signer::generate()?;
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
        )?)?
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
                forwarded_author: None,
            },
        )?)?
        .clone();
    let mut client = SpaceLog::new("sp_g");
    assert_eq!(
        client.accept(b.clone()).unwrap_err(),
        LogError::BadSequence {
            expected: 0,
            got: 1
        }
    );
    client.accept(a)?;
    client.accept(b)?;
    assert_eq!(client.head_hash(), relay.head_hash());
    Ok(())
}

#[test]
fn an_end_to_end_space_cannot_be_created_again_as_readable(
) -> Result<(), Box<dyn std::error::Error>> {
    let me = Author::root(Signer::generate()?);
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
        .sequence(created("a", Privacy::EndToEnd, None)?)?
        .clone();
    let seen = relay.head();
    let again = relay
        .sequence(created("b", Privacy::Closed, seen)?)?
        .clone();
    assert_eq!(relay.verify().unwrap_err(), LogError::Recreated { seq: 1 });
    let mut client = SpaceLog::new("sp_g");
    client.accept(first)?;
    assert_eq!(
        client.accept(again).unwrap_err(),
        LogError::Recreated { seq: 1 }
    );
    Ok(())
}

#[test]
fn signer_roundtrips_secret() -> Result<(), Box<dyn std::error::Error>> {
    let s = Signer::generate()?;
    let again = Signer::from_secret(&s.secret());
    assert_eq!(s.id(), again.id());
    assert_eq!(s.id().len(), 64);
    Ok(())
}

#[test]
fn json_hash_is_stable_and_sensitive() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(content_hash(&("a", 1))?, content_hash(&("a", 1))?);
    assert_ne!(content_hash(&("a", 1))?, content_hash(&("a", 2))?);
    Ok(())
}

#[test]
fn a_causal_position_cannot_wrap_to_an_earlier_index() -> Result<(), Box<dyn std::error::Error>> {
    let (log, _, _) = sample()?;
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
    Ok(())
}
