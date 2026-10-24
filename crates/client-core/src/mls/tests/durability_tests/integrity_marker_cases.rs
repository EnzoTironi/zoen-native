//! Unknown integrity is neither a healthy conversation nor proof of a fork.
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unreadable_marker(engine: &Engine, space: &str) -> TestResult {
    let key = fork_meta(space);
    engine.store.conn().execute(
        "INSERT INTO meta(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key, vec![0xff_u8]],
    )?;
    assert!(
        engine.store.meta(&key).is_err(),
        "real typed SQLite read must fail"
    );
    Ok(())
}

fn pending_json(engine: &Engine, id: &str) -> rusqlite::Result<String> {
    engine
        .store
        .conn()
        .query_row("SELECT json FROM outbox WHERE client_id=?1", [id], |row| {
            row.get(0)
        })
}

#[test]
fn unreadable_integrity_marker_parks_exact_ciphertext_without_blocking_another_space() -> TestResult
{
    let database = Database::new();
    let space = "unreadable-integrity-ciphertext";
    let healthy = "readable-independent-space";
    let (mut owner, owner_secret, mut peer, peer_secret) = encrypted_pair(&database, space);
    let peer_author = peer.net.author.clone().ok_or("missing peer author")?;
    closed_space(&mut owner, &peer_author, healthy);
    for event in owner
        .logs
        .get(healthy)
        .ok_or("missing healthy log")?
        .events()
        .to_vec()
    {
        let frame = ordered(&peer, Envelope::plain(&event));
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    let message = owner.send_message(space, "Preserve this exact encrypted intent")?;
    let other = owner.send_message(healthy, "Independent conversation still works")?;
    assert!(owner.mls_seal_outbox()?);
    let pending = owner
        .store
        .outbox_get(&message.id)?
        .ok_or("missing pending intent")?;
    let original = owner
        .sealed_copy(&pending)
        .ok_or("missing retained ciphertext")?;
    let json = pending_json(&owner, &message.id)?;
    let provider = provider_rows(&owner);
    let count = owner.store.event_count()?;
    unreadable_marker(&owner, space)?;

    let offered = owner.outbox_envelopes();
    assert_eq!(
        offered
            .iter()
            .map(|env| env.client_id())
            .collect::<Vec<_>>(),
        vec![other.id.as_str()],
        "an unreadable integrity marker must not become permission to publish"
    );
    assert_eq!(pending_json(&owner, &message.id)?, json);
    assert_eq!(provider_rows(&owner), provider);
    assert_eq!(owner.store.event_count()?, count);
    let retained = owner
        .sealed_copy(&pending)
        .ok_or("ciphertext lost while blocked")?;
    assert_eq!(retained.0, original.0);
    assert_eq!(retained.1.content(), original.1.content());
    assert_eq!(retained.1.sig, original.1.sig);
    // Cold reconstruction cannot turn the unreadable durable marker into absence.
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &owner_secret);
    assert!(owner.store.meta(&fork_meta(space)).is_err());
    assert_eq!(provider_rows(&owner), provider);
    assert_eq!(pending_json(&owner, &message.id)?, json);
    let offered_after_reopen = owner.outbox_envelopes();
    assert_eq!(
        offered_after_reopen
            .iter()
            .map(|env| env.client_id())
            .collect::<Vec<_>>(),
        vec![other.id.as_str()]
    );
    for envelope in offered_after_reopen {
        let frame = ordered(&owner, envelope);
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    assert_eq!(message_count(&peer, healthy, &other.id), 1);

    owner.store.meta_delete(&fork_meta(space))?;
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &owner_secret);
    assert_eq!(pending_json(&owner, &message.id)?, json);
    let mut offered = owner.outbox_envelopes();
    assert_eq!(offered.len(), 1);
    let envelope = offered.remove(0);
    assert_eq!(envelope.client_id(), message.id);
    assert_eq!(envelope.content(), original.1.content());
    assert_eq!(envelope.sig, original.1.sig);
    let frame = ordered(&owner, envelope);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(peer.ingest(frame), Ingest::Duplicate);
    assert_eq!(owner.outbox_len(), 0);
    drop(peer);
    let peer = reopen(&database.path("peer.sqlite"), &peer_secret);
    assert_eq!(message_count(&peer, space, &message.id), 1);
    assert!(peer.timeline(space)?.iter().any(|row| {
        row.id == message.id
            && matches!(&row.kind, crate::dto::EntryKind::Message { text, .. }
                if text == "Preserve this exact encrypted intent")
    }));
    owner
        .logs
        .get(space)
        .ok_or("missing sender log")?
        .verify()?;
    peer.logs
        .get(space)
        .ok_or("missing receiver log")?
        .verify()?;
    Ok(())
}

#[test]
fn a_recorded_fork_stays_blocked_when_its_marker_becomes_unreadable() -> TestResult {
    let database = Database::new();
    let space = "recorded-fork-marker-read";
    let (mut owner, secret, peer, _) = encrypted_pair(&database, space);
    let message = owner.send_message(space, "Never publish across a known fork")?;
    assert!(owner.mls_seal_outbox()?);
    let pending = owner
        .store
        .outbox_get(&message.id)?
        .ok_or("missing signed intent")?;
    let original = owner
        .sealed_copy(&pending)
        .ok_or("missing retained ciphertext")?;
    let json = pending_json(&owner, &message.id)?;
    let author = peer.net.author.clone().ok_or("missing certified member")?;
    ingest_signed(
        &mut owner,
        &author,
        space,
        checkpoint(
            types::Seen {
                seq: 0,
                hash: "11".repeat(32),
            },
            &"ab".repeat(32),
        ),
    );
    assert_eq!(owner.store.meta(&fork_meta(space))?.as_deref(), Some(FORK));
    assert!(owner.outbox_envelopes().is_empty());
    unreadable_marker(&owner, space)?;
    assert!(
        owner.outbox_envelopes().is_empty(),
        "failed fork-marker read must not release known-fork ciphertext"
    );
    assert_eq!(pending_json(&owner, &message.id)?, json);
    owner.store.set_meta(&fork_meta(space), FORK)?;
    drop(owner);
    let owner = reopen(&database.path("owner.sqlite"), &secret);
    assert!(owner.outbox_envelopes().is_empty());
    assert_eq!(owner.store.meta(&fork_meta(space))?.as_deref(), Some(FORK));
    assert_eq!(pending_json(&owner, &message.id)?, json);
    let retained = owner
        .sealed_copy(&pending)
        .ok_or("fork discarded ciphertext")?;
    assert_eq!(retained.1.content(), original.1.content());
    assert_eq!(retained.1.sig, original.1.sig);
    Ok(())
}

#[test]
fn marker_read_error_refuses_new_message_admission_without_changing_the_journal() -> TestResult {
    let database = Database::new();
    let space = "unreadable-integrity-new-intent";
    let (mut owner, _, _, _) = encrypted_pair(&database, space);
    let before = owner.store.event_count()?;
    let pending = owner.outbox_len();
    unreadable_marker(&owner, space)?;
    assert!(matches!(
        owner.send_message(space, "Do not accept an unknown integrity state"),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(owner.store.event_count()?, before);
    assert_eq!(owner.outbox_len(), pending);
    assert!(owner
        .state
        .spaces
        .get(space)
        .ok_or("missing space")?
        .integrity_error
        .is_none());
    Ok(())
}
