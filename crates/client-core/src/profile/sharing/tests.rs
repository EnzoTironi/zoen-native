//! Actual SQLite failures must leave profile shares retryable without duplicates.
use super::*;
use event_log::{Author, Signer, SpaceLog};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn person(engine: &mut Engine, name: &str) -> R<IdentityId> {
    engine.create_identity(
        Identity {
            id: String::new(),
            kind: IdentityKind::Person,
            name: name.into(),
            handle: name.into(),
            tint_hex: "#000000".into(),
            glyph: None,
            owner: None,
            bio: String::new(),
            owner_proof: None,
        },
        true,
    )
}

fn fixture() -> Result<Engine, Box<dyn std::error::Error>> {
    let mut engine = Engine::open(":memory:")?;
    let me = person(&mut engine, "owner")?;
    let signer = engine.signers.get(&me).ok_or("missing owner signer")?;
    let author = Author::device(signer, Signer::generate());
    engine.store.set_meta("me", &me)?;
    engine.me = Some(me.clone());
    engine.net.author = Some(author);
    engine.init_profile(&me, "Owner")?;
    engine.net.profiles.supported = true;
    let public = engine
        .net
        .profiles
        .agreement
        .as_ref()
        .ok_or("missing agreement")?
        .public_hex();
    engine.agreement_published(&public);
    engine.profile_upload_answered(1, Ok(()));
    assert!(!engine.profile_share_pass()?);
    assert_eq!(engine.profile_settlement(), ProfileSettlement::Ready);
    Ok(engine)
}

fn share_count(engine: &Engine) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(engine
        .store
        .outbox()?
        .iter()
        .filter(|p| matches!(p.event.body, EventBody::ProfileKeyShared { .. }))
        .count())
}

fn pending(engine: &Engine) {
    assert!(
        engine.net.profiles.shares_dirty,
        "failed shares must remain pending"
    );
    assert_eq!(engine.profile_settlement(), ProfileSettlement::KeyShares);
    assert!(
        engine.net.profiles.shared.is_none(),
        "retry must rescan durable partial progress"
    );
}

fn fail_read_and_recover(table: &str) -> TestResult {
    let mut engine = fixture()?;
    engine.net.profiles.shares_dirty = true;
    engine.net.profiles.shared = None;
    engine.store.raw_execute(&format!(
        "ALTER TABLE {table} RENAME TO unavailable_profile_test"
    ))?;
    assert!(
        engine.profile_share_pass().is_err(),
        "{table} read failure was hidden"
    );
    pending(&engine);
    engine.store.raw_execute(&format!(
        "ALTER TABLE unavailable_profile_test RENAME TO {table}"
    ))?;
    assert!(!engine.profile_share_pass()?);
    assert_eq!(engine.profile_settlement(), ProfileSettlement::Ready);
    assert!(!engine.net.profiles.shares_dirty);
    assert_eq!(share_count(&engine)?, 0);
    Ok(())
}

#[test]
fn blocklist_read_failure_stays_pending_until_storage_recovers() -> TestResult {
    fail_read_and_recover("blocked")
}

#[test]
fn outbox_read_failure_is_not_cached_as_no_existing_shares() -> TestResult {
    fail_read_and_recover("outbox")
}

#[test]
fn own_profile_read_failure_is_not_successful_completion() -> TestResult {
    fail_read_and_recover("profile_keys")
}

fn add_peer_space(engine: &mut Engine, space: &str, peer: &str) -> TestResult {
    let author = engine.net.author.clone().ok_or("missing author")?;
    engine.append_synced(
        space,
        &author,
        1,
        EventBody::SpaceCreated {
            title: space.into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?;
    engine.append_synced(
        space,
        &author,
        2,
        EventBody::MemberAdded {
            identity: peer.into(),
            role: Role::Member,
        },
    )?;
    // Confirm the real signed membership intents before exercising share generation.
    let mut log = SpaceLog::new(space);
    for row in engine
        .store
        .outbox()?
        .into_iter()
        .filter(|p| p.event.space == space)
    {
        let event = log.sequence(row.event).clone();
        assert!(matches!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event),
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    engine.net.synced.insert(space.into());
    Ok(())
}

#[test]
fn partial_write_retry_preserves_first_signed_share_and_does_not_duplicate_it() -> TestResult {
    let mut engine = fixture()?;
    let first = person(&mut engine, "first")?;
    let second = person(&mut engine, "second")?;
    let first_key = AgreementKey::generate();
    let second_key = AgreementKey::generate();
    engine
        .store
        .put_peer_agreement_key(&first, &first_key.public_hex())?;
    engine
        .store
        .put_peer_agreement_key(&second, &second_key.public_hex())?;
    add_peer_space(&mut engine, "sp_first", &first)?;
    add_peer_space(&mut engine, "sp_second", &second)?;
    assert_eq!(share_count(&engine)?, 0);
    engine.store.raw_execute("CREATE TEMP TRIGGER refuse_second_share BEFORE INSERT ON outbox WHEN NEW.space = 'sp_second' AND NEW.json LIKE '%ProfileKeyShared%' BEGIN SELECT RAISE(ABORT, 'synthetic second profile share failure'); END;")?;
    assert!(engine.profile_share_pass().is_err());
    assert_eq!(share_count(&engine)?, 1, "first durable share must survive");
    let first_event = engine.store.outbox()?.remove(0).event;
    pending(&engine);
    engine
        .store
        .raw_execute("DROP TRIGGER refuse_second_share")?;
    assert!(engine.profile_share_pass()?);
    let rows = engine.store.outbox()?;
    assert_eq!(
        rows.len(),
        2,
        "retry must append only the unfinished recipient"
    );
    assert_eq!(
        rows[0].event, first_event,
        "retry rewrote durable first share"
    );
    let me = engine.me.as_deref().ok_or("missing owner")?;
    let (expected_key, expected_version, _, _) = engine.my_fields(me)?;
    for (row, (peer, agreement)) in rows
        .iter()
        .zip([(&first, &first_key), (&second, &second_key)])
    {
        let EventBody::ProfileKeyShared { version, shares } = &row.event.body else {
            return Err("unexpected queued event".into());
        };
        assert_eq!(*version, expected_version);
        assert_eq!(shares.len(), 1);
        assert_eq!(&shares[0].to, peer);
        let sealed = SealedShare {
            ephemeral: shares[0].ephemeral.clone(),
            sealed: shares[0].sealed.clone(),
        };
        let (key, version) = open_share(
            agreement,
            &sealed,
            &share_context(&row.event.space, me, peer),
        )
        .ok_or("peer could not open its actual encrypted share")?;
        assert_eq!(key.to_hex(), expected_key.to_hex());
        assert_eq!(version, expected_version);
    }
    assert!(!engine.profile_share_pass()?);
    assert_eq!(engine.store.outbox()?.len(), 2);
    assert_eq!(engine.profile_settlement(), ProfileSettlement::Ready);
    Ok(())
}
