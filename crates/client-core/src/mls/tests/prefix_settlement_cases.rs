//! Existing MLS assertions grouped by behavior; stable entrypoints live in tests.rs.
use super::*;

pub(super) fn divergent_prefix_on_the_local_chain_marks_the_space(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-now";
    start_space(&mut engine, &author, space)?;
    ingest_signed(&mut engine, &author, space, post("one"))?;
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer)?;
    ingest_signed(
        &mut engine,
        &peer,
        space,
        checkpoint(
            types::Seen {
                seq: 0,
                hash: "11".repeat(32),
            },
            &"ab".repeat(32),
        ),
    )?;
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_eq!(
        engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
        Some(FORK)
    );
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn matching_prefix_does_not_fork() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-match";
    start_space(&mut engine, &author, space)?;
    ingest_signed(&mut engine, &author, space, post("one"))?;
    let hash = hash_at(&engine, space, 0);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer)?;
    ingest_signed(
        &mut engine,
        &peer,
        space,
        checkpoint(types::Seen { seq: 0, hash }, &"ab".repeat(32)),
    )?;
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(pending(&engine, space).is_empty());
    assert_eq!(engine.logs[space].events().len(), 4);

    Ok(())
}

pub(super) fn matching_pending_prefix_is_cleared_without_a_fork(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-clear";
    start_space(&mut engine, &author, space)?;
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer)?;
    let mut stored = BTreeMap::new();
    stored.insert(
        device_key(&peer),
        PendingPrefix {
            seq: 0,
            hash: hash_at(&engine, space, 0),
            epoch: 1,
            digest: "ab".repeat(32),
        },
    );
    engine
        .store
        .set_meta(
            &pending_prefix_meta(space),
            &serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();
    ingest_signed(&mut engine, &author, space, post("later"))?;
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(engine.store.meta(&fork_meta(space)).unwrap(), None);
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn pending_prefix_forks_when_the_chain_reaches_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "prefix-later";
    start_space(&mut engine, &author, space)?;
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer)?;
    let claimed = "22".repeat(32);
    let target = engine.next_seq(space) + 3;
    ingest_signed(&mut engine, &peer, space, claim(target, &claimed))?;
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(
        pending(&engine, space).get(&device_key(&peer)),
        Some(&PendingPrefix {
            seq: target,
            hash: claimed,
            epoch: 1,
            digest: "ab".repeat(32),
        })
    );
    let quiet = target - engine.next_seq(space);
    for _ in 0..quiet {
        ingest_signed(
            &mut engine,
            &author,
            space,
            post(&format!("before-{target}")),
        )?;
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(pending(&engine, space).len(), 1);
    }
    ingest_signed(&mut engine, &author, space, post("reached"))?;
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_eq!(
        engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
        Some(FORK)
    );
    assert!(pending(&engine, space).is_empty());
    assert_ne!(hash_at(&engine, space, target), "22".repeat(32));

    Ok(())
}

pub(super) fn pending_prefix_survives_reopen_and_forks_when_reached(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("u18-prefix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("device.sqlite");
    let _ = std::fs::remove_file(&path);
    let path = path.to_str().unwrap().to_string();
    let space = "prefix-reopen";
    let claimed = "33".repeat(32);
    let peer = Author::root(Signer::generate()?);
    let device_secret = {
        let (mut engine, author, device_secret) = member(&path);
        start_space(&mut engine, &author, space)?;
        add_member(&mut engine, &author, space, &peer)?;
        assert_eq!(engine.next_seq(space), 2);
        ingest_signed(&mut engine, &peer, space, claim(5, &claimed))?;
        assert_eq!(integrity(&engine, space), None);
        assert_eq!(
            pending(&engine, space)
                .get(&device_key(&peer))
                .map(|item| item.seq),
            Some(5)
        );
        device_secret
    };
    let mut engine = Engine::open(&path).unwrap();
    assert!(engine.unlock(Some(device_secret.to_vec())).unwrap());
    assert_eq!(integrity(&engine, space), None);
    assert_eq!(
        pending(&engine, space).get(&device_key(&peer)),
        Some(&PendingPrefix {
            seq: 5,
            hash: claimed,
            epoch: 1,
            digest: "ab".repeat(32),
        })
    );
    let author = engine.net.author.clone().unwrap();
    let quiet = 5 - engine.next_seq(space);
    for _ in 0..quiet {
        ingest_signed(&mut engine, &author, space, post("before-reopen"))?;
        assert_eq!(integrity(&engine, space), None);
    }
    ingest_signed(&mut engine, &author, space, post("reached-reopen"))?;
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert!(pending(&engine, space).is_empty());

    Ok(())
}

pub(super) fn divergent_digest_still_marks_the_space(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut engine, author, _) = member(":memory:");
    let space = "digest-still";
    start_space(&mut engine, &author, space)?;
    engine
        .store
        .set_meta(&digest_meta(space, 1), &"aa".repeat(32))
        .unwrap();
    let hash = hash_at(&engine, space, 0);
    let peer = Author::root(Signer::generate()?);
    add_member(&mut engine, &author, space, &peer)?;
    ingest_signed(
        &mut engine,
        &peer,
        space,
        checkpoint(types::Seen { seq: 0, hash }, &"bb".repeat(32)),
    )?;
    assert_eq!(integrity(&engine, space).as_deref(), Some(FORK));
    assert_eq!(
        engine.store.meta(&fork_meta(space)).unwrap().as_deref(),
        Some(FORK)
    );

    Ok(())
}
