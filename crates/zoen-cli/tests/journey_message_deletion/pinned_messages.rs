//! The same JSON command as Expo crosses two actual MLS clients and cold/offline recovery.
use super::*;
use serde_json::{json, Value};
async fn request(client: &Client, value: Value) -> TestResult<Value> {
    Ok(serde_json::from_str(
        &client.client_request(value.to_string()).await?,
    )?)
}
fn scope(client: &Client, space: &str) -> TestResult<Value> {
    let account = client.account().ok_or("pin account")?;
    Ok(
        json!({"expected_identity_id":account.identity_id,"expected_device_id":account.device_id,"expected_relay_url":account.relay_url,"space_id":space}),
    )
}
async fn review(client: &Client, space: &str, message: &str) -> TestResult<Value> {
    request(
        client,
        json!({"op":"message_pin_review","scope":scope(client,space)?,"message_id":message}),
    )
    .await
}
fn input(review: &Value, id: &str, pinned: bool) -> Value {
    json!({"scope":review["scope"],"message_id":review["entry"]["id"],"change":{
        "operation_id":id,"expected_message_revision":review["entry"]["text_revision"],"expected_pin_revision":review["pin_revision"],"pinned":pinned,
    }})
}
async fn pins(client: &Client, space: &str) -> TestResult<Value> {
    request(
        client,
        json!({"op":"message_pins","scope":scope(client,space)?,"cursor":null,"limit":50}),
    )
    .await
}
async fn wait_pin(client: &Client, space: &str, message: &str, pinned: bool) -> TestResult {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let value = review(client, space, message).await?;
            if value["pinned"] == pinned && value["pending"] == false {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_pin_commands_reach_peer_survive_offline_cold_retry_and_keep_thread_navigation(
) -> TestResult {
    let (w, space) = enrolled("shared_message_pins").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let message = bob
        .0
        .send_message(space.clone(), "The address for Saturday".into())?;
    received(&alice.0, &space, |rows| {
        rows.iter().any(|e| e.id == message.id)
    })
    .await?;
    bob.0.wait_until_settled(20_000).await?;
    let before = review(&alice.0, &space, &message.id).await?;
    assert_eq!(
        before["can_pin"], true,
        "a DM writer may pin a received message"
    );
    let pin = input(&before, "shared-pin-recovery", true);
    let before_pin = w.events_in(&space).await.len();
    alice.0.stop_sync();
    request(&alice.0, json!({"op":"set_message_pin","input":pin})).await?;
    assert_eq!(
        review(&alice.0, &space, &message.id).await?["pending"],
        true
    );
    assert_eq!(pins(&bob.0, &space).await?["total"], 0);
    drop(alice);
    let alice = Open::offline(&w, "alice")?;
    assert_eq!(
        pins(&alice.0, &space).await?["entries"][0]["id"],
        message.id
    );
    let recovered = request(&alice.0, json!({"op":"set_message_pin","input":pin})).await?;
    alice.0.start_sync(None)?;
    alice.0.wait_until_settled(20_000).await?;
    wait_pin(&bob.0, &space, &message.id, true).await?;
    wait_pin(&alice.0, &space, &message.id, true).await?;
    let pin_events = w.events_in(&space).await;
    assert_eq!(
        pin_events.len(),
        before_pin + 1,
        "one pin annotation reaches the relay"
    );
    assert!(
        pin_events[before_pin].plain_event().is_none(),
        "the exact pin event is MLS encrypted"
    );
    assert_eq!(pins(&bob.0, &space).await?["entries"][0]["id"], message.id);
    assert_eq!(
        request(&alice.0, json!({"op":"set_message_pin","input":pin})).await?["applied_revision"],
        recovered["applied_revision"]
    );
    let count = w.events_in(&space).await.len();
    request(&alice.0, json!({"op":"set_message_pin","input":pin})).await?;
    alice.0.wait_until_settled(20_000).await?;
    assert_eq!(
        w.events_in(&space).await.len(),
        count,
        "exact recovery must not publish another annotation"
    );
    let stale = input(
        &review(&bob.0, &space, &message.id).await?,
        "stale-unpin",
        false,
    );
    bob.0.edit_message(
        space.clone(),
        message.id.clone(),
        "The corrected address for Saturday".into(),
    )?;
    received(&alice.0,&space,|rows| rows.iter().any(|e| e.id==message.id && matches!(&e.kind,EntryKind::Message{text,..} if text=="The corrected address for Saturday"))).await?;
    assert!(
        request(&bob.0, json!({"op":"set_message_pin","input":stale}))
            .await
            .is_err()
    );
    let unpin = input(
        &review(&bob.0, &space, &message.id).await?,
        "peer-unpin",
        false,
    );
    request(&bob.0, json!({"op":"set_message_pin","input":unpin})).await?;
    bob.0.wait_until_settled(20_000).await?;
    wait_pin(&alice.0, &space, &message.id, false).await?;
    assert_eq!(pins(&alice.0, &space).await?["total"], 0);
    let reply = alice.0.send_reply(
        space.clone(),
        "A pinned thread detail".into(),
        message.id.clone(),
        true,
    )?;
    alice.0.wait_until_settled(20_000).await?;
    received(&bob.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == message.id && e.thread_replies == 1)
    })
    .await?;
    let thread_review = review(&bob.0, &space, &reply.id).await?;
    assert_eq!(thread_review["entry"]["in_thread"], message.id);
    request(
        &bob.0,
        json!({"op":"set_message_pin","input":input(&thread_review,"pin-thread-reply",true)}),
    )
    .await?;
    bob.0.wait_until_settled(20_000).await?;
    wait_pin(&alice.0, &space, &reply.id, true).await?;
    let pinned = pins(&alice.0, &space).await?;
    assert_eq!(pinned["entries"][0]["id"], reply.id);
    assert_eq!(
        pinned["entries"][0]["in_thread"], message.id,
        "the canonical destination remains the original thread"
    );
    alice.0.stop_sync();
    let queued_unpin = input(
        &review(&alice.0, &space, &reply.id).await?,
        "queued-thread-unpin",
        false,
    );
    request(
        &alice.0,
        json!({"op":"set_message_pin","input":queued_unpin}),
    )
    .await?;
    let queued_pin = input(
        &review(&alice.0, &space, &reply.id).await?,
        "queued-thread-repin",
        true,
    );
    request(&alice.0, json!({"op":"set_message_pin","input":queued_pin})).await?;
    assert_eq!(review(&alice.0, &space, &reply.id).await?["pending"], true);
    alice.0.start_sync(None)?;
    alice.0.wait_until_settled(20_000).await?;
    wait_pin(&alice.0, &space, &reply.id, true).await?;
    wait_pin(&bob.0, &space, &reply.id, true).await?;
    let shared_count = w.events_in(&space).await.len();
    alice.0.delete_messages(MessageDeletion {
        expected_identity_id: alice.0.account().ok_or("Alice")?.identity_id,
        expected_device_id: alice.0.account().ok_or("Alice")?.device_id,
        space_id: space.clone(),
        targets: vec![MessageDeletionTarget {
            message_id: reply.id.clone(),
            expected_revision: reply.text_revision.clone().ok_or("reply revision")?,
        }],
        for_me: Some(true),
    })?;
    assert_eq!(pins(&alice.0, &space).await?["total"], 0);
    assert_eq!(
        pins(&bob.0, &space).await?["total"],
        1,
        "private hiding never unpins another participant's shared view"
    );
    assert_eq!(w.events_in(&space).await.len(), shared_count);
    for event in w.events_in(&space).await {
        assert!(
            !matches!(
                event.env.body(),
                Some(types::EventBody::MessageAnnotated {
                    note: types::Annotation::Pin { .. },
                    ..
                })
            ),
            "no shared pin metadata is clear relay content"
        );
    }
    Ok(())
}
