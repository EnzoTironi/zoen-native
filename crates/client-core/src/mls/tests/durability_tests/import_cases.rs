//! Cold queued Pages use the same ciphertext and publication window after ACK loss.
use super::*;

#[test]
fn hundred_page_import_cold_reopen_and_lost_confirmation_keep_exact_originals(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = Database::new()?;
    let space = "hundred-pages-cold-publication";
    let (owner, owner_secret, peer, peer_secret) = encrypted_pair(&database, space)?;
    drop(owner);
    drop(peer);
    let mut owner = reopen(&database.path("owner.sqlite"), &owner_secret);
    let mut peer = reopen(&database.path("peer.sqlite"), &peer_secret);
    owner.profiles_session_start(true);
    assert!(owner.ready_epoch(space).is_some());
    let pages = import_pages(&mut owner, space)?;
    let uploads = owner
        .pending_uploads(100)
        .into_iter()
        .collect::<HashMap<_, _>>();
    assert_eq!(
        uploads.len(),
        1,
        "the corpus must exercise a real encrypted external Page"
    );
    for blob in uploads.keys() {
        owner.upload_done(blob);
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
    assert_eq!(first.len(), 5);
    let mut accepted = Vec::new();
    // The actual recipient sees the ordered ciphertext, while the owner loses its echo.
    for env in &first {
        let frame = ordered(&peer, env.clone())?;
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
        assert!(offered.len() <= 5);
        for env in offered {
            let frame = ordered(&owner, env)?;
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
    receive_page_payload(&mut peer, &uploads);
    check_pages(&owner, &peer, &pages)?;
    drop(peer);
    let peer = reopen(&database.path("peer.sqlite"), &peer_secret);
    check_pages(&owner, &peer, &pages)?;
    owner.logs[space].verify().map_err(storage)?;
    peer.logs[space].verify().map_err(storage)?;
    Ok(())
}

fn import_pages(owner: &mut Engine, space: &str) -> R<Vec<(String, String)>> {
    let mut pages = Vec::new();
    for n in 0..100 {
        let markdown = if n == 0 {
            // Incompressible enough for the real Page.snapshot -> encrypted blob path.
            let mut state = 0x5eed_u64;
            let mut text = String::from("# External original\n\n");
            for line in 0..4096 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                text.push_str(&format!("{line}: {state:016x}\n"));
            }
            text
        } else {
            format!("# Página {n}\n\nOriginal Unicode 日本語 😀\r\nLinha {n}.\n")
        };
        let item = owner.page_import_markdown(space, &format!("{n}.md"), &markdown)?;
        pages.push((item, markdown));
    }
    Ok(pages)
}
fn receive_page_payload(peer: &mut Engine, uploads: &HashMap<String, Vec<u8>>) {
    let wanted = peer.wanted_media();
    assert_eq!(wanted.len(), 1);
    for w in wanted {
        let sealed = &uploads[&w.blob];
        let mut corrupt = sealed.clone();
        corrupt[40] ^= 1;
        assert!(
            !peer.media_arrived(&w, &corrupt),
            "changed ciphertext must refuse"
        );
        assert!(peer.media_arrived(&w, sealed));
    }
}
fn check_pages(
    owner: &Engine,
    peer: &Engine,
    pages: &[(String, String)],
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for (item, markdown) in pages {
        assert_eq!(&owner.page_markdown(item)?, markdown);
        assert_eq!(&peer.page_markdown(item)?, markdown);
    }
    Ok(())
}
