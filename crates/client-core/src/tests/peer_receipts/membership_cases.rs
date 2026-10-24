//! Complete membership scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn unknown_foreign_removed_and_inaccessible_receipts_never_write() {
    let (_db, e, c) = fixture();
    let before = snapshot(&e);
    for (space, peer) in [
        ("missing-space", c.peer.as_str()),
        (c.space.as_str(), "unknown-person"),
        (c.space.as_str(), c.outsider.as_str()),
    ] {
        assert!(!e.lock().receipt(space, peer, u64::MAX, u64::MAX).unwrap());
        assert_eq!(snapshot(&e), before);
    }
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.peer.clone(),
        },
    );
    let before = snapshot(&e);
    assert!(!e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(snapshot(&e), before);
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.peer.clone(),
            role: Role::Member,
        },
    );
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.me.clone(),
        },
    );
    let before = snapshot(&e);
    for peer in [&c.peer, &c.me] {
        assert!(!e
            .lock()
            .receipt(&c.space, peer, u64::MAX, u64::MAX)
            .unwrap());
        assert_eq!(snapshot(&e), before);
    }
}

pub(super) fn early_receipts_survive_sync_boundaries_until_membership_and_foreign_pairs_stay_bounded(
) {
    let (_db, e, c) = fixture();
    let mut deferred = crate::net::DeferredReceipts::default();
    let mut dirty = std::collections::HashSet::new();
    let before = snapshot(&e);
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, c.voice.seq, 0)
        .unwrap());
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    // Neither a partial nor a full wire SyncDone proves every room's dependencies
    // have landed; a future member's pair must survive both boundaries.
    for _ in 0..2 {
        deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    }
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.outsider.clone(),
            role: Role::Member,
        },
    );
    let before_flush = snapshot(&e);
    e.lock().store.conn().execute_batch(
        "CREATE TEMP TRIGGER refuse_deferred BEFORE INSERT ON peer_receipts BEGIN SELECT RAISE(ABORT, 'synthetic deferred failure'); END;",
    ).unwrap();
    assert!(matches!(
        deferred.flush(&mut e.lock(), &mut dirty),
        Err(CoreError::Storage { .. })
    ));
    assert!(dirty.is_empty());
    assert_eq!(snapshot(&e), before_flush);
    e.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_deferred")
        .unwrap();
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(dirty, std::collections::HashSet::from([c.space.clone()]));
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.outsider],
        (c.voice.seq, c.voice.seq)
    );
    assert_eq!(
        snapshot(&e).rows,
        vec![(
            c.space.clone(),
            c.outsider.clone(),
            format!("{:020}", c.voice.seq),
            format!("{:020}", c.voice.seq)
        )]
    );

    let before = snapshot(&e);
    let mut dirty = std::collections::HashSet::new();
    assert!(!deferred
        .receive(
            &mut e.lock(),
            &c.space,
            "foreign-person",
            u64::MAX,
            u64::MAX
        )
        .unwrap());
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
    // A fresh socket has 1024 bounded slots. Unknown pairs never reach storage;
    // overflow requests reconnect instead of silently evicting a legitimate pair.
    let mut deferred = crate::net::DeferredReceipts::default();
    for index in 0..1024 {
        assert!(!deferred
            .receive(&mut e.lock(), &c.space, &format!("unknown-{index}"), 1, 0)
            .unwrap());
    }
    assert!(matches!(
        deferred.receive(&mut e.lock(), &c.space, "overflow", 1, 0),
        Err(CoreError::Invalid { .. })
    ));
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(snapshot(&e), before);
    assert!(dirty.is_empty());
}

pub(super) fn pending_membership_cannot_admit_a_new_peer_or_suppress_a_confirmed_peers_receipt() {
    let (_db, e, c) = fixture();
    e.lock()
        .add_member(&c.space, &c.outsider, Role::Member)
        .unwrap();
    let before = snapshot(&e);
    assert!(!e
        .lock()
        .receipt(&c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    assert_eq!(snapshot(&e), before);
    let mut deferred = crate::net::DeferredReceipts::default();
    let mut dirty = std::collections::HashSet::new();
    assert!(!deferred
        .receive(&mut e.lock(), &c.space, &c.outsider, 0, c.voice.seq)
        .unwrap());
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert!(dirty.is_empty());
    assert_eq!(snapshot(&e), before);
    {
        let mut engine = e.lock();
        let pending = engine.store.outbox().unwrap().into_iter().find(|row| {
            matches!(&row.event.body, EventBody::MemberAdded { identity, .. } if identity == &c.outsider)
        }).unwrap().event;
        let mut relay = engine.logs[&c.space].clone();
        let event = relay.sequence(pending).clone();
        assert_eq!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event),
            }),
            crate::sync::Ingest::Confirmed
        );
    }
    deferred.flush(&mut e.lock(), &mut dirty).unwrap();
    assert_eq!(dirty, std::collections::HashSet::from([c.space.clone()]));
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.outsider],
        (c.voice.seq, c.voice.seq)
    );

    e.lock().remove_member(&c.space, &c.peer).unwrap();
    assert!(!e.lock().state.spaces[&c.space]
        .members
        .iter()
        .any(|(id, _)| id == &c.peer));
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (u64::MAX, u64::MAX)
    );
    assert_eq!(snapshot(&e).rows.len(), 2);
    assert_delivery(&e, &c, Delivery::Read);
}

pub(super) fn confirmed_removal_discards_new_and_deferred_marks_so_rejoin_cannot_authorize_them(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for remove_reader in [false, true] {
        let (_db, e, c) = fixture();
        membership(
            &e,
            &c,
            EventBody::MemberAdded {
                identity: c.peer.clone(),
                role: Role::Owner,
            },
        );
        assert!(e.lock().receipt(&c.space, &c.peer, 10, 5).unwrap());
        let historical = snapshot(&e).rows;
        let mut deferred = crate::net::DeferredReceipts::default();
        let mut dirty = std::collections::HashSet::new();
        let removed = if remove_reader {
            c.me.clone()
        } else {
            c.peer.clone()
        };
        let actor = if remove_reader { &c.peer } else { &c.me };
        let transition = |body| {
            let mut engine = e.lock();
            let signed = engine.author_for(actor).unwrap().sign_event(
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
                    env: protocol::Envelope::plain(&event)
                }),
                crate::sync::Ingest::Applied
            );
        };
        transition(EventBody::MemberRemoved {
            identity: removed.clone(),
        });
        assert!(!deferred
            .receive(&mut e.lock(), &c.space, &c.peer, u64::MAX, u64::MAX)
            .unwrap());
        deferred.flush(&mut e.lock(), &mut dirty).unwrap();
        assert_eq!(
            snapshot(&e).rows,
            historical,
            "Old authorized historical row is retained; new arrival never persists"
        );
        transition(EventBody::MemberAdded {
            identity: removed,
            role: Role::Owner,
        });
        deferred.flush(&mut e.lock(), &mut dirty).unwrap();
        assert_eq!(
            snapshot(&e).rows,
            historical,
            "Rejoin cannot authorize the discarded pre-rejoin arrival"
        );
        assert!(!dirty.contains(&c.space));
        assert!(deferred
            .receive(&mut e.lock(), &c.space, &c.peer, 12, 11)
            .unwrap());
        assert_eq!(snapshot(&e).rows[0].2, format!("{:020}", 12));
        assert_eq!(snapshot(&e).rows[0].3, format!("{:020}", 11));
    })
}
