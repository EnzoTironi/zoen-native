//! Complete publication scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn accepted_cold_retry_returns_same_entries_and_preserves_a_newer_unsent_selection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let newer = save(
        &f,
        &sent.draft.revision,
        &attachments(&["new"], None),
        vec![input("new")],
    );
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let e = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(e.unlock(vault).unwrap());
    let before = counts(&e);
    let retried = e
        .send_media_draft(scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(retried.entries, sent.entries);
    assert_eq!(retried.draft, newer);
    let json: serde_json::Value = serde_json::from_str(
        &e.send_media_draft_json(scope.clone(), draft.revision.clone())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["entries"][0]["id"], sent.entries[0].id);
    assert_eq!(json["draft"]["revision"], newer.revision);
    assert_eq!(counts(&e), before);
    let saved = e
        .save_media_draft(
            scope.clone(),
            "0".into(),
            attachments(&["a", "b"], None),
            vec![input("a"), input("b")],
        )
        .unwrap();
    assert_eq!(saved.accepted_revision, draft.revision);
    assert_eq!(saved.draft, newer);
    assert_eq!(counts(&e), before);
    assert_eq!(
        e.media_draft_asset(scope, newer.revision, "new".into())
            .unwrap(),
        input("new").bytes
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn accepted_retry_precedes_changed_writer_reply_and_clear_retains_receipt() {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())
        .unwrap();
    let draft = save(
        &f,
        "0",
        &attachments(&["a"], Some(&root.id)),
        vec![input("a")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    let clear = save(&f, &sent.draft.revision, "null", vec![]);
    f.engine
        .delete_message(f.scope.space_id.clone(), root.id)
        .unwrap();
    f.engine
        .lock()
        .append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            types::EventBody::MemberAdded {
                identity: f.scope.expected_identity_id.clone(),
                role: types::Role::Reader,
            },
        )
        .unwrap();
    let before = counts(&f.engine);
    let retried = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(
        retried
            .entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        sent.entries
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(retried.entries[0].kind, sent.entries[0].kind);
    assert_eq!(
        retried.entries[0].reply_to.as_ref().unwrap().id,
        sent.entries[0].reply_to.as_ref().unwrap().id
    );
    assert!(
        retried.entries[0]
            .reply_to
            .as_ref()
            .unwrap()
            .text
            .is_empty(),
        "retry returns the current canonical quote tombstone"
    );
    assert_eq!(retried.draft, clear);
    assert_eq!(counts(&f.engine), before);
}
