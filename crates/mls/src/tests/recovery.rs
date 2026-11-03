//! Lost membership, authentication failures and cold recovery.
use super::*;

#[test]
fn removal_during_external_recovery_holds_both_peers_until_the_removal_commit(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let fresh = Person {
        identity: Signer::from_secret(&e.identity.secret()),
        device: Signer::generate()?,
        key: Signer::generate()?.secret(),
    };
    let (ce, cm, cf) = (db()?, db()?, db()?);
    let (old, removed, recovered) = (e.open(&ce)?, m.open(&cm)?, fresh.open(&cf)?);
    let both = roster(&[&e, &m]);
    let context = pair(&old, &removed, &both)?;
    let external = recovered.recover(SPACE, &context, &both).unwrap();
    assert!(recovered.committed_leaves(SPACE).unwrap().is_none());
    let current = roster(&[&e]);
    ce.execute_batch("CREATE TRIGGER fail_reconcile BEFORE INSERT ON mls_recovery_reconcile BEGIN SELECT RAISE(ABORT, 'injected reconciliation write failure'); END;").unwrap();
    assert!(matches!(
        old.open(SPACE, &external.commit, &current, recovered.leaf()),
        Err(MlsError::Storage(_))
    ));
    assert_eq!(old.epoch(SPACE).unwrap(), 1);
    assert_eq!(old.roster(SPACE).unwrap(), both);
    ce.execute_batch("DROP TRIGGER fail_reconcile").unwrap();
    assert_eq!(
        old.open(SPACE, &external.commit, &current, recovered.leaf())
            .unwrap(),
        Opened::Reconcile { epoch: 2 }
    );
    assert_eq!(
        recovered
            .confirm_recovery(SPACE, &external.commit, &current)
            .unwrap(),
        Opened::Reconcile { epoch: 2 }
    );
    for device in [&old, &recovered] {
        assert!(!device.recovery_pending(SPACE));
        assert!(device.needs_reconciliation(SPACE));
        assert!(
            device
                .committed_leaves(SPACE)
                .unwrap()
                .unwrap()
                .keys()
                .any(|(identity, _)| identity == &m.id()),
            "confirmed recovery leaves remain available for the removal reconciliation"
        );
        assert!(device
            .seal(SPACE, b"must not reach the removed member")
            .is_err());
    }
    let repair = recovered
        .commit(SPACE, &[], &BTreeSet::from([m.id()]))
        .unwrap();
    assert_eq!(
        recovered
            .open(SPACE, &repair.commit, &current, recovered.leaf())
            .unwrap(),
        Opened::Commit { epoch: 3 }
    );
    assert_eq!(
        old.open(SPACE, &repair.commit, &current, recovered.leaf())
            .unwrap(),
        Opened::Commit { epoch: 3 }
    );
    assert_eq!(old.roster(SPACE).unwrap(), current);
    assert_eq!(recovered.roster(SPACE).unwrap(), current);
    assert!(!old.needs_reconciliation(SPACE) && !recovered.needs_reconciliation(SPACE));
    let message = recovered
        .seal(SPACE, b"after the sequenced repair")
        .unwrap();
    assert!(
        matches!(old.open(SPACE, &message, &current, recovered.leaf()).unwrap(), Opened::Application { plaintext, .. } if plaintext == b"after the sequenced repair")
    );
    assert_eq!(
        old.checkpoint(SPACE).unwrap(),
        recovered.checkpoint(SPACE).unwrap()
    );
    Ok(())
}

