//! Media replacement uses the same sealed custody, exact retries and composer authority.
use super::*;
use crate::message_drafts::{DraftContent, MessageDraftEdit};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn selection(
    source: &crate::TimelineEntry,
    normal: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(serde_json::json!({"kind":"attachments","reply_to":null,
        "edit_revision":source.text_revision.as_ref().ok_or("source revision")?,
        "normal_revision":normal,
        "items":[{"asset_id":"replacement","mutation_id":"replace-intent","caption":"New caption"}]
    })
    .to_string())
}
fn normal_edit(
    f: &Fixture,
    source: &crate::TimelineEntry,
) -> Result<crate::MessageDraftDto, Box<dyn std::error::Error>> {
    Ok(f.engine.lock().save_message_draft_content(
        &f.scope.normal(),
        "0",
        DraftContent {
            text: "Unsent normal text".into(),
            reply_to: Some(source.id.clone()),
            reply_quote: None,
            shared_reply: None,
            reply_intent: None,
            edit: Some(MessageDraftEdit {
                message_id: source.id.clone(),
                source_revision: source.text_revision.clone().ok_or("revision")?,
                base_text: "Original".into(),
                text: "Private caption".into(),
                is_caption: false,
            }),
        },
    )?)
}
#[test]
fn replacement_restores_private_assets_and_atomically_finishes_only_its_editor() -> TestResult {
    let dir = temp()?;
    let path = dir.join("edit.sqlite").to_string_lossy().into_owned();
    let f = fixture(&path);
    let original = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let normal = normal_edit(&f, &original)?;
    let normal_media = save(
        &f,
        "0",
        &attachments(&["normal"], None),
        vec![input("normal")],
    );
    let mut scope = f.scope.clone();
    scope.edit_message_id = Some(original.id.clone());
    let saved = f
        .engine
        .save_media_draft(
            scope.clone(),
            "0".into(),
            selection(&original, Some(&normal.revision))?,
            vec![input("replacement")],
        )?
        .draft;
    assert_eq!(read(&f), normal_media);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(client.media_draft(scope.clone())?, saved);
    assert_eq!(
        client.media_draft_asset(scope.clone(), saved.revision.clone(), "replacement".into())?,
        input("replacement").bytes
    );
    let result = client.send_media_draft(scope.clone(), saved.revision.clone())?;
    assert_eq!(result.entries[0].id, original.id);
    assert_eq!(result.draft.content_json, "null");
    let persisted = client.lock().message_draft(&scope.normal())?;
    assert!(persisted.edit.is_none());
    assert_eq!(persisted.text, normal.text);
    assert_eq!(persisted.reply_to, normal.reply_to);
    assert_eq!(
        client
            .lock()
            .send_message_edit_draft(&scope.normal(), &normal.revision)?
            .message_id,
        original.id
    );
    let before = counts(&client);
    assert_eq!(
        client
            .send_media_draft(scope.clone(), saved.revision)?
            .draft,
        result.draft
    );
    assert_eq!(counts(&client), before);
    let mut ordinary = scope;
    ordinary.edit_message_id = None;
    assert_eq!(client.media_draft(ordinary)?, normal_media);
    Ok(())
}
#[test]
fn replacement_failure_rolls_back_file_and_editor_then_retries_exactly_once() -> TestResult {
    let f = fixture(":memory:");
    let original = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let normal = normal_edit(&f, &original)?;
    let mut scope = f.scope.clone();
    scope.edit_message_id = Some(original.id.clone());
    let saved = f
        .engine
        .save_media_draft(
            scope.clone(),
            "0".into(),
            selection(&original, Some(&normal.revision))?,
            vec![input("replacement")],
        )?
        .draft;
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_edit BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ReplaceMedia%' BEGIN SELECT RAISE(ABORT,'edit failed'); END;")?;
    assert!(f
        .engine
        .send_media_draft(scope.clone(), saved.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.lock().message_draft(&scope.normal())?, normal);
    assert!(f.engine.media_draft(scope.clone())?.attempted);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_edit")?;
    f.engine
        .send_media_draft(scope.clone(), saved.revision.clone())?;
    let accepted = counts(&f.engine);
    f.engine.send_media_draft(scope, saved.revision)?;
    assert_eq!(counts(&f.engine), accepted);
    Ok(())
}
#[test]
fn stale_source_is_editable_and_newer_normal_work_survives_publication() -> TestResult {
    let f = fixture(":memory:");
    let original = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let normal = normal_edit(&f, &original)?;
    let mut scope = f.scope.clone();
    scope.edit_message_id = Some(original.id.clone());
    let saved = f
        .engine
        .save_media_draft(
            scope.clone(),
            "0".into(),
            selection(&original, Some(&normal.revision))?,
            vec![input("replacement")],
        )?
        .draft;
    let newer = f.engine.lock().save_message_draft_content(
        &scope.normal(),
        &normal.revision,
        DraftContent {
            text: "Newer work".into(),
            reply_to: None,
            reply_quote: None,
            shared_reply: None,
            reply_intent: None,
            edit: None,
        },
    )?;
    f.engine.send_media_draft(scope.clone(), saved.revision)?;
    assert_eq!(f.engine.lock().message_draft(&scope.normal())?, newer);
    let mut other = scope;
    other.edit_message_id = Some(original.id.clone());
    let current = f.engine.media_draft(other.clone())?;
    let stale = f
        .engine
        .save_media_draft(
            other.clone(),
            current.revision,
            selection(&original, None)?,
            vec![input("replacement")],
        )?
        .draft;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine.send_media_draft(other.clone(), stale.revision),
        Err(crate::CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert!(!f.engine.media_draft(other)?.attempted);
    Ok(())
}

#[test]
fn replacement_metadata_cannot_enter_an_ordinary_journal_or_change_scope() -> TestResult {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "Original".into())?;
    let json = selection(&source, None)?;
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            json.clone(),
            vec![input("replacement")]
        )
        .is_err());
    assert_eq!(read(&f).revision, "0");
    let mut edit = f.scope.clone();
    edit.edit_message_id = Some(source.id);
    assert!(f
        .engine
        .save_media_draft(
            edit.clone(),
            "0".into(),
            attachments(&["replacement"], None),
            vec![input("replacement")]
        )
        .is_err());
    let draft = f
        .engine
        .save_media_draft(edit.clone(), "0".into(), json, vec![input("replacement")])?
        .draft;
    let mut other = edit;
    other.edit_message_id = Some("other-message".into());
    assert!(f
        .engine
        .media_draft_asset(other, draft.revision, "replacement".into())
        .is_err());
    assert_eq!(read(&f).revision, "0");
    Ok(())
}
