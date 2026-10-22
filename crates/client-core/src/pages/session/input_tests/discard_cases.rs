//! Complete discard scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn input_discard_guards_and_publication_ack_preserve_private_document_and_future_capture(
) {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        let pending = queued(
            session
                .commit(&mut f.engine, "publication predates input")
                .unwrap(),
        );
        drop(session);
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let captured = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_A,
                opened.revision,
                7,
                &rich_payload(7, "discardable native input"),
            )
            .unwrap();
        let capture = captured.current.capture.as_ref().unwrap();
        let journal = journal_without_input(&f);
        let cipher = f.cipher();
        trigger(
            &f,
            "refuse_input_discard",
            "INSERT",
            "meta",
            "WHEN NEW.key LIKE 'page.draft.%'",
        );
        assert!(f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A"
            )
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
        drop_trigger(&f, "refuse_input_discard");
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        assert!(
            private_document_backup_edit(&f, &mut s, &read, "old-apply", "late old editor")
                .is_err()
        );
        assert!(s.commit(&mut f.engine, "late old commit").is_err());
        if kind == ItemKind::Page {
            let old_page = f.engine.page(&f.item).unwrap();
            assert!(f
                .engine
                .page_apply_from(
                    &f.item,
                    "old-public-apply",
                    &old_page.edit_context,
                    &old_page
                        .blocks
                        .iter()
                        .map(|b| b.id.clone())
                        .collect::<Vec<_>>(),
                    &[]
                )
                .is_err());
            assert!(f.engine.page_commit(&f.item, "late public commit").is_err());
            assert!(f.engine.page_restore(&f.item, 1, "late restore").is_err());
            assert!(s.restore_binary(&f.engine, 1).is_err());
        } else {
            assert!(f
                .engine
                .page_input_apply(
                    &scope,
                    WRITER_A,
                    captured.current.revision,
                    7,
                    &capture.sha256,
                    "closed-Plan-apply"
                )
                .is_err());
        }
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), journal);
        // The real ACK for a prior Share refreshes canonical/shadow state without
        // consuming the native checkpoint retained after that Share.
        f.ack(&pending);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
        let after_ack = journal_without_input(&f);
        let view = f.session().read(&f.engine).unwrap().view;
        let discarded = f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A",
            )
            .unwrap();
        assert!(!discarded.replayed);
        assert!(discarded.current.capture.is_none() && discarded.current.binding.is_none());
        assert_eq!(journal_without_input(&f), after_ack);
        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), discarded.current);
        assert_eq!(f.session().read(&f.engine).unwrap().view, view);
        let b = input_open(&f, WRITER_B);
        let b = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_B,
                b.revision,
                8,
                &rich_payload(8, "future B remains pending"),
            )
            .unwrap();
        let cipher = f.cipher();
        let replay = f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A",
            )
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.accepted_revision, discarded.accepted_revision);
        assert_eq!(replay.current, b.current);
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), after_ack);
    }
}

pub(super) fn plan_input_metadata_survives_pending_legacy_without_admitting_empty_release() {
    for captured in [false, true] {
        let (_db, mut f) = cold_input_fixture(ItemKind::Plan);
        // A confirmed legacy Plan with no applied CRDT draft can still hold
        // private native input; checkpointing must not change has_draft.
        f.item = new_id("confirmed-input-plan");
        f.ordered(EventBody::ItemCreated {
            item: f.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "confirmed metadata fixture".into(),
        });
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let input = if captured {
            f.engine
                .page_input_checkpoint(
                    &scope,
                    WRITER_A,
                    opened.revision,
                    7,
                    &rich_payload(7, "private input behind pending legacy projection"),
                )
                .unwrap()
                .current
        } else {
            opened
        };
        assert_input_metadata(&f, &input);
        assert!(!f.engine.plan_page(&f.item).unwrap().unsaved);
        let mut pending = original();
        pending.title = "local legacy version remains pending".into();
        f.queue(f.version(ItemContent::Plan(pending)));
        let cipher = f.cipher();
        let read = f.engine.plan_page(&f.item).unwrap();
        assert_eq!(
            read.source,
            crate::pages::PlanPageReadSourceDto::LocalPendingLegacy
        );
        assert!(!read.unsaved);
        assert_eq!(read.input_pending, captured);
        assert_eq!(read.input_revision, input.revision.to_string());
        let outbox = f.engine.store.outbox_len().unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, input.revision, "pending-legacy-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        f = reopen_input_fixture(f);
        assert_input_metadata(&f, &input);
        assert_eq!(f.cipher(), cipher);

        // A purely optimistic creation has no authenticated document session.
        f.item = new_id("pending-input-plan");
        f.queue(EventBody::ItemCreated {
            item: f.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "pending metadata fixture".into(),
        });
        let read = f.engine.plan_page(&f.item).unwrap();
        assert_eq!(
            read.source,
            crate::pages::PlanPageReadSourceDto::LocalPendingLegacy
        );
        assert!(!read.input_pending);
        assert_eq!(read.input_revision, "0");
        let scope = input_scope(&f);
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, 0, "no-pending-authority")
            .is_err());
        assert!(f.cipher().is_none());
    }
}
