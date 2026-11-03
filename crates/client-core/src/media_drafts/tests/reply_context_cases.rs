//! The same reviewed source survives private media custody and signs the real send.
use super::*;
use crate::{QuoteSpan, ReplySelection};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[tokio::test]
async fn media_receipt_names_the_exact_private_reply_intent_and_never_replaces_later_typing(
) -> TestResult {
    let f = fixture(":memory:");
    let (source_space, source, json) = content(&f, false, true)?;
    let mut value: serde_json::Value = serde_json::from_str(&json)?;
    value["reply_intent"] = "selection_A".into();
    let normal_command = serde_json::json!({
        "op":"save_message_draft", "expected_identity_id":f.scope.expected_identity_id,
        "expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,
        "thread_root_id":null,"expected_revision":"0","text":"Earlier text",
        "reply_to":null,"shared_reply":value["shared_reply"],"reply_intent":"selection_A",
    });
    let original: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(normal_command.to_string()).await?)?;
    assert_eq!(original["reply_intent"], "selection_A");
    let saved = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            value.to_string(),
            vec![input("a")],
        )?
        .draft;
    let mut later = normal_command.clone();
    later["expected_revision"] = original["revision"].clone();
    later["text"] = "Later private text".into();
    later["reply_intent"] = "selection_B".into();
    let later: serde_json::Value =
        serde_json::from_str(&f.engine.client_request(later.to_string()).await?)?;
    let sent = f.engine.send_media_draft(f.scope.clone(), saved.revision)?;
    let receipt: serde_json::Value = serde_json::from_str(
        sent.draft
            .accepted_intent_json
            .as_deref()
            .ok_or("receipt")?,
    )?;
    assert_eq!(receipt["reply"]["reply_intent"], "selection_A");
    assert_eq!(receipt["messages"][0], sent.entries[0].id);
    let current = f.engine.lock().message_draft(&f.scope.normal())?;
    assert_eq!(current.text, "Later private text");
    assert_eq!(current.reply_intent.as_deref(), Some("selection_B"));
    let event = f
        .engine
        .lock()
        .store
        .outbox_get(&sent.entries[0].id)?
        .ok_or("event")?;
    let wire = serde_json::to_string(&event)?;
    assert!(!wire.contains("selection_A") && !wire.contains("selection_B"));
    f.engine
        .edit_message(source_space, source.id, "Changed source".into())?;
    let mut fresh = normal_command.clone();
    fresh["expected_revision"] = later["revision"].clone();
    fresh["reply_intent"] = "selection_C".into();
    assert!(
        f.engine.client_request(fresh.to_string()).await.is_err(),
        "new explicit selection must reauthorize"
    );
    let mut typing = fresh;
    typing["reply_intent"] = "selection_B".into();
    typing["text"] = "Keep typing against the retained context".into();
    assert!(
        f.engine.client_request(typing.to_string()).await.is_ok(),
        "existing private work survives source changes"
    );
    Ok(())
}

