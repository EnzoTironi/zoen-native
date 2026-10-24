//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn a_member_seen_at_or_beyond_the_event_seq_is_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "seen-ahead";
    start_space(&mut engine, &author, space);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    let next = engine.next_seq(space);
    let event = peer.sign_event(
        space,
        "seen-ahead",
        1_700_000_000_100,
        Some(types::Seen {
            seq: 9_000,
            hash: "12".repeat(32),
        }),
        claim(9_000, &"12".repeat(32)),
    )?;
    let got = deliver(&mut engine, event);
    assert!(9_000 >= next);
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert_eq!(engine.next_seq(space), next);
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_eq!(
        engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
        Some(FORK)
    );
    assert!(engine.logs[space]
        .events()
        .iter()
        .all(|e| e.client_id != "seen-ahead"));

    Ok(())
}

pub(super) fn an_event_ahead_of_the_local_head_stays_a_gap(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "real-gap";
    start_space(&mut engine, &author, space);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    let next = engine.next_seq(space);
    let seen_seq = next - 1;
    let seen_hash = hash_at(&engine, space, seen_seq);
    let event = peer.sign_event(
        space,
        "real-gap",
        1_700_000_000_100,
        Some(types::Seen {
            seq: seen_seq,
            hash: seen_hash.clone(),
        }),
        claim(seen_seq, &seen_hash),
    )?;
    let mut ev = ordered(&engine, Envelope::plain(&event));
    ev.seq = next + 3;
    let got = engine.ingest(ev);
    assert!(
        matches!(got, Ingest::Gap { next: n } if n == next),
        "{got:?}"
    );
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);

    Ok(())
}

pub(super) fn a_non_member_seen_beyond_the_event_is_refused_without_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "seen-stranger-ahead";
    start_space(&mut engine, &author, space);
    let stranger = Author::root(Signer::generate()?);
    let event = stranger.sign_event(
        space,
        "seen-stranger-ahead",
        1_700_000_000_100,
        Some(types::Seen {
            seq: 9_000,
            hash: "12".repeat(32),
        }),
        claim(9_000, &"12".repeat(32)),
    )?;
    let got = deliver(&mut engine, event);
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(engine.logs[space]
        .events()
        .iter()
        .all(|e| e.client_id != "seen-stranger-ahead"));

    Ok(())
}

pub(super) fn a_forged_signature_is_refused_without_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "seen-forged";
    start_space(&mut engine, &author, space);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer);
    let mut event = peer.sign_event(
        space,
        "seen-forged",
        1_700_000_000_100,
        Some(types::Seen {
            seq: 0,
            hash: "11".repeat(32),
        }),
        claim(0, &"11".repeat(32)),
    )?;
    event.sig = "00".repeat(64);
    let got = deliver(&mut engine, event);
    assert!(matches!(got, Ingest::Invalid(_)));
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(engine.logs[space]
        .events()
        .iter()
        .all(|e| e.client_id != "seen-forged"));

    Ok(())
}

pub(super) fn a_non_member_diverged_seen_is_refused_without_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "seen-stranger";
    start_space(&mut engine, &author, space);
    ingest_signed(&mut engine, &author, space, post("one"));
    let stranger = Author::root(Signer::generate()?);
    let event = stranger.sign_event(
        space,
        "seen-stranger",
        1_700_000_000_100,
        Some(types::Seen {
            seq: 0,
            hash: "11".repeat(32),
        }),
        claim(0, &"11".repeat(32)),
    )?;
    let got = deliver(&mut engine, event);
    assert!(matches!(got, Ingest::Invalid(_)));
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(pending(&engine, space).is_empty());
    assert!(engine.logs[space]
        .events()
        .iter()
        .all(|e| e.client_id != "seen-stranger"));

    Ok(())
}
