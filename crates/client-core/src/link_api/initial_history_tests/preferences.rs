//! Complete preferences scenarios; registration remains in the parent.

use super::*;

pub(super) async fn linked_chat_preferences_bootstrap_is_signed_idempotent_and_survives_history_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    assert_linked_preferences_survive_history_and_reopen(false).await?;

    Ok(())
}

pub(super) async fn linked_chat_preferences_repair_legacy_missing_personal_after_verified_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    assert_linked_preferences_survive_history_and_reopen(true).await?;

    Ok(())
}

pub(super) fn linked_chat_preferences_bootstrap_storage_failure_rolls_back_before_vault_and_can_retry(
) {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = Arc::new(Vault::default());
    core.lock()
        .store
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_link_bootstrap BEFORE INSERT ON events \
                 BEGIN SELECT RAISE(ABORT, 'fixture refused bootstrap'); END;",
        )
        .unwrap();
    let failure = finish_test_link(&core, vault.clone()).unwrap_err();
    assert!(matches!(failure, CoreError::Storage { .. }));
    assert!(core.account().is_none());
    assert!(core.lock().state.spaces.is_empty());
    {
        let e = core.lock();
        assert!(e.me.is_none());
        assert!(e.net.author.is_none());
        assert!(e.is_empty());
        assert_eq!(e.store.event_count().unwrap(), 0);
        assert!(e.store.meta("account").unwrap().is_none());
        assert!(e
            .store
            .meta(META_INITIAL_HISTORY_PENDING)
            .unwrap()
            .is_none());
        assert!(e.store.meta(META_PRIMARY).unwrap().is_none());
    }
    assert!(
        vault.0.lock().unwrap().is_empty(),
        "failed install never reaches vault writes"
    );
    core.lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_link_bootstrap")
        .unwrap();
    finish_test_link(&core, vault.clone()).unwrap();
    assert_eq!(owned_personal_spaces(&core).len(), 1);
    assert!(core.account().is_some());
    assert!(vault.load(VAULT_IDENTITY.into()).is_some());
    assert!(vault.load(VAULT_DEVICE.into()).is_some());
}

pub(super) async fn linked_chat_preferences_never_reuse_another_identity_personal_space() {
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    let vault = install_link(&core);
    let identity_key = vault.load(VAULT_IDENTITY.into());
    let device_key = vault.load(VAULT_DEVICE.into());
    let old = owned_personal_spaces(&core);
    {
        let mut e = core.lock();
        for id in old {
            e.store
                .conn()
                .execute("DELETE FROM events WHERE space = ?1", [&id])
                .unwrap();
        }
        e.reload().unwrap();
        let foreign = e
            .create_identity(
                types::Identity {
                    id: String::new(),
                    kind: types::IdentityKind::Person,
                    name: "Other local fixture identity".into(),
                    handle: "other_fixture".into(),
                    tint_hex: "#123456".into(),
                    glyph: None,
                    owner: None,
                    bio: String::new(),
                    owner_proof: None,
                },
                true,
            )
            .unwrap();
        e.append(
            "sp_foreign_personal",
            &foreign,
            EventBody::SpaceCreated {
                title: "Other Personal".into(),
                kind: SpaceKind::Personal,
                privacy: Privacy::EndToEnd,
            },
        )
        .unwrap();
    }
    let opened = encrypted_record(&core);
    let receive = core.history_receive.begin(5_000).unwrap();
    core.commit_initial_history(vec![opened], 1, 0, &receive)
        .unwrap();
    drop(receive);
    assert!(owned_personal_spaces(&core).is_empty());
    let foreign_before = core.lock().logs["sp_foreign_personal"].events().to_vec();
    let command = r#"{"op":"set_chat_prefs","space_id":"sp_initial_history","pinned":false,"muted":false,"archived":true}"#;
    assert_eq!(core.client_request(command.into()).await.unwrap(), "null");
    let personal = owned_personal_spaces(&core);
    assert_eq!(personal.len(), 1);
    assert_ne!(personal[0], "sp_foreign_personal");
    assert_eq!(
        core.lock().logs["sp_foreign_personal"].events(),
        foreign_before.as_slice()
    );
    let current = core.lock().space_summary("sp_initial_history").unwrap();
    assert!(current.archived);
    assert!(!current.pinned && !current.muted);
    assert_eq!(vault.load(VAULT_IDENTITY.into()), identity_key);
    assert_eq!(vault.load(VAULT_DEVICE.into()), device_key);
}

pub(super) fn linked_chat_preferences_vault_rejection_cleans_the_new_bootstrap() {
    struct RejectingVault;
    impl SecretVault for RejectingVault {
        fn load(&self, _: String) -> Option<Vec<u8>> {
            None
        }
        fn save(&self, _: String, _: Vec<u8>) -> bool {
            false
        }
        fn delete(&self, _: String) {}
    }
    let core = Client::open(":memory:".into(), "en-US".into()).unwrap();
    let failure = finish_test_link(&core, Arc::new(RejectingVault)).unwrap_err();
    assert!(matches!(failure, CoreError::Invalid { .. }));
    assert!(core.account().is_none());
    {
        let e = core.lock();
        assert!(e.me.is_none());
        assert!(e.net.author.is_none());
        assert!(e.state.spaces.is_empty());
        assert!(e.is_empty());
        assert_eq!(e.store.event_count().unwrap(), 0);
    }
    install_link(&core);
    assert_eq!(owned_personal_spaces(&core).len(), 1);
}
