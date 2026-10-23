//! Real FDB membership and history must use one read version per page.
use super::*;

async fn append(log: &FdbLog, owner: &Author, space: &str, body: EventBody) -> Sequenced {
    let previous = log.read(space, 0, 100).await.unwrap().pop().unwrap();
    match log
        .append(
            &sign(
                owner,
                space,
                Some(Seen {
                    seq: previous.seq,
                    hash: previous.hash,
                }),
                body,
            ),
            true,
        )
        .await
        .unwrap()
    {
        Sequencing::New { ev, .. } => ev,
        _ => panic!("new canonical history event"),
    }
}

pub(super) async fn removal_clamps_each_history_page_and_rejoin_restores_access() {
    let log = store();
    let owner = Author::root(Signer::generate());
    let reader = Author::root(Signer::generate());
    let (space, _) = create(&log, &owner).await;
    assert!(log
        .read_visible(&space, &reader.identity, 0, 3)
        .await
        .unwrap()
        .is_none());
    append(
        &log,
        &owner,
        &space,
        EventBody::MemberAdded {
            identity: reader.identity.clone(),
            role: Role::Member,
        },
    )
    .await;
    for i in 0..4 {
        append(&log, &owner, &space, message(i)).await;
    }
    let first = log
        .read_visible(&space, &reader.identity, 0, 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(first.through, None);
    let removed = append(
        &log,
        &owner,
        &space,
        EventBody::MemberRemoved {
            identity: reader.identity.clone(),
        },
    )
    .await;
    let private = append(&log, &owner, &space, message(99)).await;
    assert_eq!(removed.seq, 6);
    assert_eq!(private.seq, 7);
    // The old unqualified read contains the tail. A stale Session plan must
    // not carry that authority into its next page after removal commits.
    let raw = log.read(&space, 6, 3).await.unwrap();
    assert_eq!(raw.len(), 2);
    let middle = log
        .read_visible(&space, &reader.identity, 3, 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(middle.through, Some(6));
    assert_eq!(
        middle.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    let last = log
        .read_visible(&space, &reader.identity, 6, 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last.through, Some(6));
    assert_eq!(last.events.len(), 1);
    assert_eq!(
        last.events[0].encode(),
        removed.encode(),
        "same canonical removal, not a rewritten view"
    );
    assert!(log
        .read_visible(&space, &reader.identity, 7, 3)
        .await
        .unwrap()
        .unwrap()
        .events
        .is_empty());
    assert!(log
        .read_visible(&space, "never-member", 0, 3)
        .await
        .unwrap()
        .is_none());
    append(
        &log,
        &owner,
        &space,
        EventBody::MemberAdded {
            identity: reader.identity.clone(),
            role: Role::Member,
        },
    )
    .await;
    let rejoined = log
        .read_visible(&space, &reader.identity, 7, 3)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rejoined.through, None);
    assert_eq!(
        rejoined.events[0].encode(),
        private.encode(),
        "current rejoin preserves existing full-history policy"
    );
    assert!(log
        .read_visible(&space, &reader.identity, u64::MAX, 3)
        .await
        .is_err());
    log.drop_cell().await.unwrap();
}
