use super::*;
use crate::{QuoteSpan, ReplySelection};

fn selected(
    f: &Fixture,
) -> Result<(String, TimelineEntry, ReplySelection), Box<dyn std::error::Error>> {
    let space = f
        .engine
        .create_group_with("Private source".into(), vec![], PrivacyDto::Closed)?;
    let text = "😀 exact quotation ".repeat(12);
    let source = f.engine.send_message(space.clone(), text.clone())?;
    let selection = ReplySelection {
        source_id: source.id.clone(),
        revision: source.text_revision.clone().ok_or("text revision")?,
        spans: vec![QuoteSpan {
            start_utf16: 0,
            end_utf16: 227,
        }],
        text: text.trim_end().into(),
    };
    Ok((space, source, selection))
}

#[test]
fn shared_selection_recovers_and_sends_once_without_private_history(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("shared-selected.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let (space, source, selection) = selected(&f)?;
    let quote =
        f.engine
            .lock()
            .review_shared_reply(&f.scope, &space, &source.id, Some(&selection))?;
    let before = counts(&f.engine);
    let saved = f.engine.lock().save_message_draft_content(
        &f.scope,
        "0",
        DraftContent {
            reply_intent: None,
            edit: None,
            text: "Answer".into(),
            reply_to: None,
            reply_quote: None,
            shared_reply: Some(quote),
        },
    )?;
    assert_eq!(counts(&f.engine), before);
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
    let mut engine = client.lock();
    let event = engine
        .store
        .outbox_get(&visible_acceptance(&sent).id)?
        .ok_or("event")?;
    engine.state.spaces.remove(&space);
    assert!(crate::selected_quotes::received_valid(
        &engine.state,
        &event,
        &[]
    ));
    let face = engine
        .timeline(&scope.space_id)?
        .into_iter()
        .find(|e| e.id == visible_acceptance(&sent).id)
        .and_then(|e| e.reply_to)
        .ok_or("quote")?;
    assert_eq!(face.selection, Some(selection));
    assert!(!face.origin.ok_or("origin")?.available);
    for case in ["source", "revision", "text", "width", "overlap", "oversize"] {
        let mut bad = event.clone();
        let types::EventBody::MessagePosted {
            reply: Some(reply), ..
        } = &mut bad.body
        else {
            return Err("reply".into());
        };
        let shared = reply.shared.as_mut().ok_or("shared")?;
        let quote = shared.selection.as_mut().ok_or("selection")?;
        match case {
            "source" => quote.source_id = "different-source".into(),
            "revision" => quote.revision = "different-revision".into(),
            "text" => quote.text = "substitution".into(),
            "width" => quote.spans[0].end_utf16 += 1,
            "overlap" => quote.spans.push(quote.spans[0].clone()),
            _ => {
                quote.text = "a".repeat(4097);
                shared.text.clone_from(&quote.text);
                quote.spans[0].end_utf16 = 4097;
            }
        }
        assert!(
            !crate::selected_quotes::received_valid(&engine.state, &bad, &[]),
            "{case}"
        );
    }
    drop(engine);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[tokio::test]
async fn shared_selected_command_rechecks_exact_ranges_revision_and_sender_access(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let (space, source, selection) = selected(&f)?;
    let command = serde_json::json!({"op":"review_shared_reply", "expected_identity_id":f.scope.expected_identity_id,
        "expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,
        "source_space_id":space,"message_id":source.id,"expected_source_revision":source.text_revision, "selection":selection});
    let quote: crate::replies::external::SharedReplyDraft =
        serde_json::from_str(&f.engine.client_request(command.to_string()).await?)?;
    assert_eq!(quote.selection.as_ref(), Some(&selection));
    let retained = save(&f, "0", "Keep this draft", None);
    for case in ["surrogate", "text", "revision", "source"] {
        let mut bad = command.clone();
        match case {
            "surrogate" => bad["selection"]["spans"][0]["start_utf16"] = 1.into(),
            "text" => bad["selection"]["text"] = "different".into(),
            "revision" => bad["selection"]["revision"] = "old".into(),
            _ => bad["selection"]["source_id"] = "other".into(),
        }
        assert!(
            f.engine.client_request(bad.to_string()).await.is_err(),
            "{case}"
        );
        assert_eq!(read(&f), retained);
    }
    let saved = f.engine.lock().save_message_draft_content(
        &f.scope,
        &retained.revision,
        DraftContent {
            reply_intent: None,
            edit: None,
            text: retained.text,
            reply_to: None,
            reply_quote: None,
            shared_reply: Some(quote),
        },
    )?;
    f.engine
        .edit_message(space.clone(), source.id, "Changed".into())?;
    let before = counts(&f.engine);
    assert!(f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)
        .is_err());
    assert_eq!(read(&f), saved);
    assert_eq!(counts(&f.engine), before);
    f.engine
        .lock()
        .state
        .spaces
        .get_mut(&space)
        .ok_or("source")?
        .members
        .clear();
    assert!(f.engine.client_request(command.to_string()).await.is_err());
    Ok(())
}
