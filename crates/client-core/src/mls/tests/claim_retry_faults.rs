//! A failed membership read cannot consume the work that makes queued Pages sendable.
use super::*;

fn queued_pages(
) -> std::result::Result<(Engine, Engine, String, Vec<(String, String)>), Box<dyn std::error::Error>>
{
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    owner.net.profiles.supported = true;
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    let space = "owed-read-recovery".to_string();
    for frame in claim_group(&mut owner, &space, std::slice::from_mut(&mut peer))? {
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
    Ok((owner, peer, space, pages))
}

#[test]
fn transient_owed_read_failure_retries_without_losing_original_pages(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut owner, mut peer, space, pages) = queued_pages()?;
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
    let published = relay_ready_to_peer(&mut owner, &mut peer)?;
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

    Ok(())
}

#[test]
fn scoped_owed_storage_failure_keeps_other_chat_publishable_and_recovers(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut owner, mut first_peer, first_space, pages) = queued_pages()?;
    let first_group = owner.mls_status(&first_space);
    let mut second_peer = Engine::open(":memory:").unwrap();
    second_peer
        .create_account("Carla", "carla", "http://relay.test")
        .unwrap();
    let second_space = "owed-independent-chat";
    for frame in claim_group(
        &mut owner,
        second_space,
        std::slice::from_mut(&mut second_peer),
    )? {
        assert_eq!(second_peer.ingest(frame), Ingest::Applied);
    }
    let independent = owner
        .page_import_markdown(
            second_space,
            "independent.md",
            "# Independent\n\nStill sendable.\n",
        )
        .unwrap();
    // This exact derived row has a reliable Space association, but SQLite cannot
    // read its storage class as text. No signed record or provider byte is forged.
    let key = rewelcome_meta(&first_space);
    assert!(owner.store.meta(&key).unwrap().is_none());
    owner
        .store
        .conn()
        .execute(
            "INSERT INTO meta(key,value) VALUES (?1,?2)",
            (&key, vec![0_u8]),
        )
        .unwrap();
    assert!(owner.owed(&first_space).is_err());
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    let (space, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(space, second_space);
    // The other dirty group may have been first in the HashSet. Either iteration
    // order must process the fault without consuming or blocking that valid claim.
    assert!(owner.mls_to_claim(clock).unwrap().is_none());
    assert!(owner.net.mls.retry_at.contains_key(&first_space));
    let due = owner.net.mls.retry_at[&first_space];
    let records = claimed_records(std::slice::from_mut(&mut second_peer), &targets);
    owner.mls_claimed(second_space, &operation, Ok(records));
    for _ in 0..10 {
        for (blob, _) in owner.pending_uploads(100) {
            owner.upload_done(&blob);
        }
        owner.mls_seal_outbox().unwrap();
        let envelopes = owner.outbox_envelopes();
        if envelopes.is_empty() {
            break;
        }
        for env in envelopes {
            assert_eq!(
                env.space(),
                second_space,
                "failed group's pages escaped before repair"
            );
            let frame = ordered(&owner, env)?;
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(second_peer.ingest(frame), Ingest::Applied);
        }
    }
    assert_eq!(
        second_peer.page_markdown(&independent).unwrap(),
        "# Independent\n\nStill sendable.\n"
    );
    assert_eq!(owner.mls_status(&first_space), first_group);
    let retained: Vec<_> = owner
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .map(|row| row.event)
        .collect();
    assert_eq!(retained.len(), pages.len());
    assert!(retained.iter().all(|event| event.space == first_space));
    assert!(owner.mls_to_claim(clock).unwrap().is_none());
    assert_eq!(
        owner.net.mls.retry_at[&first_space], due,
        "an early maintenance pass moved the original retry deadline"
    );
    assert_eq!(owner.net.mls.retry_at.len(), 1);
    owner.store.meta_delete(&key).unwrap();
    owner
        .net
        .mls
        .retry_at
        .insert(first_space.clone(), Instant::now());
    let (space, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    assert_eq!(space, first_space);
    let records = claimed_records(std::slice::from_mut(&mut first_peer), &targets);
    owner.mls_claimed(&first_space, &operation, Ok(records));
    let published = relay_ready_to_peer(&mut owner, &mut first_peer)?;
    for (id, markdown) in pages {
        assert_eq!(first_peer.page_markdown(&id).unwrap(), markdown);
        let original = retained
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
    assert_eq!(owner.outbox_len(), 0);
    assert_eq!(owner.net.mls.retry_at.len(), 0);
    for space in [first_space.as_str(), second_space] {
        owner.logs[space].verify().unwrap();
    }

    Ok(())
}
