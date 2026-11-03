use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn confirmed_forward_carries_the_reviewed_source_author() -> TestResult {
    let f = fixture(":memory:");
    let reviewed = f.client.forward_review(f.scope.clone())?;
    let accepted = confirm(&f, &reviewed)?.accepted.ok_or("accepted forward")?;
    assert_eq!(
        serde_json::to_value(&accepted)?["forwarded_author"],
        "Forward author"
    );
    let info = f.client.lock().message_info(&crate::MessageInfoScope {
        expected_identity_id: f.scope.expected_identity_id.clone(),
        expected_device_id: f.scope.expected_device_id.clone(),
        space_id: f.scope.target_space.clone(),
        message_id: accepted.id.clone(),
    })?;
    assert!(info.forwarded);
    assert_eq!(info.forwarded_author, accepted.forwarded_author);
    Ok(())
}

#[test]
fn original_snapshot_survives_rename_reforward_and_cold_reopen() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("attribution-test")?);
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join("account.sqlite").to_string_lossy().into_owned();
    let f = fixture(&path);
    let reviewed = f.client.forward_review(f.scope.clone())?;
    let accepted = confirm(&f, &reviewed)?.accepted.ok_or("forward")?;
    let third = f
        .client
        .create_group_with("Third".into(), vec![], PrivacyDto::Closed)?;
    let next_scope = ForwardScope {
        source_space: f.scope.target_space.clone(),
        message_id: accepted.id.clone(),
        target_space: third.clone(),
        ..f.scope.clone()
    };
    let next_review = f.client.forward_review(next_scope.clone())?;
    // Changing the intermediate author's display must not change the preserved
    // original attribution or invalidate an unchanged reviewed output.
    f.client
        .lock()
        .update_my_profile("Intermediate name", "", crate::PhotoChange::Keep)?;
    let copied = f
        .client
        .confirm_forward(
            next_scope,
            next_review.operation_id,
            next_review.source_revision,
        )?
        .accepted
        .ok_or("second accepted")?;
    assert_eq!(copied.forwarded_author.as_deref(), Some("Forward author"));
    assert_eq!(copied.author.name, "Intermediate name");
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    let rows = client.timeline(third)?;
    let restored = rows.iter().find(|e| e.id == copied.id).ok_or("restored")?;
    assert_eq!(restored.forwarded_author, copied.forwarded_author);
    let queued = client.lock().store.outbox()?;
    let event = &queued
        .iter()
        .find(|pending| pending.event.client_id == accepted.id)
        .ok_or("signed pending output")?
        .event;
    event_log::verify_author(event)?;
    let bytes = serde_json::to_string(&event.body)?;
    assert!(bytes.contains("Forward author"));
    assert!(!bytes.contains(&scope.source_space));
    assert!(!bytes.contains(&scope.message_id));
    assert!(restored.reply_to.is_none());
    assert!(restored.in_thread.is_none());
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn changed_display_name_requires_fresh_review_before_first_publication() -> TestResult {
    let f = fixture(":memory:");
    let reviewed = f.client.forward_review(f.scope.clone())?;
    f.client
        .lock()
        .update_my_profile("Different author name", "", crate::PhotoChange::Keep)?;
    let before = counts(&f);
    assert!(matches!(
        confirm(&f, &reviewed),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f), before);
    f.client
        .dismiss_forward(f.scope.clone(), reviewed.operation_id, None)?;
    let fresh = f.client.forward_review(f.scope.clone())?;
    assert_ne!(fresh.source_revision, reviewed.source_revision);
    assert_eq!(
        confirm(&f, &fresh)?
            .accepted
            .ok_or("accepted")?
            .forwarded_author
            .as_deref(),
        Some("Different author name")
    );
    Ok(())
}

#[test]
fn legacy_unattributed_forward_does_not_acquire_intermediate_author() -> TestResult {
    let f = fixture(":memory:");
    let legacy = {
        let mut engine = f.client.lock();
        let me = engine.me_id()?;
        engine.append(
            &f.scope.source_space,
            &me,
            types::EventBody::MessagePosted {
                message: types::new_id("msg")?,
                text: "Legacy forwarded text".into(),
                attaches: None,
                media: None,
                reply: None,
                forwarded: true,
                forwarded_author: None,
            },
        )?
    };
    let copied = f.client.forward_message(
        f.scope.source_space.clone(),
        legacy.client_id,
        f.scope.target_space.clone(),
    )?;
    assert!(copied.forwarded);
    assert!(copied.forwarded_author.is_none());
    Ok(())
}

