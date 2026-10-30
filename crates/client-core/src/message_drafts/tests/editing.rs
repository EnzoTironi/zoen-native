use super::*;

fn correction(source: &TimelineEntry, text: &str) -> MessageDraftEdit {
    MessageDraftEdit {
        message_id: source.id.clone(),
        source_revision: source.text_revision.clone().expect("signed revision"),
        base_text: "Original".into(),
        text: text.into(),
        is_caption: false,
    }
}
fn save_edit(f: &Fixture, edit: Option<MessageDraftEdit>) -> R<MessageDraftDto> {
    let current = read(f);
    f.engine.lock().save_message_draft_content(
        &f.scope,
        &current.revision,
        DraftContent {
            text: current.text,
            reply_to: current.reply_to,
            reply_quote: current.reply_quote,
            shared_reply: current.shared_reply,
            edit,
        },
    )
}

#[test]
fn correction_and_normal_reply_share_one_sealed_record_and_recover_cold(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("draft.sqlite").to_string_lossy().into_owned();
    let f = fixture(&path);
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    save(
        &f,
        "0",
        "Independent unsent message",
        Some(source.id.clone()),
    );
    let before = counts(&f.engine);
    let edit = correction(&source, "Private correction 😀");
    let saved = save_edit(&f, Some(edit.clone()))?;
    assert_eq!(
        counts(&f.engine),
        before,
        "a private edit is not a signed conversation event"
    );
    let (_, cipher) = row(&f);
    for private in [
        "Private correction",
        "Independent unsent message",
        "Original",
    ] {
        assert!(!cipher.contains(private));
    }
    let vault = f.vault.clone();
    let scope = f.scope.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(client.lock().message_draft(&scope)?, saved);
    assert_eq!(
        client
            .timeline(scope.space_id.clone())?
            .iter()
            .find(|e| e.id == source.id)
            .expect("source")
            .kind,
        source.kind,
        "recovery must not publish the correction"
    );
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn confirmation_consumes_only_the_edit_and_cold_retry_never_duplicates(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("draft.sqlite").to_string_lossy().into_owned();
    let f = fixture(&path);
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    save(&f, "0", "Normal draft", Some(source.id.clone()));
    let saved = save_edit(&f, Some(correction(&source, "Correction")))?;
    let before = counts(&f.engine);
    let accepted = f
        .engine
        .lock()
        .send_message_edit_draft(&f.scope, &saved.revision)?;
    assert!(accepted.draft.edit.is_none());
    assert!(
        matches!(&accepted.entry.kind, crate::EntryKind::Message { text, .. } if text == "Correction")
    );
    assert_eq!(accepted.draft.text, "Normal draft");
    assert_eq!(accepted.draft.reply_to.as_deref(), Some(source.id.as_str()));
    assert_eq!(
        counts(&f.engine),
        (before.0, before.1 + 1),
        "one optimistic signed annotation is queued"
    );
    let after = counts(&f.engine);
    assert_eq!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &saved.revision)?
            .draft,
        accepted.draft
    );
    assert_eq!(counts(&f.engine), after);
    let newer = save(&f, &accepted.draft.revision, "Newer unsent text", None);
    let vault = f.vault.clone();
    let scope = f.scope.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(
        client
            .lock()
            .send_message_edit_draft(&scope, &saved.revision)?
            .draft,
        newer
    );
    assert_eq!(counts(&client), after);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn failed_consumption_rolls_back_annotation_and_ciphertext_then_exact_retry_succeeds(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let saved = save_edit(&f, Some(correction(&source, "Correction")))?;
    let before = counts(&f.engine);
    let cipher = row(&f);
    f.engine.lock().store.conn().execute_batch("CREATE TEMP TRIGGER refuse_edit_draft BEFORE UPDATE ON meta BEGIN SELECT RAISE(ABORT, 'fixture storage failure'); END;")?;
    assert!(f
        .engine
        .lock()
        .send_message_edit_draft(&f.scope, &saved.revision)
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(row(&f), cipher);
    assert_eq!(read(&f), saved);
    assert_eq!(
        f.engine
            .timeline(f.scope.space_id.clone())?
            .iter()
            .find(|e| e.id == source.id)
            .expect("source")
            .kind,
        source.kind
    );
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER refuse_edit_draft;")?;
    f.engine
        .lock()
        .send_message_edit_draft(&f.scope, &saved.revision)?;
    assert_eq!(
        counts(&f.engine),
        (before.0, before.1 + 1),
        "one optimistic signed annotation is queued"
    );
    Ok(())
}

#[test]
fn changed_source_preserves_correction_until_explicit_review_and_rejects_old_cas() -> R<()> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let mut edit = correction(&source, "Correction");
    let saved = save_edit(&f, Some(edit.clone()))?;
    f.engine.edit_message(
        f.scope.space_id.clone(),
        source.id.clone(),
        "Concurrent change".into(),
    )?;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &saved.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    edit.text = "Correction kept after conflict".into();
    let typed = save_edit(&f, Some(edit.clone()))?;
    assert_eq!(read(&f), typed);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &saved.revision),
        Err(CoreError::Stale { .. })
    ));
    let source = f
        .engine
        .timeline(f.scope.space_id.clone())?
        .into_iter()
        .find(|e| e.id == source.id)
        .expect("source");
    edit.source_revision = source.text_revision.expect("new signed revision");
    edit.base_text = "Concurrent change".into();
    let reviewed = save_edit(&f, Some(edit))?;
    f.engine
        .lock()
        .send_message_edit_draft(&f.scope, &reviewed.revision)?;
    assert_eq!(
        counts(&f.engine),
        (before.0, before.1 + 1),
        "one optimistic signed annotation is queued"
    );
    Ok(())
}

