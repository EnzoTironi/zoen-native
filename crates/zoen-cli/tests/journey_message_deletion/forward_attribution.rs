//! Actual reviewed JSON forwarding crosses independent enrolled MLS clients.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_forward_carries_original_display_to_a_peer_and_survives_cold_replay() -> TestResult
{
    let (w, space) = enrolled("forward_attribution_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let source = alice.0.send_message(
        space.clone(),
        "Keep original forwarding content 雨 😀".into(),
    )?;
    let rows = received(&bob.0, &space, |rows| {
        rows.iter().any(|e| e.id == source.id)
    })
    .await?;
    let source = find(&rows, &source.id)?;
    let owner = bob.0.account().ok_or("Bob")?;
    assert_ne!(source.author.id, owner.identity_id);
    let scope = serde_json::json!({
        "expected_identity_id": owner.identity_id, "expected_device_id": owner.device_id,
        "source_space": space, "message_id": source.id, "target_space": space,
    });
    let review: serde_json::Value = serde_json::from_str(
        &bob.0
            .client_request(
                serde_json::json!({ "op": "forward_review", "scope": scope }).to_string(),
            )
            .await?,
    )?;
    assert_eq!(review["message"]["author"]["name"], source.author.name);
    let command = serde_json::json!({
        "op": "confirm_forward", "scope": scope,
        "operation_id": review["operation_id"], "source_revision": review["source_revision"],
    });
    let accepted: serde_json::Value =
        serde_json::from_str(&bob.0.client_request(command.to_string()).await?)?;
    let id = accepted["accepted_message_id"]
        .as_str()
        .ok_or("accepted id")?
        .to_owned();
    assert_eq!(accepted["accepted"]["forwarded_author"], source.author.name);
    bob.0.wait_until_settled(20_000).await?;
    let rows = received(&alice.0, &space, |rows| rows.iter().any(|e| e.id == id)).await?;
    let delivered = find(&rows, &id)?;
    assert_ne!(delivered.author.id, source.author.id);
    assert_eq!(
        delivered.forwarded_author.as_deref(),
        Some(source.author.name.as_str())
    );
    assert_eq!(text(&delivered), text(&source));
    assert!(delivered.forwarded);
    assert!(delivered.reply_to.is_none());
    let before = bob.0.log_events(space.clone())?;
    drop(bob);
    let bob = Open::offline(&w, "bob")?;
    let recovered: serde_json::Value =
        serde_json::from_str(&bob.0.client_request(command.to_string()).await?)?;
    assert_eq!(recovered["accepted_message_id"], id);
    assert_eq!(
        recovered["accepted"]["forwarded_author"],
        source.author.name
    );
    assert_eq!(bob.0.log_events(space.clone())?, before);
    let twice = bob.0.forward_message(space.clone(), id, space.clone())?;
    assert_eq!(twice.forwarded_author, delivered.forwarded_author);
    bob.0.start_sync(None)?;
    bob.0.wait_until_settled(20_000).await?;
    let rows = received(&alice.0, &space, |rows| {
        rows.iter().any(|e| e.id == twice.id)
    })
    .await?;
    assert_eq!(
        find(&rows, &twice.id)?.forwarded_author,
        delivered.forwarded_author
    );
    assert_eq!(bob.0.log_events(space)?.len(), before.len() + 1);
    Ok(())
}
