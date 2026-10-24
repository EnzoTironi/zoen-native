//! Complete convergence backup scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn blob_publication_and_two_first_writers_converge_without_cas() {
    for shorter_first in [false, true] {
        let mut receiver = Fixture::new();
        let mut shorter = receiver.peer();
        let mut short_session = shorter.session();
        let short_read = short_session.read(&shorter.engine).unwrap();
        let mut short_edit = text_edit(&short_read, "");
        short_edit.text = "z".repeat(55_000);
        let short_accept = apply(
            &shorter,
            &mut short_session,
            &short_read,
            "short",
            &[short_edit],
        )
        .unwrap();
        let short_commit = queued(
            short_session
                .commit(&mut shorter.engine, "shorter exact prefix")
                .unwrap(),
        );
        let short_event = shorter.pending(&short_commit);
        let EventBody::ItemVersioned {
            content: ItemContent::Page(page),
            ..
        } = &short_event.body
        else {
            panic!("Page")
        };
        let short_blob = page.blob.clone().expect("real >48KiB Plan blob");
        assert!(shorter.engine.store.uploads_pending().unwrap() > 0);
        let mut legacy = original();
        legacy.sections[0].lines[0].cost_cents = 777;
        legacy.sections[0].lines[0].done = true;
        legacy.budget_cents = Some(9_999);
        let legacy_event = receiver.ordered(receiver.version(ItemContent::Plan(legacy)));
        let mut longer = receiver.peer();
        let mut long_session = longer.session();
        let long_read = long_session.read(&longer.engine).unwrap();
        let mut title = changed(&blocks(&long_read)[0]);
        title.text = RAW.into();
        apply(&longer, &mut long_session, &long_read, "long", &[title]).unwrap();
        let long_commit = queued(
            long_session
                .commit(&mut longer.engine, "longer exact prefix")
                .unwrap(),
        );
        let long_event = longer.pending(&long_commit);
        // Both are already signed; no seen-based CAS or re-signing chooses the winner.
        let short_seen = short_event.seen.clone();
        let long_seen = long_event.seen.clone();
        let bytes = shorter.engine.media(&short_blob.sha256).unwrap().unwrap();
        receiver
            .engine
            .store
            .put_media(
                &short_blob.sha256,
                "application/vnd.zoen.page",
                &bytes,
                1_800_000_000_000,
            )
            .unwrap();
        longer
            .engine
            .store
            .put_media(
                &short_blob.sha256,
                "application/vnd.zoen.page",
                &bytes,
                1_800_000_000_000,
            )
            .unwrap();
        deliver(&mut shorter.engine, &legacy_event, Ingest::Applied);
        for event in if shorter_first {
            [&short_event, &long_event]
        } else {
            [&long_event, &short_event]
        } {
            deliver(&mut receiver.engine, event, Ingest::Applied);
            let short_expected = if event.client_id == short_event.client_id {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
            deliver(&mut shorter.engine, event, short_expected);
            let long_expected = if event.client_id == long_event.client_id {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
            deliver(&mut longer.engine, event, long_expected);
        }
        assert_eq!(short_event.seen, short_seen);
        assert_eq!(long_event.seen, long_seen);
        let target = receiver
            .engine
            .plan_timeline(&receiver.item)
            .unwrap()
            .document
            .unwrap();
        let a = short_session.read(&shorter.engine).unwrap();
        let b = long_session.read(&longer.engine).unwrap();
        assert_eq!(blocks(&a), target.blocks().unwrap());
        assert_eq!(a.view, b.view);
        assert!(!a.unsaved);
        assert!(!b.unsaved);
        let projected = target.view().unwrap().to_plan_doc();
        assert_eq!(projected.title, RAW);
        assert_eq!(projected.sections[0].lines[0].text, "z".repeat(55_000));
        assert_eq!(projected.sections[0].lines[0].cost_cents, 777);
        assert!(projected.sections[0].lines[0].done);
        assert_eq!(projected.budget_cents, Some(9_999));
        assert_eq!(
            target.revision().unwrap(),
            plan_doc(&short_session.live).unwrap().revision().unwrap()
        );
        assert_eq!(
            target.revision().unwrap(),
            plan_doc(&long_session.live).unwrap().revision().unwrap()
        );
        assert!(blocks(&short_accept.current)
            .iter()
            .any(|b| b.plain_text().len() == 55_000));
    }
}

pub(super) fn backup_export_excludes_actual_page_and_plan_private_journals_without_mutating_source(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for kind in [ItemKind::Page, ItemKind::Plan] {
        let (f, _, _, private) = private_document_backup_source(kind);
        let ciphertext = f.cipher().unwrap();
        let head = f.engine.logs[&f.space]
            .head()
            .map(|head| (head.seq, head.hash));
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        let db = Database::new()?;
        std::fs::write(db.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let connection = rusqlite::Connection::open(db.path()).unwrap();
        let rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'page.draft.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            rows, 0,
            "a backup must not carry device-private document journals"
        );
        assert_eq!(f.cipher().as_deref(), Some(ciphertext.as_str()));
        let mut source_session = f.session();
        let source = source_session.read(&f.engine).unwrap();
        assert_eq!(source.view, private);
        assert!(source.can_edit && source.unsaved);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        assert_eq!(
            f.engine.logs[&f.space]
                .head()
                .map(|head| (head.seq, head.hash)),
            head
        );
    })
}