#[test]
fn cancellation_fences_old_confirmation_and_deleted_source_keeps_private_typing() -> R<()> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let mut edit = correction(&source, "Correction");
    let saved = save_edit(&f, Some(edit.clone()))?;
    let cleared = save_edit(&f, None)?;
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .send_message_edit_draft(&f.scope, &saved.revision)
        .is_err());
    assert_eq!(read(&f), cleared);
    assert_eq!(counts(&f.engine), before);
    save_edit(&f, Some(edit.clone()))?;
    f.engine
        .delete_message(f.scope.space_id.clone(), source.id)?;
    edit.text = "Keep my correction even after deletion".into();
    let retained = save_edit(&f, Some(edit))?;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &retained.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(read(&f), retained);
    assert_eq!(counts(&f.engine), before);
    Ok(())
}

#[test]
fn private_draft_metadata_never_authorizes_a_stale_source_empty_body_or_wrong_thread() -> R<()> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let before = counts(&f.engine);
    for forged_caption in [false, true] {
        let mut edit = correction(&source, "Correction");
        if forged_caption {
            edit.is_caption = true;
            edit.text.clear();
        } else {
            edit.source_revision = "unreviewed-revision".into();
            edit.base_text = "Untrusted preview".into();
        }
        let saved = save_edit(&f, Some(edit))?;
        assert!(matches!(
            f.engine
                .lock()
                .send_message_edit_draft(&f.scope, &saved.revision),
            Err(CoreError::DraftRejected { .. })
        ));
        assert_eq!(read(&f), saved);
        assert_eq!(counts(&f.engine), before);
    }
    let other_root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Different root".into())?;
    let mut scope = f.scope.clone();
    scope.thread_root_id = Some(other_root.id);
    let saved = f.engine.lock().save_message_draft_content(
        &scope,
        "0",
        DraftContent {
            text: String::new(),
            edit: Some(correction(&source, "Wrong thread")),
            reply_to: None,
            reply_quote: None,
            shared_reply: None,
        },
    )?;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&scope, &saved.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    Ok(())
}

#[test]
fn typing_after_acceptance_and_deletion_before_first_autosave_remain_durable_but_never_rebase_silently(
) -> R<()> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let mut edit = correction(&source, "First correction");
    let saved = save_edit(&f, Some(edit.clone()))?;
    f.engine
        .lock()
        .send_message_edit_draft(&f.scope, &saved.revision)?;
    edit.text = "New typing during the accepted request".into();
    let continued = save_edit(&f, Some(edit.clone()))?;
    assert_eq!(read(&f), continued);
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &continued.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    save_edit(&f, None)?;
    f.engine
        .delete_message(f.scope.space_id.clone(), source.id)?;
    let deleted_before_save = save_edit(&f, Some(edit))?;
    assert_eq!(read(&f), deleted_before_save);
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_edit_draft(&f.scope, &deleted_before_save.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    Ok(())
}

#[tokio::test]
async fn thread_application_commands_consume_the_child_edit_without_touching_main_composition(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Thread root".into())?;
    let child = f.engine.send_reply(
        f.scope.space_id.clone(),
        "Original".into(),
        root.id.clone(),
        true,
    )?;
    let main = save(&f, "0", "Independent main draft", Some(root.id.clone()));
    let mut scope = f.scope.clone();
    scope.thread_root_id = Some(root.id.clone());
    let mut request = serde_json::json!({
        "op": "save_message_draft", "expected_identity_id": scope.expected_identity_id,
        "expected_device_id": scope.expected_device_id, "space_id": scope.space_id,
        "thread_root_id": scope.thread_root_id, "expected_revision": "0",
        "text": "Independent thread draft", "reply_to": null, "edit": correction(&child, "Corrected thread child"),
    });
    let saved: MessageDraftDto =
        serde_json::from_str(&f.engine.client_request(request.to_string()).await?)?;
    assert_eq!(read(&f), main);
    request["op"] = "send_message_edit_draft".into();
    request["expected_revision"] = saved.revision.into();
    for key in ["text", "reply_to", "edit"] {
        request.as_object_mut().expect("command object").remove(key);
    }
    let accepted: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(request.to_string()).await?)?;
    assert_eq!(accepted["entry"]["in_thread"], root.id);
    assert_eq!(
        accepted["entry"]["kind"]["Message"]["text"],
        "Corrected thread child"
    );
    assert_eq!(accepted["draft"]["text"], "Independent thread draft");
    assert!(accepted["draft"].get("edit").is_none());
    assert_eq!(read(&f), main);
    Ok(())
}
