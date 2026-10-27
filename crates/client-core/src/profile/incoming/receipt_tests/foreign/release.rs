//! Deferred SQLite constraints fail at RELEASE, after ordinary writes succeeded.
use super::*;

pub(super) fn inject(engine: &Engine) -> R<()> {
    for sql in [
        "PRAGMA foreign_keys = ON",
        "CREATE TEMP TABLE receipt_parent(id INTEGER PRIMARY KEY)",
        "CREATE TEMP TABLE receipt_child(parent INTEGER REFERENCES receipt_parent(id) DEFERRABLE INITIALLY DEFERRED)",
        "CREATE TEMP TRIGGER fail_profile_release AFTER UPDATE OF profile ON main.identities BEGIN INSERT INTO receipt_child VALUES (1); END",
    ] {
        engine.store.raw_execute(sql)?;
    }
    Ok(())
}

pub(super) fn assert_no_transaction(engine: &Engine) -> R<()> {
    assert!(
        engine.store.raw_execute("BEGIN").is_ok(),
        "failed RELEASE must leave no open transaction"
    );
    engine.store.raw_execute("ROLLBACK")?;
    Ok(())
}

#[test]
fn foreign_release_failure_rolls_back_sql_and_memory_before_retry() -> R<()> {
    storage_fault(Fault::Release)
}

#[test]
fn own_release_failure_rolls_back_fields_journal_and_memory_before_retry() -> R<()> {
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
    let dirty = engine.index_dirty;
    engine.profile_changed(&id);
    let asked = engine.take_need_profiles();
    inject(&engine)?;
    assert!(engine
        .sealed_profiles_arrived(vec![incoming.clone()], &asked)
        .is_empty());
    assert_no_transaction(&engine)?;
    assert_eq!(engine.my_fields(&id)?.3, fields);
    assert_eq!(engine.store.meta(META_OWN_SYNC)?, journal);
    assert_eq!(engine.store.identities()?, directory);
    assert_eq!(engine.identities.get(&id), Some(&identity));
    assert_eq!(engine.index_dirty, dirty);
    assert_pending(&engine, &asked);
    engine
        .store
        .raw_execute("DROP TRIGGER fail_profile_release")?;
    assert_eq!(engine.take_need_profiles(), asked);
    assert_eq!(
        engine.sealed_profiles_arrived(vec![incoming], &asked),
        asked
    );
    assert_eq!(
        engine
            .identities
            .get(&id)
            .expect("committed own projection")
            .name,
        "Other device"
    );
    assert!(engine.profiles_settled());
    Ok(())
}
