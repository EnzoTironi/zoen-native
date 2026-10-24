//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn colluding_near_claims_do_not_evict_an_honest_far_claim(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-collude";
    start_space(&mut engine, &author, space);
    let honest = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &honest);
    let mut colluders = Vec::new();
    for _ in 0..40 {
        let peer = Author::root(Signer::generate()?);
        add_member(&mut engine, &author, space, &peer);
        colluders.push(peer);
    }
    let near = engine.next_seq(space) + 40 + 8;
    let far = near + 40;
    let honest_hash = "44".repeat(32);
    for (n, peer) in colluders.iter().enumerate() {
        ingest_signed(
            &mut engine,
            peer,
            space,
            claim(near, &format!("{:064x}", n as u64 + 1)),
        );
    }
    ingest_signed(&mut engine, &honest, space, claim(far, &honest_hash));
    let kept = pending(&engine, space);
    assert_eq!(kept.len(), 41);
    assert_eq!(
        kept.get(&device_key(&honest)).map(|item| item.seq),
        Some(far)
    );
    assert_eq!(kept.values().filter(|item| item.seq == near).count(), 40);
    assert_eq!(integrity(&engine, space), None);
    extend_until(&mut engine, &author, space, near);
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&honest))
            .map(|item| item.seq),
        Some(far)
    );
    extend_until(&mut engine, &author, space, far);
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_ne!(hash_at(&engine, space, far), honest_hash);
    assert!(!pending(&engine, space).contains_key(&device_key(&honest)));

    Ok(())
}

pub(super) fn same_device_higher_claim_replaces_the_lower_and_catches_an_earlier_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-higher";
    start_space(&mut engine, &author, space);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    let prefix = hash_at(&engine, space, 0);
    let lower = engine.next_seq(space) + 12;
    let higher = lower + 10;
    let lower_hash = "55".repeat(32);
    let higher_hash = "66".repeat(32);
    ingest_signed(&mut engine, &peer, space, claim(lower, &lower_hash));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&peer))
            .map(|item| item.seq),
        Some(lower)
    );
    ingest_signed(&mut engine, &peer, space, claim(higher, &higher_hash));
    assert_eq!(
        pending(&engine, space).get(&device_key(&peer)),
        Some(&PendingPrefix {
            seq: higher,
            hash: higher_hash.clone(),
            epoch: 1,
            digest: "ab".repeat(32),
        })
    );
    assert_eq!(pending(&engine, space).len(), 1);
    ingest_signed(&mut engine, &peer, space, claim(higher, &"77".repeat(32)));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&peer))
            .map(|item| item.hash.clone()),
        Some(higher_hash.clone())
    );
    ingest_signed(&mut engine, &peer, space, claim(lower, &"77".repeat(32)));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&peer))
            .map(|item| item.seq),
        Some(higher)
    );
    extend_until(&mut engine, &author, space, higher);
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_ne!(hash_at(&engine, space, higher), higher_hash);
    assert_eq!(hash_at(&engine, space, 0), prefix);
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn two_devices_of_one_member_keep_separate_claims(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-devices";
    start_space(&mut engine, &author, space);
    let identity = Signer::generate()?;
    let phone = Author::device(&identity, Signer::generate()?);
    let laptop = Author::device(&identity, Signer::generate()?);
    add_member(&mut engine, &author, space, &phone);
    let near = engine.next_seq(space) + 8;
    let far = near + 8;
    ingest_signed(&mut engine, &phone, space, claim(near, &"a1".repeat(32)));
    ingest_signed(&mut engine, &laptop, space, claim(far, &"b2".repeat(32)));
    let kept = pending(&engine, space);
    assert_eq!(kept.len(), 2);
    assert_ne!(device_key(&phone), device_key(&laptop));
    assert_eq!(
        kept.get(&device_key(&phone)).map(|item| item.seq),
        Some(near)
    );
    assert_eq!(
        kept.get(&device_key(&laptop)).map(|item| item.seq),
        Some(far)
    );
    ingest_signed(&mut engine, &phone, space, claim(far, &"c3".repeat(32)));
    let kept = pending(&engine, space);
    assert_eq!(kept.len(), 2);
    assert_eq!(
        kept.get(&device_key(&phone)).map(|item| item.hash.clone()),
        Some("c3".repeat(32))
    );
    assert_eq!(
        kept.get(&device_key(&laptop)).map(|item| item.hash.clone()),
        Some("b2".repeat(32))
    );

    Ok(())
}

