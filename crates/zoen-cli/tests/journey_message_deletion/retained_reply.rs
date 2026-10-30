//! The real composer contract retains typing after a peer deletes a quoted source.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_deletion_preserves_private_typing_and_only_explicit_cancellation_allows_send(
) -> TestResult {
    let (w, space) = enrolled("retained_reply_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let source = bob
        .0
        .send_message(space.clone(), "Original quotation".into())?;
    let rows = received(&alice.0, &space, |rows| {
        rows.iter().any(|e| e.id == source.id)
    })
    .await?;
    let source = find(&rows, &source.id)?;
    let account = alice.0.account().ok_or("Alice account")?;
    let request = |op: &str| {
        serde_json::json!({
            "op": op, "expected_identity_id": account.identity_id, "expected_device_id": account.device_id,
            "space_id": space, "thread_root_id": null,
        })
    };
    let initial: serde_json::Value = serde_json::from_str(
        &alice
            .0
            .client_request(request("message_draft").to_string())
            .await?,
    )?;
    let mut review = request("review_reply_selection");
    review["selection"] = serde_json::json!({
        "source_id": source.id, "revision": source.text_revision,
        "text": "Original", "spans": [{"start_utf16":0,"end_utf16":8}],
    });
    let selected: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(review.to_string()).await?)?;
    let mut save = request("save_message_draft");
    save["expected_revision"] = initial["revision"].clone();
    save["text"] = "Before peer deletion".into();
    save["reply_to"] = source.id.clone().into();
    save["reply_quote"] = selected.clone();
    let saved: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(save.to_string()).await?)?;
    bob.0.delete_message(space.clone(), source.id.clone())?;
    received(&alice.0, &space, |rows| {
        rows.iter().any(|e| e.id == source.id && e.deleted)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    let before = alice.0.log_events(space.clone())?;
    save["expected_revision"] = saved["revision"].clone();
    save["text"] = "Keep private typing after peer deletion 😀".into();
    let latest: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(save.to_string()).await?)?;
    assert_eq!(latest["reply_quote"], selected);
    let mut send = request("send_message_draft");
    send["expected_revision"] = latest["revision"].clone();
    assert!(matches!(
        alice.0.client_request(send.to_string()).await,
        Err(CoreError::DraftRejected { .. })
    ));
    assert_eq!(alice.0.log_events(space.clone())?, before);
    drop(alice);

    let alice = Open::offline(&w, "alice")?;
    let restored: serde_json::Value = serde_json::from_str(
        &alice
            .0
            .client_request(request("message_draft").to_string())
            .await?,
    )?;
    assert_eq!(restored, latest);
    assert!(matches!(
        alice.0.client_request(send.to_string()).await,
        Err(CoreError::DraftRejected { .. })
    ));
    save["expected_revision"] = latest["revision"].clone();
    save["reply_to"] = serde_json::Value::Null;
    save["reply_quote"] = serde_json::Value::Null;
    let cancelled: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(save.to_string()).await?)?;
    assert_eq!(cancelled["text"], latest["text"]);
    assert_eq!(alice.0.log_events(space.clone())?, before);
    send["expected_revision"] = cancelled["revision"].clone();
    let accepted: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(send.to_string()).await?)?;
    let id = accepted["message_id"]
        .as_str()
        .ok_or("accepted message identity")?;
    alice.0.start_sync(None)?;
    alice.0.wait_until_settled(20_000).await?;
    let received = received(&bob.0, &space, |rows| rows.iter().any(|e| e.id == id)).await?;
    let delivered = find(&received, id)?;
    assert_eq!(
        text(&delivered),
        "Keep private typing after peer deletion 😀"
    );
    assert!(delivered.reply_to.is_none());
    assert_eq!(alice.0.log_events(space.clone())?.len(), before.len() + 1);
    Ok(())
}
