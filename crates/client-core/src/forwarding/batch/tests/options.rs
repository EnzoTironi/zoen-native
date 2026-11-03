use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn partial_batch_options_preserve_accepted_receipts_and_require_explicit_rereview(
) -> TestResult {
    let (client, _, scope) = fixture(":memory:");
    let review = client.forward_batch_review(scope.clone())?;
    let before = count(&client);
    client.lock().store.conn().execute_batch(&format!("CREATE TRIGGER fail_options_second BEFORE UPDATE ON meta WHEN NEW.key LIKE 'message.forwardbatch.%' AND (SELECT COUNT(*) FROM outbox) > {} BEGIN SELECT RAISE(ABORT,'receipt failure'); END;",before+1))?;
    let partial = confirm(&client, &scope, &review)?;
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_options_second")?;
    assert!(!partial.ready);
    assert!(partial.messages[0].accepted_message_id.is_some());
    assert!(partial.messages[1].accepted_message_id.is_none());
    let command = serde_json::json!({"op":"set_forward_batch_options","source":scope.source,"batch_id":partial.batch_id,"review_revision":partial.review_revision,"options":{"hide_author":true,"hide_caption":true}});
    let response: serde_json::Value =
        serde_json::from_str(&client.client_request(command.to_string()).await?)?;
    // The native application boundary intentionally emits u64/i64 as decimal
    // strings. Inspect its actual JSON, then compare the canonical typed status.
    assert!(response["messages"][0]["message"]["seq"].is_string());
    assert_eq!(response["messages"][1]["options"]["hide_author"], true);
    let changed = client
        .forward_batch_status(scope.source.clone())?
        .ok_or("retained batch")?;
    assert_eq!(response["review_revision"], changed.review_revision);
    assert!(
        !changed.ready,
        "option changes cannot rearm interrupted publication"
    );
    assert_ne!(changed.review_revision, partial.review_revision);
    assert_eq!(changed.messages[0], partial.messages[0]);
    assert_eq!(
        changed.messages[1].options,
        ForwardOptions {
            hide_author: true,
            hide_caption: true
        }
    );
    assert_eq!(count(&client), before + 1);
    assert!(confirm(&client, &scope, &partial).is_err());
    assert!(confirm(&client, &scope, &changed).is_err());
    let renewed = client.forward_batch_review(scope.clone())?;
    assert_eq!(renewed.messages[0], partial.messages[0]);
    assert_eq!(renewed.messages[1].options, changed.messages[1].options);
    let accepted = confirm(&client, &scope, &renewed)?;
    assert_eq!(
        accepted.messages[0].accepted_message_id,
        partial.messages[0].accepted_message_id
    );
    assert!(
        accepted.messages[0]
            .accepted
            .as_ref()
            .ok_or("first")?
            .forwarded
    );
    let second = accepted.messages[1].accepted.as_ref().ok_or("second")?;
    assert!(!second.forwarded);
    assert!(second.forwarded_author.is_none());
    assert!(
        matches!(&second.kind,crate::EntryKind::Message{text,..} if text=="Second private text")
    );
    assert_eq!(count(&client), before + 2);
    assert_eq!(confirm(&client, &scope, &renewed)?, accepted);
    assert!(client
        .lock()
        .set_forward_batch_options(
            &scope.source,
            &accepted.batch_id,
            &accepted.review_revision,
            ForwardOptions::default()
        )
        .is_err());
    assert_eq!(count(&client), before + 2);
    Ok(())
}

#[test]
fn pending_batch_options_survive_cold_recovery_and_fence_wrong_scopes() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("batch-options")?);
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join("account.sqlite").to_string_lossy().into_owned();
    let (client, vault, scope) = fixture(&path);
    let review = client.forward_batch_review(scope.clone())?;
    let before = count(&client);
    let changed = client.lock().set_forward_batch_options(
        &scope.source,
        &review.batch_id,
        &review.review_revision,
        ForwardOptions {
            hide_author: true,
            hide_caption: false,
        },
    )?;
    assert!(changed.ready);
    assert_ne!(changed.review_revision, review.review_revision);
    for part in ["owner", "device", "source"] {
        let mut source = scope.source.clone();
        match part {
            "owner" => source.expected_identity_id = "foreign".into(),
            "device" => source.expected_device_id = "foreign".into(),
            _ => source.message_ids.reverse(),
        };
        assert!(client
            .lock()
            .set_forward_batch_options(
                &source,
                &changed.batch_id,
                &changed.review_revision,
                ForwardOptions::default()
            )
            .is_err());
        assert_eq!(
            client.forward_batch_status(scope.source.clone())?,
            Some(changed.clone())
        );
    }
    drop(client);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(
        client.forward_batch_status(scope.source.clone())?,
        Some(changed.clone())
    );
    assert_eq!(count(&client), before);
    assert!(confirm(&client, &scope, &review).is_err());
    let accepted = confirm(&client, &scope, &changed)?;
    assert!(accepted.messages.iter().all(|r| r
        .accepted
        .as_ref()
        .is_some_and(|e| !e.forwarded && e.forwarded_author.is_none())));
    assert_eq!(count(&client), before + 2);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
