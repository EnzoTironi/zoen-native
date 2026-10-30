//! Actual application commands, independent clients and a cold edit-only draft.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_correction_is_private_until_confirmed_and_peer_receives_exactly_one_edit(
) -> TestResult {
    let (w, space) = enrolled("cold_edit_draft_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let source = alice
        .0
        .send_message(space.clone(), "Original draft source".into())?;
    received(&bob.0, &space, |rows| {
        rows.iter().any(|e| e.id == source.id)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    let source = find(&alice.0.timeline(space.clone())?, &source.id)?;
    let account = alice.0.account().ok_or("Alice account")?;
    let request = |op: &str| {
        serde_json::json!({
            "op": op, "expected_identity_id": account.identity_id,
            "expected_device_id": account.device_id, "space_id": space,
            "thread_root_id": null,
        })
    };
    let initial: serde_json::Value = serde_json::from_str(
        &alice
            .0
            .client_request(request("message_draft").to_string())
            .await?,
    )?;
    let before = alice.0.log_events(space.clone())?;
    let mut save = request("save_message_draft");
    save["expected_revision"] = initial["revision"].clone();
    save["text"] = "Independent unsent normal message".into();
    save["reply_to"] = source.id.clone().into();
    save["edit"] = serde_json::json!({
        "message_id": source.id, "source_revision": source.text_revision,
        "base_text": "Original draft source", "text": "Cold recovered correction",
        "is_caption": false,
    });
    let saved: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(save.to_string()).await?)?;
    assert_eq!(
        alice.0.log_events(space.clone())?,
        before,
        "saving must remain private"
    );
    drop(alice);

    let alice = Open::offline(&w, "alice")?;
    let restored: serde_json::Value = serde_json::from_str(
        &alice
            .0
            .client_request(request("message_draft").to_string())
            .await?,
    )?;
    assert_eq!(restored, saved);
    assert_eq!(
        text(&find(&bob.0.timeline(space.clone())?, &source.id)?),
        "Original draft source"
    );
    let mut commit = request("send_message_edit_draft");
    commit["expected_revision"] = saved["revision"].clone();
    let accepted: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(commit.to_string()).await?)?;
    assert_eq!(
        accepted["draft"]["text"],
        "Independent unsent normal message"
    );
    assert_eq!(accepted["draft"]["reply_to"], source.id);
    assert!(accepted["draft"].get("edit").is_none());
    drop(alice);

    let alice = Open::online(&w, "alice").await?;
    received(&bob.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == source.id && text(e) == "Cold recovered correction")
    })
    .await?;
    let after = alice.0.log_events(space.clone())?;
    assert_eq!(after.len(), before.len() + 1);
    alice.0.client_request(commit.to_string()).await?;
    alice.0.wait_until_settled(20_000).await?;
    assert_eq!(alice.0.log_events(space.clone())?, after);
    let current: serde_json::Value = serde_json::from_str(
        &alice
            .0
            .client_request(request("message_draft").to_string())
            .await?,
    )?;
    assert_eq!(current, accepted["draft"]);
    // A lost acknowledgement may be retried after the sender hid the source.
    // The receiver still has the message; local presentation cannot revoke its receipt.
    let current_source = find(&alice.0.timeline(space.clone())?, &source.id)?;
    alice
        .0
        .delete_messages(deletion(&alice.0, &space, &[current_source], true)?)?;
    let before_retry = alice.0.log_events(space.clone())?;
    let hidden: serde_json::Value =
        serde_json::from_str(&alice.0.client_request(commit.to_string()).await?)?;
    assert_eq!(hidden["message_id"], source.id);
    assert_eq!(hidden["text_revision"], accepted["text_revision"]);
    assert!(hidden["entry"].is_null());
    assert_eq!(alice.0.log_events(space.clone())?, before_retry);
    assert_eq!(
        text(&find(&bob.0.timeline(space.clone())?, &source.id)?),
        "Cold recovered correction"
    );
    Ok(())
}
