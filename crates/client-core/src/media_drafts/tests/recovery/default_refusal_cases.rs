//! Complete default refusal scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn failed_media_recovery_default_ws_file_rejection_is_durable_and_explicit() {
    let dir = temp();
    let path = dir.join("default-ws-refusal.sqlite");
    let f = fixture(path.to_str().unwrap());
    media_contract_confirm_scope(&f);
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    assert_eq!(sent.entries[0].media, None);
    media_contract_upload_all(&f);
    let original = f.engine.lock().store.outbox().unwrap();
    assert_eq!(original.len(), 2, "the real send queues File and Message");
    let file = original
        .iter()
        .find(|row| {
            matches!(
                row.event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            )
        })
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&file.event.client_id, "permanent File refusal", true, None)
        .user_failed());
    let refused = f.engine.lock().store.outbox().unwrap();
    for source in &original {
        let retained = refused
            .iter()
            .find(|row| row.event.client_id == source.event.client_id)
            .unwrap();
        assert!(
            retained.failed,
            "permanent File refusal must also fail its default-media Message"
        );
        assert_eq!(retained.event, source.event, "signed source is immutable");
    }
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault.clone()).unwrap());
    let f = Fixture {
        engine,
        vault,
        scope,
    };
    let retained = f
        .engine
        .timeline(f.scope.space_id.clone())
        .unwrap()
        .into_iter()
        .find(|row| row.id == sent.entries[0].id)
        .unwrap();
    assert_eq!(retained.delivery, crate::Delivery::Failed);
    assert_eq!(retained.kind, sent.entries[0].kind);
    assert_eq!(retained.media, None);
    let before = public_state(&f);
    let recovered = recover(&f, &retained.id).unwrap();
    assert_eq!(
        public_state(&f),
        before,
        "recovery does not resend or rewrite"
    );
    let Content::Attachments { items, options, .. } =
        serde_json::from_str::<Content>(&recovered.content_json).unwrap()
    else {
        panic!("default attachment must recover as an attachment draft")
    };
    assert!(options.is_default());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].caption, "Private caption a");
    assert_ne!(items[0].mutation_id, "send-a");
    assert_eq!(asset(&f, &recovered, &items[0].asset_id), input("a").bytes);
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let reopened = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    assert!(reopened.unlock(vault).unwrap());
    assert_eq!(reopened.media_draft(scope.clone()).unwrap(), recovered);
    assert_eq!(
        reopened
            .timeline(scope.space_id)
            .unwrap()
            .into_iter()
            .find(|row| row.id == retained.id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn failed_media_recovery_default_ws_refusal_is_atomic_and_keeps_other_pending_files() {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f);
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert!(sent.entries.iter().all(|entry| entry.media.is_none()));
    media_contract_upload_all(&f);
    let file = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| {
            matches!(&row.event.body, types::EventBody::ItemCreated {
                content: types::ItemContent::File(file), ..
            } if file.name == "a.pdf")
        })
        .unwrap()
        .event;
    let before = public_state(&f);
    f.engine.lock().store.conn().execute_batch(&format!("CREATE TEMP TRIGGER fail_default_refusal BEFORE UPDATE OF failed ON outbox WHEN NEW.client_id = '{}' BEGIN SELECT RAISE(ABORT,'dependent refusal failed'); END;", sent.entries[0].id)).unwrap();
    assert!(matches!(
        f.engine
            .lock()
            .reject(&file.client_id, "permanent File refusal", true, None),
        crate::sync::RejectionOutcome::StorageFailure(_)
    ));
    assert_eq!(public_state(&f), before, "File and Message fail atomically");
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_default_refusal")
        .unwrap();
    assert!(f
        .engine
        .lock()
        .reject(&file.client_id, "permanent File refusal", true, None)
        .user_failed());
    let rows = f.engine.lock().store.outbox().unwrap();
    assert!(
        rows.iter()
            .find(|row| row.event.client_id == sent.entries[0].id)
            .unwrap()
            .failed
    );
    let unrelated = rows
        .iter()
        .find(|row| row.event.client_id == sent.entries[1].id)
        .unwrap();
    assert!(!unrelated.failed);
    assert_eq!(unrelated.attempts, 0);
    assert!(unrelated.last_error.is_none());
    assert!(
        f.engine
            .lock()
            .outbox_envelopes()
            .iter()
            .any(|env| env.client_id() != file.client_id),
        "an independent default attachment still has a publishable File head"
    );
}