#[test]
fn two_lost_members_recover_from_public_context_without_a_surviving_device(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let both = roster(&[&e, &m]);
    let info = {
        let (ce, cm) = (db()?, db()?);
        let (enzo, marina) = (e.open(&ce)?, m.open(&cm)?);
        enzo.create_group(SPACE).unwrap();
        let c = add(&enzo, &marina)?;
        enzo.open(SPACE, &c.commit, &both, enzo.leaf()).unwrap();
        marina.join(SPACE, &c.welcome.unwrap(), &both).unwrap();
        c.group_info
    }; // Both original device databases, including their group state, are gone.
    let replacement = |p: &Person| -> Result<Person, event_log::CryptoError> {
        Ok(Person {
            identity: Signer::from_secret(&p.identity.secret()),
            device: Signer::generate()?,
            key: Signer::generate()?.secret(),
        })
    };
    let (fresh_e, fresh_m) = (replacement(&e)?, replacement(&m)?);
    let (ce, cm) = (db()?, db()?);
    let (enzo, marina) = (fresh_e.open(&ce)?, fresh_m.open(&cm)?);
    let first = enzo.recover(SPACE, &info, &both).unwrap();
    assert_eq!(commit_epoch(&first.commit), Some(1));
    assert!(enzo.seal(SPACE, b"before confirmation").is_err());
    assert_eq!(
        enzo.confirm_recovery(SPACE, &first.commit, &both).unwrap(),
        Opened::Commit { epoch: 2 }
    );
    let next = marina.recover(SPACE, &first.group_info, &both).unwrap();
    assert_eq!(
        enzo.open(SPACE, &next.commit, &both, marina.leaf())
            .unwrap(),
        Opened::Commit { epoch: 3 }
    );
    marina.confirm_recovery(SPACE, &next.commit, &both).unwrap();
    for (sender, receiver) in [(&enzo, &marina), (&marina, &enzo)] {
        let bytes = sender.seal(SPACE, b"after both devices were lost").unwrap();
        assert!(matches!(
            receiver.open(SPACE, &bytes, &both, sender.leaf()).unwrap(),
            Opened::Application { plaintext, .. } if plaintext == b"after both devices were lost"
        ));
    }
    let gone = BTreeSet::from([
        leaf_name(&e.id(), &e.device.id()),
        leaf_name(&m.id(), &m.device.id()),
    ]);
    let removal = enzo.commit(SPACE, &[], &gone).unwrap();
    for d in [&enzo, &marina] {
        d.open(SPACE, &removal.commit, &both, enzo.leaf()).unwrap();
        assert_eq!(d.leaves(SPACE).unwrap().len(), 2);
    }
    assert_eq!(
        enzo.checkpoint(SPACE).unwrap(),
        marina.checkpoint(SPACE).unwrap()
    );
    Ok(())
}

#[test]
fn recovery_refuses_removed_members_wrong_groups_and_invalid_context(
) -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db()?, db()?);
    let enzo = e.open(&ce)?;
    enzo.create_group(SPACE).unwrap();
    let c = add(&enzo, &m.open(&cm)?)?;
    let fresh = Person {
        identity: Signer::from_secret(&m.identity.secret()),
        device: Signer::generate()?,
        key: Signer::generate()?.secret(),
    };
    let fresh_db = db()?;
    let replacement = fresh.open(&fresh_db)?;
    assert!(matches!(
        replacement.recover(SPACE, &c.group_info, &roster(&[&e])),
        Err(MlsError::Unlisted(_))
    ));
    assert!(replacement
        .recover("other-space", &c.group_info, &roster(&[&e, &m]))
        .is_err());
    let mut corrupt = c.group_info;
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(replacement
        .recover(SPACE, &corrupt, &roster(&[&e, &m]))
        .is_err());
    assert!(!replacement.has_group(SPACE));
    Ok(())
}

#[test]
fn recovery_marker_survives_reopen_and_failed_staging_rolls_back_the_group(
) -> Result<(), Box<dyn std::error::Error>> {
    let old = Person::new()?;
    let c = db()?;
    let device = old.open(&c)?;
    device.create_group(SPACE).unwrap();
    let context = device
        .commit(SPACE, &[], &BTreeSet::new())
        .unwrap()
        .group_info;
    let fresh = Person {
        identity: Signer::from_secret(&old.identity.secret()),
        device: Signer::generate()?,
        key: Signer::generate()?.secret(),
    };
    let restored = db()?;
    restored.execute_batch("CREATE TRIGGER fail_recovery BEFORE INSERT ON mls_recovery_pending BEGIN SELECT RAISE(ABORT, 'injected disk write failure'); END;").unwrap();
    let both = roster(&[&old]);
    assert!(fresh
        .open(&restored)?
        .recover(SPACE, &context, &both)
        .is_err());
    assert!(
        !fresh.open(&restored)?.has_group(SPACE),
        "failure must roll back operational OpenMLS state"
    );
    restored
        .execute_batch("DROP TRIGGER fail_recovery")
        .unwrap();
    let commit = fresh
        .open(&restored)?
        .recover(SPACE, &context, &both)
        .unwrap();
    let reopened = fresh.open(&restored)?;
    assert_eq!(reopened.group_state(SPACE).unwrap(), GroupState::Pending);
    assert!(reopened.pending(SPACE));
    assert!(reopened.seal(SPACE, b"held after restart").is_err());
    assert!(reopened
        .confirm_recovery(SPACE, b"another commit", &both)
        .is_err());
    assert!(reopened.pending(SPACE));
    reopened.abandon(SPACE).unwrap();
    assert_eq!(reopened.group_state(SPACE).unwrap(), GroupState::Missing);
    assert!(!reopened.has_group(SPACE));
    assert!(!reopened.pending(SPACE));
    let retry = reopened.recover(SPACE, &context, &both).unwrap();
    assert_ne!(retry.commit, commit.commit);
    reopened
        .confirm_recovery(SPACE, &retry.commit, &both)
        .unwrap();
    assert_eq!(reopened.group_state(SPACE).unwrap(), GroupState::Ready);
    assert!(!reopened.pending(SPACE));
    assert!(reopened.seal(SPACE, b"confirmed").is_ok());
    Ok(())
}