#[test]
fn hidden_accepted_media_recovers_its_receipt_cold_without_restoring_history_or_overwriting_new_work(
) -> TestResult {
    let dir = temp()?;
    let path = dir.join("hidden-media-receipt.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let (_, _, json) = content(&f, false, true)?;
    let saved = f
        .engine
        .save_media_draft(f.scope.clone(), "0".into(), json, vec![input("a")])?
        .draft;
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), saved.revision.clone())?;
    let accepted = &sent.entries[0];
    f.engine.delete_messages(crate::MessageDeletion {
        expected_identity_id: f.scope.expected_identity_id.clone(),
        expected_device_id: f.scope.expected_device_id.clone(),
        space_id: f.scope.space_id.clone(),
        for_me: Some(true),
        targets: vec![crate::MessageDeletionTarget {
            message_id: accepted.id.clone(),
            expected_revision: accepted
                .text_revision
                .clone()
                .unwrap_or_else(|| accepted.id.clone()),
        }],
    })?;
    let newer = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            sent.draft.revision,
            attachments(&["b"], None),
            vec![input("b")],
        )?
        .draft;
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(name.into(), "en".into())?;
    assert!(engine.unlock(vault)?);
    let before = counts(&engine);
    let recovered = engine.send_media_draft(scope.clone(), saved.revision.clone())?;
    assert!(recovered.entries.is_empty());
    assert_eq!(recovered.draft, newer);
    let receipt: serde_json::Value = serde_json::from_str(
        recovered
            .draft
            .accepted_intent_json
            .as_deref()
            .ok_or("receipt")?,
    )?;
    assert_eq!(receipt["input_revision"], saved.revision);
    assert_eq!(receipt["messages"][0], accepted.id);
    assert_eq!(counts(&engine), before);
    assert!(!engine
        .timeline(scope.space_id)?
        .iter()
        .any(|entry| entry.id == accepted.id));
    drop(engine);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn content(
    f: &Fixture,
    voice_note: bool,
    shared: bool,
) -> TestResult<(String, TimelineEntry, String)> {
    let source_space = if shared {
        f.engine
            .create_group_with("Private quote source".into(), vec![], PrivacyDto::Closed)?
    } else {
        f.scope.space_id.clone()
    };
    let source = f
        .engine
        .send_message(source_space.clone(), "Alpha 😀 Beta".into())?;
    let selected = ReplySelection {
        source_id: source.id.clone(),
        revision: source.text_revision.clone().ok_or("source revision")?,
        spans: vec![QuoteSpan {
            start_utf16: 6,
            end_utf16: 8,
        }],
        text: "😀".into(),
    };
    let mut value: serde_json::Value = serde_json::from_str(&if voice_note {
        voice(true)
    } else {
        attachments(&["a"], None)
    })?;
    if shared {
        let quote = f.engine.lock().review_shared_reply(
            &f.scope.normal(),
            &source_space,
            &source.id,
            Some(&selected),
        )?;
        value["shared_reply"] = serde_json::to_value(quote)?;
    } else {
        value["reply_to"] = source.id.clone().into();
        value["reply_quote"] = serde_json::to_value(selected)?;
    }
    Ok((source_space, source, value.to_string()))
}

#[test]
fn selected_local_and_cross_chat_quotes_sign_files_and_voice_once() -> TestResult {
    for (voice_note, shared) in [(false, false), (false, true), (true, false), (true, true)] {
        let f = fixture(":memory:");
        let (source_space, source, content) = content(&f, voice_note, shared)?;
        let before = counts(&f.engine);
        let assets = if voice_note {
            vec![audio("original"), audio("edited")]
        } else {
            vec![input("a")]
        };
        let saved = f
            .engine
            .save_media_draft(f.scope.clone(), "0".into(), content, assets)?;
        assert_eq!(
            counts(&f.engine),
            before,
            "reviewed context remains private"
        );
        let sent = f
            .engine
            .send_media_draft(f.scope.clone(), saved.draft.revision.clone())?;
        assert_eq!(sent.entries.len(), 1);
        let entry = &sent.entries[0];
        let reply = entry.reply_to.as_ref().ok_or("reply quote")?;
        assert_eq!(reply.id, source.id);
        assert_eq!(reply.text, "😀");
        assert_eq!(reply.selection.as_ref().ok_or("selected quote")?.text, "😀");
        assert_eq!(
            reply.origin.as_ref().map(|origin| origin.space_id.as_str()),
            shared.then_some(source_space.as_str())
        );
        if voice_note {
            let voice = entry.voice.as_ref().ok_or("voice")?;
            assert_eq!(voice.transcript, "spoken result");
            assert_eq!(
                f.engine.file_bytes(voice.item_id.clone(), Some(1))?,
                Some(audio("edited").bytes)
            );
        } else {
            assert!(
                matches!(&entry.kind, crate::EntryKind::Message {text, ..} if text == "Private caption a")
            );
        }
        let before = counts(&f.engine);
        assert_eq!(
            f.engine
                .send_media_draft(f.scope.clone(), saved.draft.revision)?
                .entries,
            sent.entries
        );
        assert_eq!(
            counts(&f.engine),
            before,
            "accepted retry never republishes"
        );
    }
    Ok(())
}

