//! Foreign lookup faults cannot silently consume work or install half a projection.
use super::*;

fn foreign_fixture() -> R<(Engine, SealedProfile, SealedProfile)> {
    let (mut engine, _) = fixture()?;
    let mut peer = Engine::open(":memory:")?;
    peer.create_account("Previous", "peer", "http://relay.test")?;
    let id = peer.me_id()?;
    peer.profiles_session_start(true);
    peer.sealed_profiles_arrived(Vec::new(), std::slice::from_ref(&id));
    let old = peer.profile_to_upload().expect("real peer bootstrap");
    peer.profile_upload_answered(old.version, Ok(()));
    let (_, key_version, seen_version, fields) = peer.my_fields(&id)?;
    engine
        .store
        .put_profile_key(&id, &peer.my_fields(&id)?.0.to_hex(), key_version)?;
    engine
        .store
        .set_profile_fields(&id, seen_version, &fields.to_bytes())?;
    let identity = peer
        .identities
        .get(&id)
        .expect("actual peer identity")
        .clone();
    engine.store.put_identity(&identity, None)?;
    engine.identities.insert(id.clone(), identity);
    peer.update_my_profile("Updated peer", "new bio", PhotoChange::Keep)?;
    let incoming = peer.profile_to_upload().expect("real signed peer update");
    engine.profile_changed(&id);
    Ok((engine, incoming, old))
}

#[derive(Clone, Copy)]
enum Fault {
    Read,
    Fields,
    Projection,
    Release,
}

fn storage_fault(fault: Fault) -> R<()> {
    let (mut engine, incoming, _) = foreign_fixture()?;
    let id = incoming.identity.clone();
    let before = engine.store.profile_key(&id)?.expect("held peer key");
    let identity = engine.identities.get(&id).expect("known peer").clone();
    let own_journal = engine.store.meta(META_OWN_SYNC)?;
    let directory = engine.store.identities()?;
    let asked = engine.take_need_profiles();
    let (inject, repair) = match fault {
        Fault::Read => ("ALTER TABLE profile_keys RENAME TO unavailable_profile_keys", "ALTER TABLE unavailable_profile_keys RENAME TO profile_keys"),
        Fault::Fields => ("CREATE TEMP TRIGGER fail_foreign_fields BEFORE UPDATE OF fields ON profile_keys BEGIN SELECT RAISE(ABORT, 'foreign fields fault'); END", "DROP TRIGGER fail_foreign_fields"),
        Fault::Projection => ("CREATE TEMP TRIGGER fail_foreign_projection BEFORE INSERT ON identities BEGIN SELECT RAISE(ABORT, 'foreign projection fault'); END", "DROP TRIGGER fail_foreign_projection"),
        Fault::Release => (release::INJECT, "DROP TRIGGER fail_profile_release"),
    };
    engine.store.raw_execute(inject)?;
    assert!(engine
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    if matches!(fault, Fault::Release) {
        assert!(
            engine.store.raw_execute("BEGIN; ROLLBACK").is_ok(),
            "failed RELEASE must leave no open transaction"
        );
    }
    // Restore only the injected read/write fault, before reading the actual tables.
    engine.store.raw_execute(repair)?;
    let retained = engine
        .store
        .profile_key(&id)?
        .expect("original key still present");
    assert_eq!(
        retained.fields, before.fields,
        "projection failure rolls back decrypted fields"
    );
    assert_eq!(retained.seen_version, before.seen_version);
    assert_eq!(retained.key, before.key);
    assert_eq!(retained.key_version, before.key_version);
    assert_eq!(engine.identities.get(&id), Some(&identity));
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, own_journal);
    assert_eq!(engine.store.identities()?, directory);
    assert!(
        !engine.profiles_answer_complete(&asked),
        "foreign storage fault remains work"
    );
    assert_eq!(engine.take_need_profiles(), asked);
    assert_eq!(
        engine.sealed_profiles_arrived(vec![incoming.clone()], &asked),
        asked
    );
    assert!(engine.profiles_answer_complete(&asked));
    assert_eq!(
        engine.profile_view(&id)?.name.as_deref(),
        Some("Updated peer")
    );
    assert_eq!(
        engine.identities.get(&id).expect("updated projection").name,
        "Updated peer"
    );
    let committed = engine.store.profile_key(&id)?.expect("committed fields");
    assert_eq!(committed.seen_version, incoming.version);
    // Exact replay stays inert. Stale, absent and no-key are separately legitimate.
    assert!(engine
        .sealed_profiles_arrived(vec![incoming], &asked)
        .is_empty());
    assert_eq!(
        engine.store.profile_key(&id)?.expect("same fields").fields,
        committed.fields
    );
    Ok(())
}

