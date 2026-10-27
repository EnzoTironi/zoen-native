//! Same signed queue, real SQLite/OpenMLS, cold retry and causal fencing.
use super::*;

pub(super) fn cold_outbox_fifo_preserves_signed_intents_ciphertext_and_receiver_order_after_clock_rollback(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let space = "fifo-backwards-clock";
    let (mut owner, secret, mut peer, _) = encrypted_pair(&database, space)?;
    let author = owner.net.author.clone().unwrap();
    let queued = [("first", 300), ("second", 100), ("third", 100)]
        .into_iter()
        .map(|(text, at)| owner.append_synced(space, &author, at, post(text)).unwrap())
        .collect::<Vec<_>>();
    let ids = queued
        .iter()
        .map(|e| e.client_id.clone())
        .collect::<Vec<_>>();
    owner
        .store
        .outbox_note(&ids[0], "lost connection", false)
        .unwrap();
    owner.store.outbox_put(&queued[0]).unwrap();
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    assert_eq!(
        owner
            .store
            .outbox()
            .unwrap()
            .iter()
            .map(|p| &p.event)
            .collect::<Vec<_>>(),
        queued.iter().collect::<Vec<_>>()
    );
    assert!(owner.mls_seal_outbox().unwrap());
    let original = owner.outbox_envelopes();
    assert_eq!(
        original.iter().map(|e| e.client_id()).collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    let ratchet = provider_rows(&owner);
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    assert!(!owner.mls_seal_outbox().unwrap());
    assert_eq!(owner.outbox_envelopes(), original);
    assert_eq!(provider_rows(&owner), ratchet);
    for envelope in original {
        let frame = ordered(&owner, envelope)?;
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(peer.ingest(frame), Ingest::Duplicate);
    }
    let received = peer.logs[space]
        .events()
        .iter()
        .filter(|e| ids.contains(&e.client_id))
        .collect::<Vec<_>>();
    assert_eq!(
        received
            .iter()
            .map(|e| e.client_id.as_str())
            .collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    for expected in queued {
        let actual = received
            .iter()
            .find(|e| e.client_id == expected.client_id)
            .unwrap();
        assert_eq!(actual.content, expected.content);
        assert_eq!(actual.sig, expected.sig);
        if matches!(&expected.body, EventBody::MessagePosted { .. }) {
            // Timeline IDs are stable client IDs, not the inner message field.
            assert_eq!(message_count(&peer, space, &expected.client_id), 1);
        } else {
            panic!("expected real application message");
        }
    }
    assert_eq!(owner.store.outbox_len().unwrap(), 0);
    peer.logs[space].verify().unwrap();

    Ok(())
}

pub(super) fn backward_clock_welcome_stays_behind_original_commit_and_other_space_advances(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let space = "fifo-handshake";
    let (mut owner, secret, peer, _) = encrypted_pair(&database, space)?;
    // encrypted_pair already added peer's signature key. A different key package
    // for that same leaf cannot add it again; stage a genuinely new member.
    let (newcomer, newcomer_author, _) = member(&database.path("newcomer.sqlite"));
    let author = owner.net.author.clone().unwrap();
    add_member(&mut owner, &author, space, &newcomer_author)?;
    let package = newcomer
        .device()
        .unwrap()
        .key_packages(1, false)
        .unwrap()
        .remove(0);
    owner
        .commit_now(space, &[package], &Default::default())
        .unwrap();
    let handshakes = owner.store.outbox().unwrap();
    let mut commit = None;
    let mut welcome = None;
    for pending in handshakes {
        let original = pending.event;
        let parsed = event_log::content::SignedContent::parse(&original.content).unwrap();
        let Some(event_log::content::Payload::Sealed(sealed)) = parsed.payload else {
            continue;
        };
        let at = if sealed.kind == SealedKind::Commit as i32 {
            commit = Some(original.client_id.clone());
            300
        } else if sealed.kind == SealedKind::Welcome as i32 {
            welcome = Some(original.client_id.clone());
            100
        } else {
            continue;
        };
        let (content, sig) = author.sign_sealed(
            space,
            &original.client_id,
            at,
            original.seen.as_ref(),
            sealed,
        );
        let event =
            event_from_content(content, sig, original.cert, 0, String::new(), String::new())
                .unwrap();
        owner
            .store
            .conn()
            .execute(
                "UPDATE outbox SET json=?1,created_ms=?2 WHERE client_id=?3",
                rusqlite::params![serde_json::to_string(&event).unwrap(), at, event.client_id],
            )
            .unwrap();
    }
    let commit = commit.unwrap();
    let welcome = welcome.unwrap();
    let other = "independent-space";
    closed_space(&mut owner, &peer.net.author.clone().unwrap(), other)?;
    let independent = owner
        .append_synced(other, &author, 50, post("other"))
        .unwrap();
    for (sha, _) in owner.pending_uploads(100) {
        owner.upload_done(&sha);
    }
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    let offers = owner.outbox_envelopes();
    assert!(offers.iter().any(|e| e.client_id() == commit));
    assert!(!offers.iter().any(|e| e.client_id() == welcome));
    assert!(offers
        .iter()
        .any(|e| e.client_id() == independent.client_id));
    let envelope = offers
        .into_iter()
        .find(|e| e.client_id() == commit)
        .unwrap();
    assert_eq!(owner.ingest(ordered(&owner, envelope)?), Ingest::Confirmed);
    assert!(owner
        .outbox_envelopes()
        .iter()
        .any(|e| e.client_id() == welcome));
    owner
        .store
        .outbox_note(&welcome, "terminal test refusal", true)
        .unwrap();
    assert!(!owner
        .outbox_envelopes()
        .iter()
        .any(|e| e.client_id() == welcome));
    assert!(owner
        .outbox_envelopes()
        .iter()
        .any(|e| e.client_id() == independent.client_id));

    Ok(())
}
