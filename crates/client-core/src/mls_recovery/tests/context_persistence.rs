//! Preserved context persistence scenarios; stable test entries stay in the parent.
use super::*;

pub(super) fn context_and_commit_roll_back_together_and_wait_for_upload_after_restart(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path =
        std::env::temp_dir().join(format!("zoen-recovery-{}.db", types::new_ulid(now_ms())?));
    let space = "recovery-transaction";
    let (root_secret, secret, agreement, commit, reference, genesis, uploaded_blob);
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        let (root, device, key) = engine
            .create_account("Ana", "ana", "http://relay.test")
            .unwrap();
        secret = device;
        root_secret = root;
        agreement = key;
        let author = engine.net.author.clone().unwrap();
        let mut log = SpaceLog::new(space);
        let created = log
            .sequence(author.sign_event(
                space,
                "genesis",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Recovery".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            ))
            .clone();
        genesis = created.hash.clone();
        assert_eq!(
            engine.ingest(Sequenced {
                seq: 0,
                prev: created.prev.clone(),
                hash: created.hash.clone(),
                env: Envelope::plain(&created)
            }),
            Ingest::Applied
        );
        engine.create_mls_group(space).unwrap();
        engine.net.profiles.supported = true;
        engine.store.conn().execute_batch("CREATE TRIGGER fail_context BEFORE INSERT ON mls_recovery_uploads BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        assert!(engine.commit_now(space, &[], &Default::default()).is_err());
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.uploads_pending(), 0);
        engine
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_context")
            .unwrap();
        engine.commit_now(space, &[], &Default::default()).unwrap();
        let queued = engine.store.outbox().unwrap();
        assert_eq!(queued.len(), 1);
        commit = Envelope::plain(&queued[0].event);
        reference = RecoveryRef::parse(commit.recovery().unwrap()).unwrap();
        uploaded_blob = engine.pending_uploads(1).remove(0).1;
        assert_eq!(engine.uploads_pending(), 1);
        assert!(engine.outbox_envelopes().is_empty());
    }
    let (replacement_secret, external, external_reference, previous);
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        assert!(engine.unlock(Some(secret.to_vec())).unwrap());
        engine.unlock_profile(Some(agreement.to_vec())).unwrap();
        assert!(engine.device().unwrap().pending(space));
        assert!(engine.outbox_envelopes().is_empty());
        engine.upload_refused(&reference.blob).unwrap();
        assert_eq!(engine.uploads_pending(), 1);
        assert!(
            engine.outbox_envelopes().is_empty(),
            "HTTP refusal must not release the commit"
        );
        engine.upload_done(&reference.blob);
        let ready = engine.outbox_envelopes();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].content(), commit.content());
        assert_eq!(ready[0].client_id(), commit.client_id());
        previous = chain_hash(space, 1, &genesis, &commit.wire_hash());
        assert_eq!(
            engine.ingest(Sequenced {
                seq: 1,
                prev: genesis.clone(),
                hash: previous.clone(),
                env: commit
            }),
            Ingest::Confirmed
        );
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.recovery_context(space).unwrap().blob, reference.blob);
        engine.device().unwrap().forget(space).unwrap();
        let replacement = event_log::Signer::generate()?;
        replacement_secret = replacement.secret();
        engine
            .install_restored_account(
                &event_log::Signer::from_secret(&root_secret),
                replacement,
                "http://relay.test",
            )
            .unwrap();
        engine.net.profiles.supported = true;
        let context = engine.recovery_context(space).unwrap();
        assert!(engine.recovery_context_arrived(&context, &uploaded_blob));
        assert!(engine.mls_recovery_pass().unwrap());
        assert!(engine.device().unwrap().recovery_pending(space));
        assert!(engine
            .device()
            .unwrap()
            .seal(space, b"held external join")
            .is_err());
        let old = engine.store.outbox().unwrap()[0].event.client_id.clone();
        engine.store.conn().execute_batch("CREATE TRIGGER fail_abandon BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT, 'injected refusal write failure'); END;").unwrap();
        assert!(
            matches!(engine.reject(&old, protocol::STALE_COMMIT, true, None), crate::sync::RejectionOutcome::StorageFailure(reason) if reason.contains("injected refusal write failure"))
        );
        assert!(engine.device().unwrap().recovery_pending(space));
        assert_eq!(engine.outbox_len(), 1);
        assert_eq!(engine.uploads_pending(), 1);
        engine
            .store
            .conn()
            .execute_batch("DROP TRIGGER fail_abandon")
            .unwrap();
        assert_eq!(
            engine.reject(&old, protocol::STALE_COMMIT, true, None),
            crate::sync::RejectionOutcome::HandshakeReconciled
        );
        assert!(!engine.device().unwrap().has_group(space));
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert_eq!(engine.uploads_pending(), 0);
        assert!(engine.mls_recovery_pass().unwrap());
        external = Envelope::plain(&engine.store.outbox().unwrap()[0].event);
        external_reference = RecoveryRef::parse(external.recovery().unwrap()).unwrap();
        assert!(engine.outbox_envelopes().is_empty());
    }
    {
        let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
        engine.unlock(Some(replacement_secret.to_vec())).unwrap();
        engine.unlock_profile(Some(agreement.to_vec())).unwrap();
        assert!(engine.device().unwrap().recovery_pending(space));
        assert!(engine.outbox_envelopes().is_empty());
        engine.upload_done(&external_reference.blob);
        assert_eq!(engine.outbox_envelopes()[0].content(), external.content());
        let confirmation = Sequenced {
            seq: 2,
            prev: previous.clone(),
            hash: chain_hash(space, 2, &previous, &external.wire_hash()),
            env: external,
        };
        for table in ["mls_recovery_pending", "outbox"] {
            engine.store.conn().execute_batch(&format!("CREATE TRIGGER fail_confirmation BEFORE DELETE ON {table} BEGIN SELECT RAISE(ABORT, 'injected confirmation write failure'); END;")).unwrap();
            assert!(matches!(
                engine.ingest(confirmation.clone()),
                Ingest::Invalid(_)
            ));
            assert!(engine.device().unwrap().recovery_pending(space));
            assert_eq!(engine.logs[space].next_seq(), 2);
            assert_eq!(engine.outbox_len(), 1);
            assert!(engine
                .store
                .recovery_upload_confirmed(&external_reference.blob)
                .unwrap());
            assert!(engine
                .device()
                .unwrap()
                .seal(space, b"still held after failed confirmation")
                .is_err());
            engine
                .store
                .conn()
                .execute_batch("DROP TRIGGER fail_confirmation")
                .unwrap();
        }
        assert_eq!(engine.ingest(confirmation), Ingest::Confirmed);
        assert!(!engine.device().unwrap().pending(space));
        assert_eq!(engine.outbox_len(), 0);
        assert!(!engine
            .store
            .recovery_upload_confirmed(&external_reference.blob)
            .unwrap());
        assert!(engine
            .device()
            .unwrap()
            .seal(space, b"confirmed after external restart")
            .is_ok());
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));

    Ok(())
}
