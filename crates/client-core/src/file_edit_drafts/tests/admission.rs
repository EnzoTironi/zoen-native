//! Whole admission cases on the same original parent fixture.

use super::*;

pub(super) fn stage_replay_authenticates_all_bytes_and_binding_inputs_and_discard_is_explicit_cas()
{
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    for (editor, capture, version, sha, bytes) in [
        ("other", "a", 1, f.origin.sha256.clone(), pdf("a")),
        ("editor", "other", 1, f.origin.sha256.clone(), pdf("a")),
        ("editor", "a", 2, f.origin.sha256.clone(), pdf("a")),
        ("editor", "a", 1, "0".repeat(64), pdf("a")),
        (
            "editor",
            "a",
            1,
            f.origin.sha256.clone(),
            pdf("changed bytes"),
        ),
    ] {
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                editor.into(),
                capture.into(),
                version,
                sha,
                bytes
            )
            .is_err());
    }
    assert_eq!(read(&f), a.draft);
    assert!(f
        .engine
        .discard_file_edit_draft(f.scope.clone(), "0".into())
        .is_err());
    let cleared = f
        .engine
        .discard_file_edit_draft(f.scope.clone(), a.draft.revision.clone())
        .unwrap();
    assert!(cleared.pending.is_empty());
    assert_ne!(cleared.revision, "0");
    let replay = stage(&f, "0", "editor", "a");
    assert_eq!(replay.accepted_revision, a.accepted_revision);
    assert_eq!(replay.draft, cleared);
    assert!(export(&f, &cleared.revision, "a").is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(f.engine.item(f.scope.item_id.clone()).unwrap().version, 1);
    let empty = f
        .engine
        .discard_file_edit_draft(f.scope.clone(), cleared.revision)
        .unwrap();
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            "0".into(),
            "new".into(),
            "new".into(),
            1,
            f.origin.sha256.clone(),
            pdf("new")
        )
        .is_err());
    let claimed = stage(&f, &empty.revision, "new", "new");
    assert_eq!(claimed.draft.editor_id.as_deref(), Some("new"));
}

pub(super) fn exact_owner_device_item_space_membership_and_writer_checks_guard_every_boundary() {
    let f = fixture(":memory:");
    let a = stage(&f, "0", "editor", "a");
    let other_space = f
        .engine
        .create_group_with("Other PDF".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    for scope in [
        FileEditDraftScope {
            expected_identity_id: "foreign".into(),
            ..f.scope.clone()
        },
        FileEditDraftScope {
            expected_device_id: "foreign".into(),
            ..f.scope.clone()
        },
        FileEditDraftScope {
            space_id: other_space,
            ..f.scope.clone()
        },
    ] {
        assert!(f.engine.file_edit_draft(scope.clone()).is_err());
        assert!(f
            .engine
            .file_edit_draft_bytes(scope.clone(), a.draft.revision.clone(), "a".into())
            .is_err());
        assert!(f
            .engine
            .commit_file_edit_draft(
                scope.clone(),
                "editor".into(),
                "a".into(),
                "Annotations".into()
            )
            .is_err());
        assert!(f
            .engine
            .discard_file_edit_draft(scope, a.draft.revision.clone())
            .is_err());
    }
    role(&f, Role::Reader);
    let before = counts(&f.engine);
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(export(&f, &a.draft.revision, "a").unwrap(), pdf("a"));
    assert_eq!(counts(&f.engine), before);
    let removed = fixture(":memory:");
    let captured = stage(&removed, "0", "editor", "a");
    removed
        .engine
        .lock()
        .append(
            &removed.scope.space_id,
            &removed.scope.expected_identity_id,
            EventBody::MemberRemoved {
                identity: removed.scope.expected_identity_id.clone(),
            },
        )
        .unwrap();
    assert!(removed
        .engine
        .file_edit_draft(removed.scope.clone())
        .is_err());
    assert!(export(&removed, &captured.draft.revision, "a").is_err());
    assert!(removed
        .engine
        .discard_file_edit_draft(removed.scope.clone(), captured.draft.revision)
        .is_err());
}

pub(super) fn origin_hash_and_current_pdf_type_are_canonical_and_cannot_be_supplied_by_the_caller()
{
    let f = fixture(":memory:");
    let before = counts(&f.engine);
    for (version, sha) in [(1, "0".repeat(64)), (2, f.origin.sha256.clone())] {
        assert!(f
            .engine
            .stage_file_edit_draft(
                f.scope.clone(),
                "0".into(),
                "editor".into(),
                "a".into(),
                version,
                sha,
                pdf("a")
            )
            .is_err());
    }
    assert_eq!(counts(&f.engine), before);
    assert!(rows(&f).is_empty());
    let a = stage(&f, "0", "editor", "a");
    f.engine
        .file_new_version_typed(
            f.scope.item_id.clone(),
            b"synthetic image bytes".to_vec(),
            None,
            "Changed type".into(),
            "renamed.pdf".into(),
            "image/png".into(),
        )
        .unwrap();
    let before = counts(&f.engine);
    assert!(f
        .engine
        .stage_file_edit_draft(
            f.scope.clone(),
            a.draft.revision.clone(),
            "editor".into(),
            "b".into(),
            1,
            f.origin.sha256.clone(),
            pdf("b")
        )
        .is_err());
    assert!(f
        .engine
        .commit_file_edit_draft(
            f.scope.clone(),
            "editor".into(),
            "a".into(),
            "Annotations".into()
        )
        .is_err());
    assert_eq!(
        export(&f, &a.draft.revision, "a").unwrap(),
        pdf("a"),
        "private recovery remains possible after type changes"
    );
    assert_eq!(counts(&f.engine), before);
}
