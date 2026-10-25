//! Whole capture queue cases on the same original parent fixture.

use super::*;

pub(super) fn fifo_keeps_all_snapshots_and_advances_only_its_own_base_while_the_origin_stays_pinned(
) {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    let b = stage(&f, &a.draft.revision, "editor", "b");
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "b".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    let first = commit(&f, "editor", "a");
    assert_eq!(first.version, 2);
    let after_a = read(&f);
    assert_eq!(
        after_a
            .pending
            .iter()
            .map(|c| c.capture_id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    assert_eq!(after_a.base_version, Some(2));
    assert_eq!(after_a.origin_version, Some(1));
    // The same QuickLook editor captures C while its A commit ACK is still unseen.
    let c = stage(&f, &after_a.revision, "editor", "c");
    assert_eq!(
        c.draft.origin_sha256.as_deref(),
        Some(f.origin.sha256.as_str())
    );
    assert_eq!(
        stage(&f, "0", "editor", "a").accepted_revision,
        a.accepted_revision
    );
    assert_eq!(stage(&f, "0", "editor", "a").draft, c.draft);
    assert!(
        export(&f, &c.draft.revision, "a").is_err(),
        "accepted replay must never resurrect consumed bytes"
    );
    assert_eq!(commit(&f, "editor", "b").version, 3);
    assert_eq!(commit(&f, "editor", "c").version, 4);
    for (version, capture) in [(1, "original"), (2, "a"), (3, "b"), (4, "c")] {
        assert_eq!(
            f.engine
                .file_bytes(f.scope.item_id.clone(), Some(version))
                .unwrap()
                .unwrap(),
            pdf(capture)
        );
    }
    let empty = read(&f);
    assert!(empty.pending.is_empty());
    assert_eq!(empty.origin_version, Some(1));
    assert_eq!(empty.base_version, Some(4));
    let d = stage(&f, &empty.revision, "editor", "d");
    assert_eq!(d.draft.origin_version, Some(1));
    assert_eq!(commit(&f, "editor", "d").version, 5);
    assert_eq!(b.draft.pending.len(), 2);
}

pub(super) fn lost_commit_ack_reopens_and_returns_its_exact_receipt_without_touching_newer_edits_or_versions(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let a = stage(&f, "0", "editor", "a");
    let result = commit(&f, "editor", "a");
    let b = stage(&f, &read(&f).revision, "editor", "b");
    // A separate writer changes the canonical file. A retry of accepted A is still safe.
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote"),
            None,
            "Remote".into(),
        )
        .unwrap();
    role(&f, Role::Reader);
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault).unwrap());
    let before = counts(&engine);
    assert_eq!(
        engine
            .commit_file_edit_draft(
                scope.clone(),
                "editor".into(),
                "a".into(),
                "Different retry note".into()
            )
            .unwrap(),
        result
    );
    assert_eq!(engine.file_edit_draft(scope.clone()).unwrap(), b.draft);
    assert_eq!(counts(&engine), before);
    let replay = engine
        .stage_file_edit_draft(
            scope.clone(),
            "0".into(),
            "editor".into(),
            "a".into(),
            1,
            a.draft.origin_sha256.unwrap(),
            pdf("a"),
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, a.accepted_revision);
    assert_eq!(replay.draft, b.draft);
    assert!(engine
        .commit_file_edit_draft(scope, "editor".into(), "b".into(), "Annotations".into())
        .is_err());
    assert_eq!(counts(&engine), before);
    drop(engine);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn remote_change_preserves_late_full_capture_but_never_rebases_or_publishes_it() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote"),
            None,
            "Remote".into(),
        )
        .unwrap();
    let b = stage(&f, &a.draft.revision, "editor", "b");
    let before = counts(&f.engine);
    let retained = rows(&f);
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(rows(&f), retained);
    assert_eq!(export(&f, &b.draft.revision, "b").unwrap(), pdf("b"));
    assert_eq!(
        f.engine
            .file_bytes(f.scope.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        pdf("remote")
    );
}

pub(super) fn editor_claims_are_exclusive_until_empty_and_preserve_historical_origin_without_rebasing(
) {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "other".into(),
            "other".into(),
            1,
            f.origin.sha256.clone(),
            pdf("other")
        )
        .is_err());
    commit(&f, "editor", "a");
    let empty = read(&f);
    let other = f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            empty.revision,
            "other".into(),
            "other".into(),
            1,
            f.origin.sha256.clone(),
            pdf("other"),
        )
        .unwrap();
    assert_eq!(other.draft.origin_version, Some(1));
    assert_eq!(other.draft.base_version, Some(1));
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "other".into(),
            "other".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            other.draft.revision,
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
}

pub(super) fn first_capture_after_a_remote_change_is_durable_but_can_never_overwrite_that_remote_version(
) {
    let f = fixture(":memory:");
    f.engine
        .file_new_version(
            f.scope.item_id.clone(),
            pdf("remote-before-first-capture"),
            None,
            "Remote".into(),
        )
        .unwrap();
    let before = counts(&f.engine);
    let captured = stage(&f, "0", "opened-before-remote", "late-first");
    assert_eq!(captured.draft.base_version, Some(1));
    assert_eq!(captured.draft.origin_version, Some(1));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(
        export(&f, &captured.draft.revision, "late-first").unwrap(),
        pdf("late-first")
    );
    assert!(matches!(
        f.engine.commit_file_edit_draft(
            f.scope.clone(),
            "opened-before-remote".into(),
            "late-first".into(),
            "Annotations".into()
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(read(&f), captured.draft);
    assert_eq!(
        f.engine
            .file_bytes(f.scope.item_id.clone(), None)
            .unwrap()
            .unwrap(),
        pdf("remote-before-first-capture")
    );
    assert_eq!(counts(&f.engine), before);
}
