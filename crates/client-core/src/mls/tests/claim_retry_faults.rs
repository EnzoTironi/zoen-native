//! A failed membership read cannot consume the work that makes queued Pages sendable.
use super::*;

fn queued_pages() -> (Engine, Engine, String, Vec<(String, String)>) {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    let space = "owed-read-recovery".to_string();
    for frame in claim_group(&mut owner, &space, std::slice::from_mut(&mut peer)) {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    let pages = [
        "# Original\n\nFirst page.\n",
        "# Outra\n\nSegunda página.\n",
    ]
    .into_iter()
    .enumerate()
    .map(|(n, markdown)| {
        let id = owner
            .page_import_markdown(&space, &format!("{n}.md"), markdown)
            .unwrap();
        (id, markdown.to_string())
    })
    .collect();
    assert!(owner.owed(&space).unwrap().is_some());
    assert!(owner.outbox_envelopes().is_empty());
    (owner, peer, space, pages)
}

#[test]
fn transient_owed_read_failure_retries_without_losing_original_pages() {
    let (mut owner, mut peer, space, pages) = queued_pages();
    let original: Vec<_> = owner
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|row| row.event)
        .collect();
    let group = owner.mls_status(&space);
    // The existing completed admin-turn marker must survive a storage failure too.
    owner.net.mls.waited.insert(space.clone());
    // Real SQLite query failure; the exact table and its signed rows remain intact.
    owner
        .store
        .conn()
        .execute_batch("ALTER TABLE outbox RENAME TO held_outbox")
        .unwrap();
    assert!(
        owner.owed(&space).is_err(),
        "fault must reach the actual owed read"
    );
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    assert!(owner.mls_to_claim(clock).unwrap().is_none());
    assert!(
        owner.net.mls.retry_at.contains_key(&space),
        "owed failure silently lost the only membership work"
    );
    assert!(
        owner.net.mls.waited.contains(&space),
        "storage failure reset an already completed admin turn"
    );
    let retry = owner.net.mls.retry_at[&space];
    assert!(retry > Instant::now());
    assert!(!owner.net.mls.dirty.contains(&space));
    assert_eq!(owner.mls_status(&space), group);
    owner
        .store
        .conn()
        .execute_batch("ALTER TABLE held_outbox RENAME TO outbox")
        .unwrap();
    assert!(
        owner.mls_to_claim(clock).unwrap().is_none(),
        "no hot-loop retry before the existing backoff is due"
    );
    assert_eq!(owner.net.mls.retry_at[&space], retry);
    assert_eq!(owner.net.mls.retry_at.len(), 1);
    assert_eq!(
        owner
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        original
    );
    assert!(owner.outbox_envelopes().is_empty());

    // Drive the existing scheduler's due transition, without sleeping or changing a timeout.
    owner.net.mls.retry_at.insert(space.clone(), Instant::now());
    let (claimed_space, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(claimed_space, space);
    let records = claimed_records(std::slice::from_mut(&mut peer), &targets);
    owner.mls_claimed(&space, &operation, Ok(records));
    let published = relay_ready_to_peer(&mut owner, &mut peer);
    for (id, markdown) in pages {
        assert_eq!(peer.page_markdown(&id).unwrap(), markdown);
        let original = original
            .iter()
            .find(|event| matches!(&event.body, EventBody::ItemCreated { item, .. } if item == &id))
            .unwrap();
        assert_eq!(
            published
                .iter()
                .filter(|frame| frame.env.client_id() == original.client_id)
                .count(),
            1
        );
    }
    assert!(owner.ready_epoch(&space).is_some());
    assert_eq!(owner.outbox_len(), 0);
    assert_eq!(owner.net.mls.retry_at.len(), 0);
    owner.logs[&space].verify().unwrap();
    peer.logs[&space].verify().unwrap();
}
