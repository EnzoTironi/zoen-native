//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn direct_seed_waits_for_missing_package_then_reaches_the_same_recipient(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    let space = "direct-seed-delayed-package";
    let frames = claim_space(
        &mut owner,
        space,
        SpaceKind::Direct,
        std::slice::from_mut(&mut peer),
    )?;
    for frame in frames {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    let seed = owner.send_message(space, "original direct seed").unwrap();
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    let (_, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    owner.mls_claimed(space, &operation, Ok(vec![]));
    assert!(owner.net.mls.stuck.contains(space));
    assert_eq!(owner.ready_epoch(space), None);
    assert!(!owner.mls_seal_outbox().unwrap());
    assert!(owner
        .outbox_envelopes()
        .iter()
        .all(|env| env.client_id() != seed.id));
    assert_eq!(owner.outbox_len(), 1);
    assert!(peer.timeline(space).unwrap().is_empty());

    // The existing retry becomes due; the same user message remains queued.
    owner.net.mls.retry_at.insert(space.into(), Instant::now());
    let (_, retry_targets, retry_operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(targets, retry_targets);
    let records = claimed_records(std::slice::from_mut(&mut peer), &retry_targets);
    owner.mls_claimed(space, &retry_operation, Ok(records));
    assert_eq!(
        owner.ready_epoch(space),
        None,
        "commit and Welcome must land first"
    );
    let published = relay_ready_to_peer(&mut owner, &mut peer)?;
    let seed_frame = published
        .iter()
        .find(|frame| frame.env.client_id() == seed.id)
        .unwrap();
    assert_eq!(seed_frame.env.sealed_kind(), Some(SealedKind::Application));
    assert!(::mls::application_epoch(seed_frame.env.sealed_data().unwrap().1).unwrap() > 0);
    let received = peer.timeline(space).unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, seed.id);
    assert!(
        matches!(&received[0].kind, crate::dto::EntryKind::Message { text, .. } if text == "original direct seed")
    );
    owner.logs[space].verify().unwrap();
    peer.logs[space].verify().unwrap();
    Ok(())
}

pub(super) fn group_existing_reader_can_receive_while_another_identity_has_no_package(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peers: Vec<_> = ["bruno", "carla"]
        .into_iter()
        .map(|handle| {
            let mut peer = Engine::open(":memory:").unwrap();
            peer.create_account(handle, handle, "http://relay.test")
                .unwrap();
            peer
        })
        .collect();
    let space = "group-missing-newcomer-package";
    let frames = claim_group(&mut owner, space, &mut peers)?;
    for frame in frames {
        assert_eq!(peers[0].ingest(frame), Ingest::Applied);
    }
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    let (_, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    let records = claimed_records(&mut peers[..1], &targets);
    owner.mls_claimed(space, &operation, Ok(records));
    assert!(owner.net.mls.stuck.contains(space));
    relay_ready_to_peer(&mut owner, &mut peers[0])?;
    assert!(owner.ready_epoch(space).is_some());
    let message = owner
        .send_message(space, "for existing group readers")
        .unwrap();
    relay_ready_to_peer(&mut owner, &mut peers[0])?;
    assert!(peers[0]
        .timeline(space)
        .unwrap()
        .iter()
        .any(|entry| entry.id == message.id));
    assert!(!owner
        .device()
        .unwrap()
        .roster(space)
        .unwrap()
        .contains(peers[1].me.as_ref().unwrap()));
    Ok(())
}

pub(super) fn fifty_one_members_reconcile_in_two_durable_claim_batches(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peers: Vec<Engine> = (0..51)
        .map(|i| {
            let mut peer = Engine::open(":memory:").unwrap();
            peer.create_account(
                &format!("Peer {i}"),
                &format!("peer{i}"),
                "http://relay.test",
            )
            .unwrap();
            peer
        })
        .collect();
    let space = "fifty-one-claim-members";
    let mut frames = claim_group(&mut owner, space, &mut peers)?;
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    let (first_space, first_targets, first_operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(first_space, space);
    assert_eq!(first_targets.len(), 50);
    assert!(first_targets.windows(2).all(|pair| pair[0] < pair[1]));
    owner.mls_claimed(
        space,
        &first_operation,
        Ok(claimed_records(&mut peers, &first_targets)),
    );
    frames.extend(confirm_claim_handshakes(&mut owner)?);
    let (second_space, second_targets, second_operation) =
        owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(second_space, space);
    assert_eq!(second_targets.len(), 1);
    assert!(!first_targets.contains(&second_targets[0]));
    assert_ne!(second_operation, first_operation);
    owner.mls_claimed(
        space,
        &second_operation,
        Ok(claimed_records(&mut peers, &second_targets)),
    );
    frames.extend(confirm_claim_handshakes(&mut owner)?);
    assert!(owner.mls_to_claim(clock).unwrap().is_none());
    assert_eq!(owner.device().unwrap().leaves(space).unwrap().len(), 52);
    assert!(owner.ready_epoch(space).is_some());
    let recipient = peers
        .iter_mut()
        .find(|p| p.me.as_ref() == Some(&second_targets[0]))
        .unwrap();
    for frame in frames {
        assert_eq!(recipient.ingest(frame), Ingest::Applied);
    }
    assert_eq!(
        recipient.device().unwrap().checkpoint(space).unwrap(),
        owner.device().unwrap().checkpoint(space).unwrap()
    );
    Ok(())
}

pub(super) fn unsupported_claims_in_one_chat_do_not_block_another_chats_removal(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    let peer_identity = peer.me.clone().unwrap();
    for space in ["fallback-one", "fallback-two"] {
        claim_group(&mut owner, space, std::slice::from_mut(&mut peer))?;
        let records = claimed_records(
            std::slice::from_mut(&mut peer),
            std::slice::from_ref(&peer_identity),
        );
        owner
            .commit_now(space, &[records[0].data.clone()], &Default::default())
            .unwrap();
        confirm_claim_handshakes(&mut owner)?;
    }
    owner.net.mls.dirty = ["fallback-one".into(), "fallback-two".into()]
        .into_iter()
        .collect();
    let adding = owner.net.mls.dirty.iter().next().unwrap().clone();
    let removing = owner
        .net
        .mls
        .dirty
        .iter()
        .find(|s| **s != adding)
        .unwrap()
        .clone();
    let author = owner.net.author.clone().unwrap();
    let mut newcomer = Engine::open(":memory:").unwrap();
    newcomer
        .create_account("Carol", "carol", "http://relay.test")
        .unwrap();
    for (space, body) in [
        (
            adding.as_str(),
            EventBody::MemberAdded {
                identity: newcomer.me.clone().unwrap(),
                role: Role::Member,
            },
        ),
        (
            removing.as_str(),
            EventBody::MemberRemoved {
                identity: peer_identity.clone(),
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(
            space,
            &format!("change-{space}"),
            2,
            seen(&owner, space),
            body,
        )?);
        assert_eq!(owner.ingest(ordered(&owner, env)?), Ingest::Applied);
    }
    assert_eq!(
        owner.net.mls.dirty.iter().next(),
        Some(&adding),
        "blocked addition must precede removal in this regression"
    );
    assert!(owner.mls_to_claim(None).is_err());
    assert!(owner.net.mls.dirty.contains(&adding));
    assert!(!owner.device().unwrap().pending(&adding));
    assert!(
        owner.device().unwrap().pending(&removing),
        "unrelated removal was never staged"
    );
    confirm_claim_handshakes(&mut owner)?;
    assert_eq!(owner.device().unwrap().leaves(&removing).unwrap().len(), 1);
    assert!(owner.ready_epoch(&removing).is_some());
    Ok(())
}