pub(super) fn backup_restore_strips_authentic_old_device_page_and_plan_journals_before_edit_admission(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(for kind in [ItemKind::Page, ItemKind::Plan] {
        let (f, header, canonical, private) = private_document_backup_source(kind);
        let ciphertext = f.cipher().unwrap();
        let row = row_id(&f.engine, f.engine.item_state(&f.item).unwrap()).unwrap();
        let original_device = f.engine.account().unwrap().device.clone();
        let head = f.engine.logs[&f.space]
            .head()
            .map(|head| (head.seq, head.hash));
        let db = Database::new()?;
        std::fs::write(db.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let connection = rusqlite::Connection::open(db.path()).unwrap();
        // Test restore independently of export using the real old device's sealed row.
        connection
            .execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES (?1,?2)",
                rusqlite::params![row, ciphertext],
            )
            .unwrap();
        drop(connection);
        let mut restored = Engine::open(":memory:").unwrap();
        restored
            .restore_snapshot(
                &header,
                &std::fs::read(db.path()).unwrap(),
                event_log::Signer::generate()?,
            )
            .unwrap();
        assert_eq!(restored.account().unwrap().identity, f.owner);
        assert_ne!(restored.account().unwrap().device, original_device);
        assert!(restored.store.meta(&row).unwrap().is_none());
        assert_eq!(
            restored.logs[&f.space]
                .head()
                .map(|head| (head.seq, head.hash)),
            head
        );
        assert_eq!(restored.item(&f.item).unwrap().version, 1);
        let restored = Fixture {
            engine: restored,
            owner: f.owner.clone(),
            space: f.space.clone(),
            item: f.item.clone(),
        };
        let mut session = restored.session();
        let current = session.read(&restored.engine).unwrap();
        assert_eq!(current.view, canonical);
        assert!(!current.unsaved);
        assert!(
            current.can_edit,
            "foreign ciphertext must not lock the canonical document"
        );
        assert!(current.save_error.is_none());
        let accepted = private_document_backup_edit(
            &restored,
            &mut session,
            &current,
            "new-device-edit",
            "Fresh edit after recovery",
        )
        .unwrap();
        assert!(accepted.current.unsaved && accepted.current.can_edit);
        assert_ne!(accepted.current.view, canonical);
        assert_ne!(restored.cipher().as_deref(), Some(ciphertext.as_str()));
        assert_eq!(restored.engine.item(&restored.item).unwrap().version, 1);
        let mut original_session = f.session();
        assert_eq!(original_session.read(&f.engine).unwrap().view, private);
        assert_eq!(f.cipher().as_deref(), Some(ciphertext.as_str()));
    })
}
