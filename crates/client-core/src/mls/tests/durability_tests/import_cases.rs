//! Cold queued Pages use the same ciphertext and publication window after ACK loss.
use super::*;

#[test]
fn hundred_page_import_cold_reopen_and_lost_confirmation_keep_exact_originals() -> R<()> {
    let database = Database::new();
    let space = "hundred-pages-cold-publication";
    let (owner, owner_secret, peer, peer_secret) = encrypted_pair(&database, space);
    drop(owner);
    drop(peer);
    let mut owner = reopen(&database.path("owner.sqlite"), &owner_secret);
    let mut peer = reopen(&database.path("peer.sqlite"), &peer_secret);
    owner.profiles_session_start(true);
    assert!(owner.ready_epoch(space).is_some());
    let mut pages = Vec::new();
    for n in 0..100 {
        let markdown = format!("# Página {n}\n\nOriginal Unicode 日本語 😀\r\nLinha {n}.\n");
        let item = owner.page_import_markdown(space, &format!("{n}.md"), &markdown)?;
        pages.push((item, markdown));
    }
    assert_eq!(owner.outbox_len(), 100);
    assert_eq!(owner.net.pending.len(), 100);
    let originals = owner
        .store
        .outbox()?
        .into_iter()
        .map(|r| r.event)
        .collect::<Vec<_>>();
    let clock = Some(
        protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
            .ok_or_else(|| storage("test clock is outside the actual claim domain"))?,
    );
    assert!(owner.mls_to_claim(clock)?.is_none());
    assert!(owner.ready_epoch(space).is_some());
    assert!(owner.mls_seal_outbox()?);
    let first = owner.outbox_envelopes();
    assert_eq!(first.len(), Engine::APPLICATION_IN_FLIGHT);
    let mut accepted = Vec::new();
    // The actual recipient sees the ordered ciphertext, while the owner loses its echo.
    for env in &first {
        let frame = ordered(&peer, env.clone());
        assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
        accepted.push(frame);
    }
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &owner_secret);
    owner.profiles_session_start(true);
    assert_eq!(owner.outbox_len(), 100);
    assert_eq!(owner.net.pending.len(), 100);
    assert!(
        !owner.mls_seal_outbox()?,
        "cold recovery must not reseal accepted generations"
    );
    assert_eq!(
        owner.outbox_envelopes(),
        first,
        "retry must preserve exact retained ciphertext"
    );
    assert_eq!(
        owner
            .store
            .outbox()?
            .into_iter()
            .map(|r| r.event)
            .collect::<Vec<_>>(),
        originals
    );
    for frame in accepted {
        assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(peer.ingest(frame), Ingest::Duplicate);
    }
    let mut count = first.len();
    while owner.outbox_len() != 0 {
        let offered = owner.outbox_envelopes();
        assert!(
            !offered.is_empty(),
            "ready retained Page queue stalled before offer"
        );
        assert!(offered.len() <= Engine::APPLICATION_IN_FLIGHT);
        for env in offered {
            let frame = ordered(&owner, env);
            assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
            assert_eq!(peer.ingest(frame), Ingest::Applied);
            count += 1;
        }
        assert!(
            count <= 100,
            "canonical delivery duplicated an original Page"
        );
    }
    assert_eq!(count, 100);
    assert!(owner.net.pending.is_empty());
    assert_eq!(peer.state.items.len(), 100);
    for (item, markdown) in pages {
        assert_eq!(owner.page_markdown(&item)?, markdown);
        assert_eq!(peer.page_markdown(&item)?, markdown);
    }
    owner.logs[space].verify().map_err(storage)?;
    peer.logs[space].verify().map_err(storage)?;
    Ok(())
}
