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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_forward_options_only_publish_on_confirmation_and_preserve_peer_media() -> TestResult
{
    let (w, space) = enrolled("forward_options_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let bytes = b"Opaque bounded original media";
    let plain = alice
        .0
        .send_message(space.clone(), "Keep plain text 雨".into())?;
    let file = alice.0.send_file_message_with_caption(
        space.clone(),
        "report.pdf".into(),
        "application/pdf".into(),
        bytes.to_vec(),
        None,
        None,
        false,
        "forward-option-file".into(),
        "Hide only this caption".into(),
    )?;
    let voice = alice.0.send_voice_note(
        space.clone(),
        bytes.to_vec(),
        "audio/mp4".into(),
        2400,
        vec![0.2, 0.8],
        "Keep the transcription".into(),
        None,
        false,
    )?;
    let sources = [plain, file, voice];
    received(&bob.0, &space, |rows| {
        sources
            .iter()
            .all(|source| rows.iter().any(|row| row.id == source.id))
    })
    .await?;
    let owner = bob.0.account().ok_or("Bob")?;
    let mut accepted_ids = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        // Voice copying needs the canonical encrypted source bytes locally.
        if let Some(voice) = &source.voice {
            bob.0
                .ensure_file_version(voice.item_id.clone(), 1, 1024, 20_000)
                .await?;
        }
        let scope = serde_json::json!({
            "expected_identity_id": owner.identity_id, "expected_device_id": owner.device_id,
            "source_space": space, "message_id": source.id, "target_space": space,
        });
        let review: serde_json::Value = serde_json::from_str(
            &bob.0
                .client_request(
                    serde_json::json!({"op":"forward_review","scope":scope}).to_string(),
                )
                .await?,
        )?;
        let before = bob.0.log_events(space.clone())?;
        let changed: serde_json::Value = serde_json::from_str(&bob.0.client_request(
            serde_json::json!({
                "op":"set_forward_options","scope":scope,
                "operation_id":review["operation_id"],"source_revision":review["source_revision"],
                "options":{"hide_author":true,"hide_caption":true},
            }).to_string(),
        ).await?)?;
        assert_eq!(
            bob.0.log_events(space.clone())?,
            before,
            "options are private"
        );
        assert!(bob
            .0
            .client_request(
                serde_json::json!({
                    "op":"confirm_forward","scope":scope,"operation_id":review["operation_id"],
                    "source_revision":review["source_revision"],
                })
                .to_string()
            )
            .await
            .is_err());
        let command = serde_json::json!({
            "op":"confirm_forward","scope":scope,"operation_id":changed["operation_id"],
            "source_revision":changed["source_revision"],
        });
        let accepted: serde_json::Value =
            serde_json::from_str(&bob.0.client_request(command.to_string()).await?)?;
        let id = accepted["accepted_message_id"]
            .as_str()
            .ok_or("accepted")?
            .to_owned();
        let replay: serde_json::Value =
            serde_json::from_str(&bob.0.client_request(command.to_string()).await?)?;
        assert_eq!(
            accepted["accepted_message_id"],
            replay["accepted_message_id"]
        );
        assert_eq!(accepted["options"], replay["options"]);
        bob.0.wait_until_settled(20_000).await?;
        // A synced log counts relay-confirmed events. Voice uses one canonical
        // file event; publication is not observable here until sync settles.
        assert_eq!(bob.0.log_events(space.clone())?.len(), before.len() + 1);
        let rows = received(&alice.0, &space, |rows| rows.iter().any(|row| row.id == id)).await?;
        let delivered = find(&rows, &id)?;
        assert!(!delivered.forwarded);
        assert!(delivered.forwarded_author.is_none());
        assert_eq!(delivered.author.id, owner.identity_id);
        if index == 0 {
            assert_eq!(text(&delivered), text(source));
        }
        if index == 1 {
            assert!(text(&delivered).is_empty());
            let EntryKind::Message {
                card: Some(card), ..
            } = &delivered.kind
            else {
                return Err("file card".into());
            };
            alice
                .0
                .ensure_file_version(card.item_id.clone(), card.version, 1024, 20_000)
                .await?;
            assert_eq!(
                alice
                    .0
                    .file_bytes(card.item_id.clone(), Some(card.version))?,
                Some(bytes.to_vec())
            );
        }
        if index == 2 {
            let voice = delivered.voice.as_ref().ok_or("delivered voice")?;
            let original = source.voice.as_ref().ok_or("original voice")?;
            assert_ne!(voice.item_id, original.item_id);
            assert_eq!(voice.transcript, original.transcript);
            assert_eq!(voice.levels, original.levels);
            assert_eq!(voice.ms, original.ms);
            alice
                .0
                .ensure_file_version(voice.item_id.clone(), 1, 1024, 20_000)
                .await?;
            assert_eq!(
                alice.0.file_bytes(voice.item_id.clone(), Some(1))?,
                Some(bytes.to_vec())
            );
        }
        assert_eq!(rows.iter().filter(|row| row.id == id).count(), 1);
        accepted_ids.push(id);
    }
    let rows = alice.0.timeline(space)?;
    assert!(sources
        .iter()
        .all(|source| rows.iter().any(|row| row.id == source.id)));
    assert!(accepted_ids
        .iter()
        .all(|id| rows.iter().any(|row| &row.id == id)));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_batch_delivers_once_after_offline_confirmation_and_relay_restart() -> TestResult {
    let (mut world, space) = enrolled("forward_batch_relay_restart").await?;
    let alice = Open::online(&world, "alice")
        .await
        .map_err(|error| format!("Alice before relay stop: {error}"))?;
    let first = alice
        .0
        .send_message(space.clone(), "First batch source".into())?;
    let second = alice
        .0
        .send_message(space.clone(), "Second batch source".into())?;
    let bob = Open::online(&world, "bob")
        .await
        .map_err(|error| format!("Bob before relay stop: {error}"))?;
    received(&bob.0, &space, |rows| {
        rows.iter().any(|row| row.id == first.id) && rows.iter().any(|row| row.id == second.id)
    })
    .await?;
    let owner = bob.0.account().ok_or("Bob")?;
    drop(bob);
    drop(alice);
    world.stop_relay();

    let bob = Open::offline(&world, "bob")?;
    let source = serde_json::json!({
        "expected_identity_id": owner.identity_id,
        "expected_device_id": owner.device_id,
        "source_space": space,
        "message_ids": [first.id, second.id],
    });
    let review: serde_json::Value = serde_json::from_str(&bob.0.client_request(
        serde_json::json!({
            "op": "forward_batch_review", "scope": { "source": source, "target_space": space },
        }).to_string(),
    ).await?)?;
    let confirm = serde_json::json!({
        "op": "confirm_forward_batch", "source": source,
        "batch_id": review["batch_id"], "review_revision": review["review_revision"],
    })
    .to_string();
    let accepted: serde_json::Value =
        serde_json::from_str(&bob.0.client_request(confirm.clone()).await?)?;
    assert!(accepted["error"].is_null(), "{accepted}");
    let ids: Vec<_> = accepted["messages"]
        .as_array()
        .ok_or("accepted messages")?
        .iter()
        .map(|message| message["accepted_message_id"].as_str().ok_or("accepted id"))
        .collect::<Result<_, _>>()?;
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    drop(bob);

    let upstream = world.start_node();
    let proxy =
        final_event_proxy::FinalEventProxy::on_port(world.port, upstream, world.dir.clone()).await;
    proxy.arm_client_id(&space, &owner.identity_id, ids[0]);
    let bob = Open::offline(&world, "bob")?;
    bob.0.start_sync(None)?;
    proxy
        .until(
            Duration::from_secs(20),
            "forward receipt was not intercepted",
            |records| {
                records.iter().any(|record| {
                    matches!(&record.kind,
            final_event_proxy::Kind::Event { id, dropped: true, .. } if id == ids[0])
                })
            },
        )
        .await;
    drop(bob);
    proxy.shutdown().await;
    world.start_relay();
    let bob = Open::online(&world, "bob")
        .await
        .map_err(|error| format!("Bob after relay restart: {error}"))?;
    let replay: serde_json::Value = serde_json::from_str(&bob.0.client_request(confirm).await?)?;
    assert_eq!(replay["batch_id"], accepted["batch_id"]);
    assert!(replay["error"].is_null());
    let replayed = replay["messages"].as_array().ok_or("replayed messages")?;
    assert_eq!(replayed.len(), ids.len());
    for (message, id) in replayed.iter().zip(&ids) {
        assert_eq!(message["accepted_message_id"].as_str(), Some(*id));
    }
    let alice = Open::online(&world, "alice")
        .await
        .map_err(|error| format!("Alice after relay restart: {error}"))?;
    let rows = received(&alice.0, &space, |rows| {
        ids.iter().all(|id| rows.iter().any(|row| row.id == *id))
    })
    .await?;
    for (id, body) in ids
        .iter()
        .zip(["First batch source", "Second batch source"])
    {
        let delivered = find(&rows, id)?;
        assert!(delivered.forwarded);
        assert_eq!(text(&delivered), body);
        assert_eq!(
            rows.iter()
                .filter(|row| row.forwarded && text(row) == body)
                .count(),
            1
        );
    }
    Ok(())
}
