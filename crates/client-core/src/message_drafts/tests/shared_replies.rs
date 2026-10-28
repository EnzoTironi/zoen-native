use super::*;
use crate::replies::external::SharedReplyDraft;

fn source(f: &Fixture) -> Result<(String, TimelineEntry), CoreError> {
    let space = f
        .engine
        .create_group_with("Source".into(), vec![], PrivacyDto::Closed)?;
    let message = f
        .engine
        .send_message(space.clone(), "A quotation deliberately shared".into())?;
    Ok((space, message))
}
fn content(quote: SharedReplyDraft) -> DraftContent {
    DraftContent {
        text: "My answer".into(),
        reply_to: None,
        reply_quote: None,
        shared_reply: Some(quote),
    }
}
#[test]
fn shared_reply_survives_restart_and_lost_send_response_without_forwarding(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = temp()?;
    let path = dir.join("shared.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name);
    let (space, source) = source(&f)?;
    let before = counts(&f.engine);
    let quote = f
        .engine
        .lock()
        .review_shared_reply(&f.scope, &space, &source.id)?;
    let saved =
        f.engine
            .lock()
            .save_message_draft_content(&f.scope, "0", content(quote.clone()))?;
    assert_eq!(
        counts(&f.engine),
        before,
        "preparation cannot send or forward"
    );
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(name.into(), "en".into())?;
    assert!(client.unlock(vault.clone())?);
    assert_eq!(client.lock().message_draft(&scope)?, saved);
    let before_send = counts(&client);
    let sent = client.lock().send_message_draft(&scope, &saved.revision)?;
    let face = sent.entry.reply_to.as_ref().ok_or("quote")?;
    assert_eq!(face.text, quote.text);
    assert_eq!(face.id, source.id);
    assert!(face.selection.is_none(), "not a verified local selection");
    assert_eq!(face.origin.as_ref().ok_or("origin")?.space_id, space);
    assert!(face.origin.as_ref().ok_or("origin")?.available);
    assert!(!sent.entry.forwarded);
    assert_eq!(counts(&client), (before_send.0, before_send.1 + 1));
    drop(client);
    let client = Client::open(name.into(), "en".into())?;
    assert!(client.unlock(vault)?);
    let before = counts(&client);
    assert_eq!(
        client.lock().send_message_draft(&scope, &saved.revision)?,
        sent
    );
    assert_eq!(counts(&client), before);
    client.delete_message(space.clone(), source.id)?;
    let after = client.timeline(scope.space_id.clone())?;
    let persisted = after
        .iter()
        .find(|e| e.id == sent.entry.id)
        .ok_or("reply")?;
    let face = persisted.reply_to.as_ref().ok_or("snapshot")?;
    assert_eq!(face.text, quote.text);
    assert!(!face.origin.as_ref().ok_or("origin")?.available);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn shared_reply_refuses_substitution_stale_revision_and_wrong_device_without_losing_text(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let (space, source) = source(&f)?;
    let quote = f
        .engine
        .lock()
        .review_shared_reply(&f.scope, &space, &source.id)?;
    let existing = save(&f, "0", "Keep my draft", None);
    for field in [
        "text",
        "author_id",
        "author_name",
        "revision",
        "message_id",
        "space_id",
    ] {
        let mut value = serde_json::to_value(&quote)?;
        value[field] = "substituted".into();
        let forged = serde_json::from_value(value)?;
        assert!(
            f.engine
                .lock()
                .save_message_draft_content(&f.scope, &existing.revision, content(forged))
                .is_err(),
            "{field}"
        );
        assert_eq!(read(&f), existing);
    }
    let mut wrong = f.scope.clone();
    wrong.expected_device_id = "other-device".into();
    assert!(f
        .engine
        .lock()
        .review_shared_reply(&wrong, &space, &source.id)
        .is_err());
    assert!(f
        .engine
        .lock()
        .review_shared_reply(&f.scope, &f.scope.space_id, &source.id)
        .is_err());
    let saved =
        f.engine
            .lock()
            .save_message_draft_content(&f.scope, &existing.revision, content(quote))?;
    f.engine
        .edit_message(space, source.id, "Edited after selection".into())?;
    let before = counts(&f.engine);
    assert!(matches!(
        f.engine
            .lock()
            .send_message_draft(&f.scope, &saved.revision),
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(counts(&f.engine), before);
    assert_eq!(read(&f), saved);
    let cleared = save(&f, &saved.revision, &saved.text, None);
    assert_eq!(cleared.text, saved.text);
    assert!(cleared.shared_reply.is_none());
    Ok(())
}
#[tokio::test]
async fn shared_reply_actual_json_commands_keep_flat_compatibility_and_reject_unknown_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let (space, source) = source(&f)?;
    let mut command = serde_json::json!({"op":"review_shared_reply", "expected_identity_id":f.scope.expected_identity_id,
        "expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,
        "source_space_id":space,"message_id":source.id,"expected_source_revision":source.text_revision});
    let quote: SharedReplyDraft =
        serde_json::from_str(&f.engine.client_request(command.to_string()).await?)?;
    command["unexpected_authority"] = true.into();
    assert!(f.engine.client_request(command.to_string()).await.is_err());
    let mut save = serde_json::json!({"op":"save_message_draft", "expected_identity_id":f.scope.expected_identity_id,
        "expected_device_id":f.scope.expected_device_id,"space_id":f.scope.space_id,"thread_root_id":null,
        "expected_revision":"0","text":"Actual answer","reply_to":null,"shared_reply":quote});
    let saved: MessageDraftDto =
        serde_json::from_str(&f.engine.client_request(save.to_string()).await?)?;
    assert_eq!(saved.shared_reply, Some(quote));
    save["unexpected_authority"] = true.into();
    assert!(f.engine.client_request(save.to_string()).await.is_err());
    assert_eq!(read(&f), saved);
    Ok(())
}

#[test]
fn source_membership_and_target_write_permission_are_rechecked_at_send(
) -> Result<(), Box<dyn std::error::Error>> {
    for source_access in [true, false] {
        let f = fixture(":memory:");
        let (space, source) = source(&f)?;
        let quote = f
            .engine
            .lock()
            .review_shared_reply(&f.scope, &space, &source.id)?;
        let saved = f
            .engine
            .lock()
            .save_message_draft_content(&f.scope, "0", content(quote))?;
        let before = counts(&f.engine);
        {
            // Apply the same membership projection that authenticated removal/role
            // changes produce, without involving a relay in this policy boundary test.
            let mut e = f.engine.lock();
            if source_access {
                e.state
                    .spaces
                    .get_mut(&space)
                    .ok_or("source")?
                    .members
                    .clear();
            } else {
                let target = e.state.spaces.get_mut(&f.scope.space_id).ok_or("target")?;
                for (_, role) in &mut target.members {
                    *role = types::Role::Reader;
                }
            }
            assert!(matches!(
                e.send_message_draft(&f.scope, &saved.revision),
                Err(CoreError::DraftRejected { .. })
            ));
            assert_eq!(e.message_draft(&f.scope)?, saved);
        }
        assert_eq!(counts(&f.engine), before);
    }
    Ok(())
}
#[test]
fn receiving_shared_quote_requires_no_source_history_and_cannot_claim_local_quote_or_thread(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let (space, source) = source(&f)?;
    let quote = f
        .engine
        .lock()
        .review_shared_reply(&f.scope, &space, &source.id)?;
    let saved =
        f.engine
            .lock()
            .save_message_draft_content(&f.scope, "0", content(quote.clone()))?;
    let sent = f
        .engine
        .lock()
        .send_message_draft(&f.scope, &saved.revision)?;
    let mut e = f.engine.lock();
    let event = e.store.outbox_get(&sent.entry.id)?.ok_or("sent event")?;
    e.state.spaces.remove(&space);
    assert!(crate::selected_quotes::received_valid(
        &e.state,
        &event,
        &[]
    ));
    let entry = e
        .timeline(&f.scope.space_id)?
        .into_iter()
        .find(|x| x.id == sent.entry.id)
        .ok_or("entry")?;
    let face = entry.reply_to.ok_or("quote")?;
    assert_eq!(face.text, quote.text);
    assert!(!face.origin.ok_or("origin")?.available);
    for field in ["thread", "quote", "oversize", "same_space"] {
        let mut bad = event.clone();
        let types::EventBody::MessagePosted {
            reply: Some(reply), ..
        } = &mut bad.body
        else {
            return Err("reply".into());
        };
        match field {
            "thread" => reply.thread = true,
            "quote" => {
                reply.quote = Some(Box::new(types::SelectedQuote {
                    source_id: source.id.clone(),
                    revision: quote.revision.clone(),
                    spans: vec![],
                    text: quote.text.clone(),
                }))
            }
            "oversize" => reply.shared.as_mut().ok_or("shared")?.text = "x".repeat(142),
            _ => reply
                .shared
                .as_mut()
                .ok_or("shared")?
                .space_id
                .clone_from(&f.scope.space_id),
        }
        assert!(
            !crate::selected_quotes::received_valid(&e.state, &bad, &[]),
            "{field}"
        );
    }
    Ok(())
}