#[test]
fn state_survives_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let (e, m) = (Person::new()?, Person::new()?);
    let (ce, cm) = (db()?, db()?);
    let both = roster(&[&e, &m]);
    pair(&e.open(&ce)?, &m.open(&cm)?, &both)?;
    let msg = e.open(&ce)?.seal(SPACE, b"after restart").unwrap();
    let marina = m.open(&cm)?;
    assert!(marina.has_group(SPACE));
    assert!(matches!(
        marina
            .open(SPACE, &msg, &both, e.open(&ce)?.leaf())
            .unwrap(),
        Opened::Application { .. }
    ));
    Ok(())
}

#[test]
fn failed_application_authentication_preserves_state_across_cold_reopen(
) -> Result<(), Box<dyn std::error::Error>> {
    fn group_rows(conn: &Connection) -> Vec<(Vec<u8>, String, Vec<u8>, i64)> {
        conn.prepare(
            "SELECT group_id, data_type, group_data, provider_version
             FROM openmls_group_data ORDER BY group_id, data_type",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    struct Database(std::path::PathBuf);
    impl Drop for Database {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    let (e, m) = (Person::new()?, Person::new()?);
    let database = Database(
        std::env::temp_dir().join(format!("zoen-mls-auth-rejection-{}.sqlite", m.device.id())),
    );
    let ce = db()?;
    let mut cm = Connection::open(&database.0).unwrap();
    migrate(&mut cm).unwrap();
    let enzo = e.open(&ce)?;
    let both = roster(&[&e, &m]);
    pair(&enzo, &m.open(&cm)?, &both)?;
    let plaintext = b"The original ciphertext remains readable after rejection";
    let valid = enzo.seal(SPACE, plaintext).unwrap();
    let mut tampered = valid.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert_eq!(application_epoch(&tampered), application_epoch(&valid));
    assert!(application_epoch(&tampered).is_some());
    let before = group_rows(&cm);
    let checkpoint = {
        let marina = m.open(&cm)?;
        let checkpoint = marina.checkpoint(SPACE).unwrap();
        assert!(matches!(
            marina.open(SPACE, &tampered, &both, enzo.leaf()),
            Err(MlsError::Mls(_))
        ));
        assert_eq!(group_rows(&cm), before);
        assert_eq!(marina.checkpoint(SPACE).unwrap(), checkpoint);
        assert_eq!(marina.group_state(SPACE).unwrap(), GroupState::Ready);
        checkpoint
    };
    drop(cm);

    let cm = Connection::open(&database.0).unwrap();
    assert_eq!(group_rows(&cm), before);
    {
        let marina = m.open(&cm)?;
        assert_eq!(marina.checkpoint(SPACE).unwrap(), checkpoint);
        assert!(matches!(
            marina.open(SPACE, &valid, &both, enzo.leaf()).unwrap(),
            Opened::Application { plaintext: opened, from }
                if opened == plaintext && &from == enzo.leaf()
        ));
    }
    drop(cm);

    let cm = Connection::open(&database.0).unwrap();
    let marina = m.open(&cm)?;
    assert!(matches!(
        marina.open(SPACE, &valid, &both, enzo.leaf()),
        Err(MlsError::Mls(_))
    ));
    let next = enzo
        .seal(SPACE, b"Traffic continues after the cold retry")
        .unwrap();
    assert!(matches!(
        marina.open(SPACE, &next, &both, enzo.leaf()).unwrap(),
        Opened::Application { plaintext: opened, from }
            if opened == b"Traffic continues after the cold retry" && &from == enzo.leaf()
    ));
    Ok(())
}