pub(super) fn a_non_member_checkpoint_is_ignored(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-stranger";
    start_space(&mut engine, &author, space);
    ingest_signed(&mut engine, &author, space, post("one"));
    let stranger = Author::root(Signer::generate()?);
    ingest_signed(&mut engine, &stranger, space, claim(0, &"11".repeat(32)));
    ingest_signed(
        &mut engine,
        &stranger,
        space,
        claim(9_000, &"12".repeat(32)),
    );
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(pending(&engine, space).is_empty());
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    ingest_signed(&mut engine, &peer, space, claim(0, &"11".repeat(32)));
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));

    Ok(())
}

pub(super) fn an_honest_conversation_with_many_members_ends_with_no_pending_prefix(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-honest";
    start_space(&mut engine, &author, space);
    let peers: Vec<Author> = (0..12)
        .map(|_| Signer::generate().map(Author::root))
        .collect::<std::result::Result<_, _>>()?;
    for peer in &peers {
        add_member(&mut engine, &author, space, peer);
    }
    for round in 0..3 {
        ingest_signed(&mut engine, &author, space, post(&format!("round-{round}")));
        let upto = engine.logs[space].head().unwrap();
        for peer in &peers {
            ingest_signed(
                &mut engine,
                peer,
                space,
                checkpoint(upto.clone(), &"ab".repeat(32)),
            );
            assert_eq!(integrity(&engine, space), None);
            assert!(pending(&engine, space).is_empty());
        }
    }
    assert_eq!(engine.logs[space].events().len(), 1 + 12 + 3 + 36);
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn a_removed_member_pending_claim_is_kept_until_the_chain_reaches_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-removed";
    start_space(&mut engine, &author, space);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    let target = engine.next_seq(space) + 6;
    ingest_signed(&mut engine, &peer, space, claim(target, &"88".repeat(32)));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&peer))
            .map(|item| item.seq),
        Some(target)
    );
    ingest_signed(
        &mut engine,
        &author,
        space,
        EventBody::MemberRemoved {
            identity: peer.identity.clone(),
        },
    );
    ingest_signed(
        &mut engine,
        &peer,
        space,
        claim(target + 20, &"99".repeat(32)),
    );
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&peer))
            .map(|item| item.seq),
        Some(target)
    );
    assert_eq!(pending(&engine, space).len(), 1);
    assert_eq!(integrity(&engine, space), None);
    extend_until(&mut engine, &author, space, target);
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn pending_prefix_ceiling_refuses_a_new_device_without_evicting(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-ceiling";
    start_space(&mut engine, &author, space);
    let held = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &held);
    let mut planted = BTreeMap::new();
    planted.insert(
        device_key(&held),
        PendingPrefix {
            seq: 40_000,
            hash: "11".repeat(32),
            epoch: 1,
            digest: "ab".repeat(32),
        },
    );
    for n in 1..PENDING_PREFIX_CEILING {
        planted.insert(
            format!("device-{n:04}"),
            PendingPrefix {
                seq: 40_000,
                hash: format!("{n:064x}"),
                epoch: 1,
                digest: "ab".repeat(32),
            },
        );
    }
    assert_eq!(planted.len(), PENDING_PREFIX_CEILING);
    engine
        .store
        .set_meta(
            &pending_prefix_meta(space),
            &serde_json::to_string(&planted).unwrap(),
        )
        .unwrap();
    ingest_signed(&mut engine, &held, space, claim(50_000, &"22".repeat(32)));
    assert_eq!(
        pending(&engine, space)
            .get(&device_key(&held))
            .map(|item| item.seq),
        Some(50_000)
    );
    assert_eq!(pending(&engine, space).len(), PENDING_PREFIX_CEILING);
    let extra = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &extra);
    ingest_signed(&mut engine, &extra, space, claim(50_001, &"aa".repeat(32)));
    let kept = pending(&engine, space);
    assert_eq!(kept.len(), PENDING_PREFIX_CEILING);
    assert!(!kept.contains_key(&device_key(&extra)));
    assert_eq!(
        kept.get(&device_key(&held)).map(|item| item.seq),
        Some(50_000)
    );
    assert_eq!(kept.get("device-0001").map(|item| item.seq), Some(40_000));
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.pending_prefix_refusals(space), 1);

    Ok(())
}
