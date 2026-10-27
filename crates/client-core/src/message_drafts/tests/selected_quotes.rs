use super::*;
use crate::{QuoteSpan, ReplySelection};

fn selection(entry: &TimelineEntry) -> Result<ReplySelection, Box<dyn std::error::Error>> {
    Ok(ReplySelection {
        source_id: entry.id.clone(),
        revision: entry.text_revision.clone().ok_or("missing text revision")?,
        spans: vec![
            QuoteSpan {
                start_utf16: 3,
                end_utf16: 7,
            },
            QuoteSpan {
                start_utf16: 9,
                end_utf16: 12,
            },
            QuoteSpan {
                start_utf16: 14,
                end_utf16: 20,
            },
        ],
        text: "one two three".into(),
    })
}

#[test]
fn selected_draft_cold_recovery_exact_once_and_post_send_edit_delete_snapshot(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("selected.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "😀 one **two** three".into())?;
    let quote = selection(&source)?;
    let saved = f.engine.lock().save_message_draft_selected(
        &f.scope,
        "0",
        "Answer".into(),
        Some(source.id.clone()),
        Some(quote.clone()),
    )?;
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(name.into(), "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(client.lock().message_draft(&scope)?, saved);
    let sent = client.lock().send_message_draft(&scope, &saved.revision)?;
    let before = counts(&client);
    assert_eq!(
        client.lock().send_message_draft(&scope, &saved.revision)?,
        sent
    );
    assert_eq!(counts(&client), before);
    assert_eq!(
        sent.entry
            .reply_to
            .as_ref()
            .ok_or("quote")?
            .selection
            .as_ref(),
        Some(&quote)
    );
    client.edit_message(scope.space_id.clone(), source.id.clone(), "Later".into())?;
    client.delete_message(scope.space_id.clone(), source.id.clone())?;
    let timeline = client.timeline(scope.space_id.clone())?;
    let received = timeline
        .iter()
        .find(|e| e.id == sent.entry.id)
        .ok_or("reply missing")?;
    assert_eq!(
        received.reply_to.as_ref().ok_or("snapshot")?.text,
        "one two three"
    );
    assert!(client.lock().message_draft(&scope)?.reply_quote.is_none());
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn invalid_or_stale_selection_never_overwrites_draft_or_sends(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "😀 one **two** three".into())?;
    let good = selection(&source)?;
    let retained = save(&f, "0", "Retain", None);
    for case in ["surrogate", "revision", "source", "omitted", "layout"] {
        let mut q = good.clone();
        match case {
            "surrogate" => {
                q.spans[0] = QuoteSpan {
                    start_utf16: 1,
                    end_utf16: 2,
                }
            }
            "revision" => q.revision = "foreign-revision".into(),
            "source" => q.source_id = "other-source".into(),
            "omitted" => {
                q.spans.remove(1);
                q.text = "one  three".into();
            }
            "layout" => q.text = "one two\nthree".into(),
            _ => {}
        }
        let before = counts(&f.engine);
        assert!(
            f.engine
                .lock()
                .save_message_draft_selected(
                    &f.scope,
                    &retained.revision,
                    "Bad".into(),
                    Some(source.id.clone()),
                    Some(q)
                )
                .is_err(),
            "{case}"
        );
        assert_eq!(read(&f), retained);
        assert_eq!(counts(&f.engine), before);
    }
    let selected = f.engine.lock().save_message_draft_selected(
        &f.scope,
        &retained.revision,
        "Answer".into(),
        Some(source.id.clone()),
        Some(good),
    )?;
    f.engine.edit_message(
        f.scope.space_id.clone(),
        source.id.clone(),
        "😀 one **two** three".into(),
    )?;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &selected.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(read(&f), selected);
    assert_eq!(counts(&f.engine), before);
    f.engine
        .delete_message(f.scope.space_id.clone(), source.id)?;
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &selected.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(read(&f), selected);
    Ok(())
}

#[tokio::test]
async fn selected_quote_actual_commands_stage_only_then_explicit_send(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let source = f
        .engine
        .send_message(f.scope.space_id.clone(), "😀 one **two** three".into())?;
    let quote = selection(&source)?;
    let before = counts(&f.engine);
    let mut review = serde_json::json!({"op":"review_reply_selection","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"selection":quote});
    let reviewed: ReplySelection =
        serde_json::from_str(&f.engine.client_request(review.to_string()).await?)?;
    assert_eq!(reviewed, quote);
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f).revision, "0");
    review["selection"]["text"] = "omitted or transformed".into();
    assert!(f.engine.client_request(review.to_string()).await.is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f).revision, "0");
    let save = serde_json::json!({"op":"save_message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"expected_revision":"0","text":"Actual reply","reply_to":source.id,"reply_quote":quote});
    let saved: MessageDraftDto =
        serde_json::from_str(&f.engine.client_request(save.to_string()).await?)?;
    assert_eq!(saved.reply_quote, Some(quote.clone()));
    assert_eq!(counts(&f.engine), before);
    let mut wrong = f.scope.clone();
    wrong.expected_device_id = "other-session".into();
    assert!(f.engine.lock().message_draft(&wrong).is_err());
    let send = serde_json::json!({"op":"send_message_draft","expected_identity_id":f.scope.expected_identity_id,"expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,"expected_revision":saved.revision});
    let result: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(send.to_string()).await?)?;
    let face: crate::ReplyQuote = serde_json::from_value(result["entry"]["reply_to"].clone())?;
    assert_eq!(face.selection, Some(quote));
    let after = counts(&f.engine);
    let again: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(send.to_string()).await?)?;
    assert_eq!(again, result);
    assert_eq!(counts(&f.engine), after);
    Ok(())
}

#[test]
fn selected_thread_draft_preserves_child_and_rejects_foreign_root_scope(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let root = f
        .engine
        .send_message(f.scope.space_id.clone(), "Root".into())?;
    let child = f.engine.send_reply(
        f.scope.space_id.clone(),
        "😀 one **two** three".into(),
        root.id.clone(),
        true,
    )?;
    let quote = selection(&child)?;
    let other = f
        .engine
        .send_message(f.scope.space_id.clone(), "Other root".into())?;
    let mut wrong = f.scope.clone();
    wrong.thread_root_id = Some(other.id);
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .review_reply_selection(&wrong, quote.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(f.engine.lock().message_draft(&wrong)?.revision, "0");
    let mut scope = f.scope.clone();
    scope.thread_root_id = Some(root.id.clone());
    assert_eq!(
        f.engine
            .lock()
            .review_reply_selection(&scope, quote.clone())?,
        quote
    );
    let draft = f.engine.lock().save_message_draft_selected(
        &scope,
        "0",
        "Answer".into(),
        Some(child.id.clone()),
        Some(quote.clone()),
    )?;
    let sent = f
        .engine
        .lock()
        .send_message_draft(&scope, &draft.revision)?;
    assert_eq!(sent.entry.in_thread.as_deref(), Some(root.id.as_str()));
    let selected = sent.entry.reply_to.as_ref().ok_or("child quote")?;
    assert_eq!(selected.id, child.id);
    assert_eq!(selected.selection, Some(quote));
    Ok(())
}