#[test]
fn cold_media_quote_is_private_and_changed_source_refuses_send_without_losing_private_work(
) -> TestResult {
    let dir = temp()?;
    let path = dir.join("quoted-media.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let (source_space, source, json) = content(&f, false, true)?;
    let saved = f
        .engine
        .save_media_draft(f.scope.clone(), "0".into(), json.clone(), vec![input("a")])?
        .draft;
    for (_, sealed) in rows(&f) {
        assert!(!sealed.contains("Alpha"));
        assert!(!sealed.contains(&source.id));
        assert!(!sealed.contains("Private caption"));
    }
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(name.into(), "en".into())?;
    assert!(engine.unlock(vault)?);
    assert_eq!(engine.media_draft(scope.clone())?, saved);
    engine.edit_message(source_space, source.id, "Different source".into())?;
    let before = counts(&engine);
    assert!(matches!(
        engine.send_media_draft(scope.clone(), saved.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&engine), before);
    assert_eq!(engine.media_draft(scope.clone())?, saved);
    let mut changed: serde_json::Value = serde_json::from_str(&json)?;
    changed["items"][0]["caption"] = "Keep my private edit".into();
    let edited = engine
        .save_media_draft(scope.clone(), saved.revision, changed.to_string(), vec![])?
        .draft;
    assert_eq!(
        counts(&engine),
        before,
        "private caption editing never reauthorizes Send"
    );
    assert!(edited.content_json.contains("Keep my private edit"));
    assert!(matches!(
        engine.send_media_draft(scope.clone(), edited.revision.clone()),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(engine.media_draft(scope)?, edited);
    drop(engine);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn new_or_replaced_quote_must_match_the_current_authorized_source_before_private_writes(
) -> TestResult {
    let f = fixture(":memory:");
    let (_, _, json) = content(&f, false, true)?;
    let original: serde_json::Value = serde_json::from_str(&json)?;
    for part in ["author_name", "revision", "space_id", "text", "selection"] {
        let mut forged = original.clone();
        if part == "selection" {
            // Half of a surrogate pair is not a Unicode selection.
            forged["shared_reply"]["selection"]["spans"][0]["end_utf16"] = 7.into();
        } else {
            forged["shared_reply"][part] = "Forged".into();
        }
        let before = counts(&f.engine);
        let private_before = rows(&f);
        assert!(
            f.engine
                .save_media_draft(
                    f.scope.clone(),
                    "0".into(),
                    forged.to_string(),
                    vec![input("a")]
                )
                .is_err(),
            "{part}"
        );
        assert_eq!(counts(&f.engine), before);
        assert_eq!(rows(&f), private_before);
    }
    Ok(())
}

#[test]
fn file_receipt_binds_the_exact_selected_quote_instead_of_reusing_a_different_reply() -> TestResult
{
    let f = fixture(":memory:");
    let (_, _, json) = content(&f, false, false)?;
    let saved = f
        .engine
        .save_media_draft(f.scope.clone(), "0".into(), json.clone(), vec![input("a")])?
        .draft;
    let accepted = f.engine.send_media_draft(f.scope.clone(), saved.revision)?;
    let mut other: serde_json::Value = serde_json::from_str(&json)?;
    other["reply_quote"]["text"] = "Beta".into();
    other["reply_quote"]["spans"][0] = serde_json::json!({"start_utf16":9,"end_utf16":13});
    let changed = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            accepted.draft.revision,
            other.to_string(),
            vec![input("a")],
        )?
        .draft;
    let before = counts(&f.engine);
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), changed.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    let retained = f.engine.media_draft(f.scope.clone())?;
    assert!(retained.attempted);
    assert_eq!(retained.revision, changed.revision);
    assert_eq!(retained.content_json, changed.content_json);
    assert_eq!(
        f.engine
            .timeline(f.scope.space_id.clone())?
            .iter()
            .filter(|row| row.id == accepted.entries[0].id)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn second_quoted_file_failure_rolls_back_the_whole_selection_and_retries_once() -> TestResult {
    let f = fixture(":memory:");
    let (_, source, json) = content(&f, false, true)?;
    let mut value: serde_json::Value = serde_json::from_str(&attachments(&["a", "b"], None))?;
    value["shared_reply"] =
        serde_json::from_str::<serde_json::Value>(&json)?["shared_reply"].clone();
    let saved = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            value.to_string(),
            vec![input("a"), input("b")],
        )?
        .draft;
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_quote_second BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' AND NEW.json LIKE '%Private caption b%' BEGIN SELECT RAISE(ABORT,'second failed'); END;")?;
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), saved.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(f.engine.media_draft(f.scope.clone())?.attempted);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_quote_second")?;
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), saved.revision.clone())?;
    assert_eq!(sent.entries.len(), 2);
    assert!(sent.entries.iter().all(|entry| entry
        .reply_to
        .as_ref()
        .is_some_and(|quote| quote.id == source.id && quote.text == "😀")));
    let before = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), saved.revision)?
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), before);
    Ok(())
}
