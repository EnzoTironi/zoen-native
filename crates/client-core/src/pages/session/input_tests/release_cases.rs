//! Complete release scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn release_empty_binding_is_atomic_durable_and_replays_without_dropping_newer_input() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        let pending = queued(
            session
                .commit(&mut f.engine, "publication before empty release")
                .unwrap(),
        );
        drop(session);
        let scope = input_scope(&f);
        let journal = journal_without_input(&f);
        let private = f.session().read(&f.engine).unwrap().view;
        let pending_event = serde_json::to_vec(&f.pending(&pending)).unwrap();
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        let head = f.engine.logs[&f.space].head().map(|h| (h.seq, h.hash));
        let legacy_cipher = f.cipher();
        assert!(legacy_cipher
            .as_ref()
            .unwrap()
            .starts_with(if kind == ItemKind::Page { "v1:" } else { "v2:" }));
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, 0, "no-binding-to-release")
            .is_err());
        assert_eq!(
            f.cipher(),
            legacy_cipher,
            "an absent binding cannot migrate or rewrite a legacy journal"
        );

        let opened = input_open(&f, WRITER_A);
        assert!(opened.capture.is_none());
        assert_input_metadata(&f, &opened);
        let opened_cipher = f.cipher();
        trigger(
            &f,
            "refuse_empty_release",
            "INSERT",
            "meta",
            "WHEN NEW.key LIKE 'page.draft.%'",
        );
        // A first native retention failure leaves an empty accepted slot. Release
        // needs no invented payload, sequence, or capture digest.
        assert!(f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_A,
                opened.revision,
                7,
                &rich_payload(7, "not accepted because SQLite refused the write")
            )
            .is_err());
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        assert!(f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                opened.revision,
                0,
                "",
                "no-sentinel-discard"
            )
            .is_err());
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .is_err());
        assert_eq!(f.cipher(), opened_cipher);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.session().read(&f.engine).unwrap().view, private);
        drop_trigger(&f, "refuse_empty_release");

        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        let accepted = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(!accepted.replayed);
        assert_eq!(accepted.accepted_revision, opened.revision + 1);
        assert_eq!(accepted.current.revision, accepted.accepted_revision);
        assert!(accepted.current.binding.is_none() && accepted.current.capture.is_none());
        assert!(f.cipher().unwrap().starts_with("v3:"));
        assert_eq!(journal_without_input(&f), journal);
        assert_input_metadata(&f, &accepted.current);
        let accepted_cipher = f.cipher();

        // Treat the first ACK as lost, destroy the Engine, unlock the same actual
        // device against the file, and replay the exact request.
        f = reopen_input_fixture(f);
        let replay = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.accepted_revision, accepted.accepted_revision);
        assert_eq!(replay.current, accepted.current);
        assert_eq!(f.cipher(), accepted_cipher);

        let b = input_open(&f, WRITER_B);
        let before = f.cipher();
        let old_ack = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(old_ack.replayed);
        assert_eq!(old_ack.accepted_revision, accepted.accepted_revision);
        assert_eq!(old_ack.current, b);
        assert_eq!(f.cipher(), before);
        for (writer, revision, mutation) in [
            (WRITER_A, opened.revision, "stale-release-A"),
            (WRITER_A, b.revision, "wrong-current-writer"),
            (WRITER_B, accepted.current.revision, "stale-release-B"),
            ("invalid writer", b.revision, "valid-mutation"),
            (WRITER_B, b.revision, "invalid mutation"),
            (WRITER_B, b.revision, ""),
        ] {
            assert!(f
                .engine
                .page_input_release_empty(&scope, writer, revision, mutation)
                .is_err());
            assert_eq!(f.cipher(), before);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), b);
        }
        let b = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_B,
                b.revision,
                8,
                &rich_payload(8, "accepted B must require exact capture discard"),
            )
            .unwrap();
        let before = f.cipher();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_B, b.current.revision, "B-is-not-empty")
            .is_err());
        assert_eq!(f.cipher(), before);
        let old_ack = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(old_ack.replayed);
        assert_eq!(old_ack.accepted_revision, accepted.accepted_revision);
        assert_eq!(old_ack.current, b.current);
        assert_eq!(f.cipher(), before);
        assert_input_metadata(&f, &b.current);
        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), b.current);
        assert_input_metadata(&f, &b.current);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.session().read(&f.engine).unwrap().view, private);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        assert_eq!(
            f.engine.logs[&f.space].head().map(|h| (h.seq, h.hash)),
            head
        );
        assert_eq!(
            serde_json::to_vec(&f.pending(&pending)).unwrap(),
            pending_event
        );
    }
}

pub(super) fn release_empty_requires_the_current_scope_key_and_writing_membership() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let cipher = f.cipher();
        let journal = journal_without_input(&f);
        let mut foreign = vec![scope.clone(); 4];
        foreign[0].expected_identity_id = "another-identity".into();
        foreign[1].expected_device_id = "another-device".into();
        foreign[2].space_id = "another-space".into();
        foreign[3].item_id = "another-item".into();
        for scope in foreign {
            assert!(f
                .engine
                .page_input_release_empty(&scope, WRITER_A, opened.revision, "foreign-release")
                .is_err());
            assert_eq!(f.cipher(), cipher);
        }
        let unlocked = f.engine.net.author.take();
        let root_key = f.engine.signers.remove(&f.owner);
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "locked-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine.net.author = unlocked;
        if let Some(root_key) = root_key {
            f.engine.signers.insert(f.owner.clone(), root_key);
        }
        f.engine
            .store
            .set_meta("provider_signup_pending", "1")
            .unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "setup-pending-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine
            .store
            .conn()
            .execute("DELETE FROM meta WHERE key='provider_signup_pending'", [])
            .unwrap();
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        f.ordered(EventBody::MemberAdded {
            identity: f.owner.clone(),
            role: Role::Reader,
        });
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "reader-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
    }
}
