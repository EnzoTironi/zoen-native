use super::*;

pub(super) async fn concurrent_writers_on_one_space_build_one_gapless_chain() {
    let log = store();
    let a = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&log, &a).await;
    let mut tasks = Vec::new();
    for w in 0..8 {
        let (log, a, space, genesis) = (log.clone(), a.clone(), space.clone(), genesis.clone());
        tasks.push(tokio::spawn(async move {
            for i in 0..40 {
                let env = sign(&a, &space, Some(genesis.clone()), message(w * 100 + i));
                assert!(matches!(
                    log.append(&env, true).await,
                    Ok(Sequencing::New { .. })
                ));
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let events = read_all(&log, &space, 1000).await;
    assert_eq!(events.len(), 321);
    assert_chain(&space, &events);
    log.drop_cell().await.unwrap();
}

pub(super) async fn catch_up_returns_every_event_past_partial_batches() {
    let log = store();
    let a = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &a).await;
    for i in 0..1200 {
        log.append(&sign(&a, &space, Some(genesis.clone()), message(i)), true)
            .await
            .unwrap();
    }
    let events = read_all(&log, &space, 500).await;
    assert_eq!(
        events.len(),
        1201,
        "pages of 500 come back full until the end"
    );
    assert_chain(&space, &events);
    assert_eq!(log.read(&space, 1100, 500).await.unwrap().len(), 101);
    log.drop_cell().await.unwrap();
}

pub(super) async fn a_retried_envelope_is_answered_with_the_stored_copy() {
    let log = store();
    let a = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &a).await;
    let env = sign(&a, &space, Some(genesis), message(0));
    let Ok(Sequencing::New { ev: first, .. }) = log.append(&env, true).await else {
        panic!()
    };
    let Ok(Sequencing::Duplicate { ev, .. }) = log.append(&env, true).await else {
        panic!("not deduplicated")
    };
    assert_eq!((ev.seq, ev.hash), (first.seq, first.hash));
    assert_eq!(read_all(&log, &space, 500).await.len(), 2);
    log.drop_cell().await.unwrap();
}

pub(super) async fn membership_changes_commit_with_the_event_that_makes_them() {
    let log = store();
    let ana = Author::root(Signer::generate());
    let bruno = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &ana).await;
    let r = log
        .append(
            &sign(&bruno, &space, Some(genesis.clone()), message(0)),
            true,
        )
        .await;
    assert!(r.is_err_and(|r| r.reason.contains("not a member")));

    let add = EventBody::MemberAdded {
        identity: bruno.identity.clone(),
        role: Role::Member,
    };
    let Ok(Sequencing::New {
        audience, joined, ..
    }) = log
        .append(&sign(&ana, &space, Some(genesis.clone()), add), true)
        .await
    else {
        panic!("add refused")
    };
    assert_eq!(joined.as_deref(), Some(bruno.identity.as_str()));
    assert!(audience.contains(&bruno.identity) && audience.contains(&ana.identity));
    assert_eq!(
        log.role(&space, &bruno.identity).await.unwrap(),
        Some(Role::Member)
    );
    assert_eq!(
        log.spaces_of(&bruno.identity).await.unwrap(),
        vec![space.clone()]
    );
    assert!(log
        .append(
            &sign(&bruno, &space, Some(genesis.clone()), message(1)),
            true
        )
        .await
        .is_ok());

    let remove = EventBody::MemberRemoved {
        identity: bruno.identity.clone(),
    };
    let Ok(Sequencing::New { audience, .. }) = log
        .append(&sign(&ana, &space, Some(genesis.clone()), remove), true)
        .await
    else {
        panic!("remove refused")
    };
    assert!(
        audience.contains(&bruno.identity),
        "the removed member learns it"
    );
    assert!(log.spaces_of(&bruno.identity).await.unwrap().is_empty());
    assert!(log
        .append(&sign(&bruno, &space, Some(genesis), message(2)), true)
        .await
        .is_err());
    log.drop_cell().await.unwrap();
}

pub(super) async fn invites_are_bounded_by_uses() {
    let log = store();
    let ana = Author::root(Signer::generate());
    let (space, _) = create(&log, &ana).await;
    let inv = log
        .create_invite(&ana.identity, &space, Role::Member, 1, 3600)
        .await
        .unwrap();
    let preview = log.preview_invite(&inv.code).await.unwrap();
    assert_eq!(
        (preview.space.as_str(), preview.members),
        (space.as_str(), 1)
    );

    let join = |who: &Author| {
        let mut env = sign(
            who,
            &space,
            None,
            EventBody::MemberAdded {
                identity: who.identity.clone(),
                role: Role::Member,
            },
        );
        env.invite = Some(inv.code.to_lowercase());
        env
    };
    let carla = Author::root(Signer::generate());
    assert!(matches!(
        log.append(&join(&carla), true).await,
        Ok(Sequencing::New { .. })
    ));
    let dani = Author::root(Signer::generate());
    assert!(
        log.append(&join(&dani), true).await.is_err(),
        "one use only"
    );
    assert!(log.preview_invite(&inv.code).await.is_err());
    let stored = read_all(&log, &space, 500).await;
    assert!(
        stored.iter().all(|e| e.env.invite.is_none()),
        "codes are never stored"
    );
    log.drop_cell().await.unwrap();
}

pub(super) async fn two_storage_clients_share_one_chain_and_one_membership() {
    let cell = types::new_id("t");
    let relays = [store_on(&cell), store_on(&cell)];
    let ana = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&relays[0], &ana).await;
    let mut tasks = Vec::new();
    for w in 0..8 {
        let log = relays[w % 2].clone();
        let (a, space, genesis) = (ana.clone(), space.clone(), genesis.clone());
        tasks.push(tokio::spawn(async move {
            for i in 0..40 {
                let env = sign(&a, &space, Some(genesis.clone()), message(w * 100 + i));
                assert!(matches!(
                    log.append(&env, true).await,
                    Ok(Sequencing::New { .. })
                ));
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let events = read_all(&relays[1], &space, 1000).await;
    assert_eq!(events.len(), 321);
    assert_chain(&space, &events);

    // Both relays now hold a warm cache of the Space. A change through one must bind the
    // other.
    let bruno = Author::root(Signer::generate());
    let add = EventBody::MemberAdded {
        identity: bruno.identity.clone(),
        role: Role::Member,
    };
    relays[0]
        .append(&sign(&ana, &space, Some(genesis.clone()), add), true)
        .await
        .unwrap();
    assert!(
        relays[1]
            .append(
                &sign(&bruno, &space, Some(genesis.clone()), message(1)),
                true
            )
            .await
            .is_ok(),
        "the other relay sees the new member"
    );
    let remove = EventBody::MemberRemoved {
        identity: bruno.identity.clone(),
    };
    relays[1]
        .append(&sign(&ana, &space, Some(genesis.clone()), remove), true)
        .await
        .unwrap();
    let r = relays[0]
        .append(&sign(&bruno, &space, Some(genesis), message(2)), true)
        .await;
    assert!(
        r.is_err_and(|r| r.reason.contains("not a member")),
        "and the removal"
    );
    assert_chain(&space, &read_all(&relays[0], &space, 1000).await);
    relays[0].drop_cell().await.unwrap();
}

pub(super) async fn duplicates_in_one_batch_get_the_one_stored_copy() {
    // On a single-threaded runtime the Space's worker can't run until every copy below is
    // queued, so they meet in one batch every time.
    tokio::task::spawn_blocking(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(duplicates_in_one_batch())
    })
    .await
    .unwrap();
}

pub(super) async fn duplicates_in_one_batch() {
    let log = store();
    let a = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&log, &a).await;
    // Each copy is queued on its first poll, and `join_all` polls them all before yielding.
    let twin = sign(&a, &space, Some(genesis.clone()), message(0));
    let copies = futures_util::future::join_all((0..16).map(|_| log.append(&twin, true))).await;
    let mut new = 0;
    let mut stored = Vec::new();
    for c in copies {
        match c {
            Ok(Sequencing::New { ev, .. }) => {
                new += 1;
                stored.push((ev.seq, ev.hash));
            }
            Ok(Sequencing::Duplicate { ev, .. }) => stored.push((ev.seq, ev.hash)),
            Err(r) => panic!("refused: {}", r.reason),
        }
    }
    assert_eq!(new, 1, "one copy is sequenced");
    assert!(
        stored.windows(2).all(|w| w[0] == w[1]),
        "every copy answers with it"
    );
    let events = read_all(&log, &space, 1000).await;
    assert_eq!(events.len(), 2);
    assert_chain(&space, &events);
    log.drop_cell().await.unwrap();
}