#[test]
fn foreign_key_read_fault_retains_original_lookup_until_repair() -> R<()> {
    storage_fault(Fault::Read)
}
#[test]
fn foreign_fields_write_fault_retains_original_lookup_until_repair() -> R<()> {
    storage_fault(Fault::Fields)
}
#[test]
fn foreign_projection_write_fault_rolls_back_fields_and_recovers() -> R<()> {
    storage_fault(Fault::Projection)
}

#[test]
fn legitimate_absent_stale_forged_and_no_key_foreign_replies_still_drain() -> R<()> {
    let (mut engine, incoming, old) = foreign_fixture()?;
    let id = incoming.identity.clone();
    let asked = engine.take_need_profiles();
    // An absent foreign publication is not the own known-publication contract.
    assert!(engine
        .sealed_profiles_arrived(Vec::new(), &asked)
        .is_empty());
    assert!(engine.profiles_answer_complete(&asked));
    engine.profile_changed(&id);
    let asked = engine.take_need_profiles();
    let mut forged = incoming.clone();
    forged.ciphertext.push(0);
    assert!(engine
        .sealed_profiles_arrived(vec![forged], &asked)
        .is_empty());
    assert!(engine.profiles_answer_complete(&asked));
    // A missing key is distinct from a failed SELECT.
    let (mut no_key, _) = fixture()?;
    no_key.net.profiles.need_profiles.insert(id.clone());
    assert!(no_key
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    assert!(no_key.profiles_answer_complete(&asked));
    engine.sealed_profiles_arrived(vec![incoming.clone()], &asked);
    engine.profile_changed(&id);
    let asked = engine.take_need_profiles();
    assert!(engine.sealed_profiles_arrived(vec![old], &asked).is_empty());
    assert!(engine.profiles_answer_complete(&asked));
    assert_eq!(
        engine.profile_view(&id)?.name.as_deref(),
        Some("Updated peer")
    );
    Ok(())
}

#[test]
fn own_projection_fault_rolls_back_fields_and_journal_until_retry() -> R<()> {
    let (mut engine, signer) = fixture()?;
    let id = signer.id();
    let identity = Identity {
        id: id.clone(),
        kind: IdentityKind::Person,
        name: "Original".into(),
        handle: "owner".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    };
    engine.store.put_identity(&identity, None)?;
    engine.identities.insert(id.clone(), identity.clone());
    let incoming = remote(&engine, &signer, 2)?;
    let fields = engine.my_fields(&id)?.3;
    let journal = engine.store.meta(META_OWN_SYNC)?;
    let directory = engine.store.identities()?;
    engine.profile_changed(&id);
    let asked = engine.take_need_profiles();
    engine.store.raw_execute("CREATE TEMP TRIGGER fail_own_projection BEFORE INSERT ON identities BEGIN SELECT RAISE(ABORT, 'own projection fault'); END")?;
    assert!(engine
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    assert_eq!(
        engine.my_fields(&id)?.3,
        fields,
        "own projection must not half-commit fields"
    );
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, journal);
    assert_eq!(engine.store.identities()?, directory);
    assert_eq!(engine.identities.get(&id), Some(&identity));
    assert_pending(&engine, &asked);
    engine
        .store
        .raw_execute("DROP TRIGGER fail_own_projection")?;
    assert_eq!(engine.take_need_profiles(), asked);
    assert_eq!(
        engine.sealed_profiles_arrived(vec![incoming], &asked),
        asked
    );
    assert_eq!(
        engine
            .identities
            .get(&id)
            .expect("committed projection")
            .name,
        "Other device"
    );
    assert!(engine.profiles_settled());
    Ok(())
}

mod release;