#[test]
fn text_file_and_implicit_voice_use_the_same_forward_attribution() -> TestResult {
    for kind in ["text", "file", "voice"] {
        let f = fixture(":memory:");
        let source = match kind {
            "file" => f.client.lock().send_file_message_with_caption(
                &f.scope.source_space,
                "report.pdf",
                "application/pdf",
                b"bounded opaque test file",
                None,
                None,
                false,
                "first-file",
                "Keep this caption",
            )?,
            "voice" => f.client.lock().send_voice_note(
                &f.scope.source_space,
                b"bounded opaque test audio",
                "audio/mp4",
                2300,
                &[0.2, 0.7],
                "Keep the transcript",
                None,
                false,
            )?,
            _ => f
                .client
                .timeline(f.scope.source_space.clone())?
                .into_iter()
                .find(|e| e.id == f.scope.message_id)
                .ok_or("text")?,
        };
        let scope = ForwardScope {
            message_id: source.id,
            ..f.scope.clone()
        };
        let reviewed = f.client.forward_review(scope.clone())?;
        let accepted = f
            .client
            .confirm_forward(scope, reviewed.operation_id, reviewed.source_revision)?
            .accepted
            .ok_or("forward")?;
        assert!(accepted.forwarded, "{kind}");
        assert_eq!(
            accepted.forwarded_author.as_deref(),
            Some("Forward author"),
            "{kind}"
        );
        if kind == "voice" {
            assert_eq!(
                accepted.voice.ok_or("voice")?.transcript,
                "Keep the transcript"
            );
        }
        if kind == "file" {
            assert!(
                matches!(accepted.kind, crate::EntryKind::Message { text, .. } if text == "Keep this caption")
            );
        }
    }
    Ok(())
}

#[test]
fn global_deletion_and_private_hidden_anchor_remove_forward_attribution() -> TestResult {
    for private in [false, true] {
        let f = fixture(":memory:");
        let reviewed = f.client.forward_review(f.scope.clone())?;
        let accepted = confirm(&f, &reviewed)?.accepted.ok_or("forward")?;
        if private {
            f.client.delete_messages(crate::MessageDeletion {
                expected_identity_id: f.scope.expected_identity_id.clone(),
                expected_device_id: f.scope.expected_device_id.clone(),
                space_id: f.scope.target_space.clone(),
                for_me: Some(true),
                targets: vec![crate::MessageDeletionTarget {
                    message_id: accepted.id.clone(),
                    expected_revision: accepted.text_revision.ok_or("revision")?,
                }],
            })?;
        } else {
            f.client
                .delete_message(f.scope.target_space.clone(), accepted.id.clone())?;
        }
        let engine = f.client.lock();
        let stored = engine
            .space_state(&f.scope.target_space)?
            .entries
            .iter()
            .find(|e| e.id() == accepted.id)
            .ok_or("entry")?;
        assert!(engine.entry_dto(stored).forwarded_author.is_none());
    }
    Ok(())
}

#[test]
fn malformed_forward_metadata_is_refused_by_the_shared_admission() -> TestResult {
    for (forwarded, name) in [
        (false, "Plausible name"),
        (true, "Fake\nname"),
        (true, "Fake\u{202e}name"),
    ] {
        let body = types::EventBody::MessagePosted {
            message: types::new_id("msg")?,
            text: "content".into(),
            attaches: None,
            media: None,
            reply: None,
            forwarded,
            forwarded_author: Some(name.into()),
        };
        assert!(!metadata_valid(&body));
        let file = types::FileDoc {
            app_origin: None,
            name: "voice.m4a".into(),
            path: "VoiceNotes/a.m4a".into(),
            mime: "audio/mp4".into(),
            bytes: 1,
            sha256: "a".repeat(64),
            chunks: vec![],
            thumb: None,
            voice: Some(types::VoiceMeta {
                forwarded,
                forwarded_author: Some(name.into()),
                ..Default::default()
            }),
        };
        // Creation and later file versions use the same metadata admission.
        for created in [true, false] {
            let body = if created {
                types::EventBody::ItemCreated {
                    item: "file".into(),
                    kind: types::ItemKind::File,
                    content: types::ItemContent::File(file.clone()),
                    origin: "voice".into(),
                }
            } else {
                types::EventBody::ItemVersioned {
                    item: "file".into(),
                    content: types::ItemContent::File(file.clone()),
                    note: "edit".into(),
                }
            };
            assert!(!metadata_valid(&body));
        }
    }
    Ok(())
}
