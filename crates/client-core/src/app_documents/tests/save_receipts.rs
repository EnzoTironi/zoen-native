//! Whole save receipts cases on the same original parent fixture.

use super::*;

pub(super) fn saves_replay_exact_accepted_version_after_newer_edits_and_reject_reused_intents() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let one = scene("First");
    let two = scene("Second");
    let accepted = save(&f, &initial, &one, "save1").unwrap();
    let newer = save(&f, &accepted, &two, "save2").unwrap();
    role(&f, Role::Reader);
    let before = counts(&f.engine);
    let replay = save(&f, &initial, &one, "save1").unwrap();
    assert_eq!(replay.version, accepted.version);
    assert_eq!(replay.current_version, newer.version);
    assert_eq!(replay.scene_json, one);
    assert!(save(&f, &initial, &two, "save1").is_err());
    assert!(save(&f, &newer, &scene("Denied"), "save3").is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.app_document_read(f.item.clone()).unwrap(), newer);
}

pub(super) fn unchanged_scene_ack_adds_no_version_and_receipt_eviction_never_rebases_stale_work() {
    let f = fixture(":memory:");
    let initial = f.engine.app_document_read(f.item.clone()).unwrap();
    let first = save(&f, &initial, &scene("Changed"), "first").unwrap();
    for n in 0..MAX_RECEIPTS {
        let accepted = save(&f, &first, &first.scene_json, &format!("noop{n}")).unwrap();
        assert_eq!(accepted.version, 2);
    }
    assert_eq!(
        f.engine.lock().item_state(&f.item).unwrap().versions.len(),
        2
    );
    assert!(matches!(
        save(&f, &initial, &first.scene_json, "first"),
        Err(CoreError::Stale { .. })
    ));
}

pub(super) fn receipt_storage_failure_rolls_back_chunks_event_key_outbox_and_retry_is_one_version()
{
    let f = fixture(":memory:");
    let base = f.engine.app_document_read(f.item.clone()).unwrap();
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER refuse_drawing_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(save(&f, &base, &scene("Retry"), "retry").is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.app_document_read(f.item.clone()).unwrap(), base);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_drawing_receipt")
        .unwrap();
    assert_eq!(
        save(&f, &base, &scene("Retry"), "retry").unwrap().version,
        2
    );
    assert_eq!(
        save(&f, &base, &scene("Retry"), "retry").unwrap().version,
        2
    );
}
