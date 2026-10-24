#[test]
fn canonical_drawing_install_save_reload_and_ciphertext_barrier_preserve_original_bytes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let (item, installed) = install(&mut client, log.space(), 10);
    assert!(installed
        .update
        .as_ref()
        .unwrap()
        .frames
        .iter()
        .all(|frame| !matches!(
            ClientFrame::decode(frame).unwrap(),
            ClientFrame::Publish { .. }
        )));
    assert_eq!(read(&mut client, &item, 11)["scene_json"], EMPTY);
    upload_and_confirm(&mut client, &mut log, 12)?;
    let base = read(&mut client, &item, 13);
    let text = scene("Browser keeps exact whitespace and passive fields");
    let saved = doc(&mut client, save_request(&item, &base, &text, "save1"), 14);
    assert_eq!(saved.result["version"], 2);
    assert_eq!(saved.result["sha256"], hash(&text));
    assert_eq!(saved.result["scene_json"], text);
    assert!(saved
        .update
        .as_ref()
        .unwrap()
        .frames
        .iter()
        .all(|frame| !matches!(
            ClientFrame::decode(frame).unwrap(),
            ClientFrame::Publish { .. }
        )));
    let snapshot = saved.update.unwrap().snapshot;
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(read(&mut reopened, &item, 15), saved.result);
    let manifest = doc(
        &mut reopened,
        json!({"op":"file_manifest","item_id":item,"version":2}),
        16,
    )
    .result;
    let file: types::FileDoc = serde_json::from_value(manifest["doc"].clone()).unwrap();
    assert_eq!(file.mime, MIME);
    let bytes = file
        .chunks
        .iter()
        .flat_map(|chunk| {
            crate::media::open_chunk(chunk, &reopened.local_chunk(&chunk.blob)).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(bytes, text.as_bytes());
    assert_ne!(file.chunks[0].blob, file.chunks[0].sha256);
    reopened
        .receive(&ServerFrame::SyncDone.encode(), 17)
        .unwrap();
    upload_and_confirm(&mut reopened, &mut log, 18)?;
    let canonical = read(&mut reopened, &item, 19);
    assert_eq!(canonical["scene_json"], text);
    let detail = doc(&mut reopened, json!({"op":"item","item_id":item}), 20).result;
    assert_eq!(detail["version"], 2);
    assert!(detail["file"].is_null());
    assert!(!detail["app"]["view_json"]
        .as_str()
        .unwrap()
        .contains("Browser keeps exact"));
    Ok(())
}

#[test]
fn drawing_save_has_stale_cas_and_exact_immutable_retry_before_later_reader_permission(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    let first = scene("First");
    let request = save_request(&item, &base, &first, "save1");
    let accepted = doc(&mut client, request.clone(), 13).result;
    upload_and_confirm(&mut client, &mut log, 14)?;
    let before = pending_state(&client);
    assert!(client
        .document_request(
            &save_request(&item, &base, &scene("stale"), "stale").to_string(),
            15
        )
        .is_err());
    assert_eq!(pending_state(&client), before);
    let second = scene("Second");
    doc(
        &mut client,
        save_request(&item, &accepted, &second, "save2"),
        16,
    );
    upload_and_confirm(&mut client, &mut log, 17)?;
    let same = read(&mut client, &item, 18);
    let versions = client.saved.outbox.len();
    assert_eq!(
        doc(
            &mut client,
            save_request(&item, &same, &second, "unchanged"),
            19
        )
        .result["version"],
        3
    );
    assert_eq!(client.saved.outbox.len(), versions);
    let downgraded = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Reader,
        },
        20,
    )?;
    client.receive(&downgraded, 21).unwrap();
    let before = pending_state(&client);
    let retry = doc(&mut client, request.clone(), 22).result;
    assert_eq!(retry["version"], 2);
    assert_eq!(retry["current_version"], 3);
    assert_eq!(retry["scene_json"], first);
    assert!(client
        .document_request(
            &save_request(&item, &base, &second, "save1").to_string(),
            23
        )
        .is_err());
    assert!(client
        .document_request(
            &save_request(&item, &same, &scene("fresh reader write"), "denied").to_string(),
            24
        )
        .is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

#[test]
fn drawing_copy_is_a_new_owned_item_and_exact_retries_never_make_another_copy(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let text = scene("Copy exact bytes");
    let request =
        json!({"op":"app_document_copy","item_id":item,"scene_json":text,"mutation_id":"copy1"});
    let copied = doc(&mut client, request.clone(), 12);
    let copied_id = copied.result["item_id"].as_str().unwrap().to_string();
    assert_ne!(copied_id, item);
    assert_eq!(copied.result["version"], 1);
    assert_eq!(read(&mut client, &copied_id, 13)["scene_json"], text);
    upload_and_confirm(&mut client, &mut log, 14)?;
    let downgraded = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Reader,
        },
        15,
    )?;
    client.receive(&downgraded, 16).unwrap();
    let before = pending_state(&client);
    assert_eq!(doc(&mut client, request.clone(), 17).result, copied.result);
    assert!(client.document_request(&json!({"op":"app_document_copy","item_id":item,"scene_json":scene("changed intent"),"mutation_id":"copy1"}).to_string(),18).is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

#[test]
fn received_drawing_downloads_verify_signed_chunks_and_whole_scene_before_reading(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let text = scene("Native peer scene");
    let sealed =
        crate::media::seal_file("drawing.excalidraw", MIME, text.as_bytes(), None).unwrap();
    let received = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_received".into(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "excalidraw".into(),
                resource_uri: RESOURCE.into(),
                title: "Native".into(),
                state_json: "{}".into(),
                document: Some(sealed.doc.clone()),
            }),
            origin: "Native".into(),
        },
        10,
    )?;
    client.receive(&received, 11).unwrap();
    let waiting = doc(
        &mut client,
        json!({"op":"app_document_read","item_id":"it_received"}),
        12,
    );
    assert!(waiting.result.is_null());
    assert_eq!(waiting.downloads, sealed.doc.chunks);
    for reference in waiting.downloads {
        let upload = sealed
            .uploads
            .iter()
            .find(|upload| upload.blob == reference.blob)
            .unwrap();
        let plain = crate::media::open_chunk(&reference, &upload.sealed).unwrap();
        let mut changed = plain.clone();
        changed[0] ^= 1;
        assert!(client
            .document_cache_blob(reference.clone(), changed)
            .is_err());
        client.document_cache_blob(reference, plain).unwrap();
    }
    assert_eq!(read(&mut client, "it_received", 13)["scene_json"], text);
    let mut wrong = sealed.doc;
    wrong.sha256 = "0".repeat(64);
    let invalid = clear(
        &mut log,
        &author,
        EventBody::ItemVersioned {
            item: "it_received".into(),
            content: ItemContent::App(AppDoc {
                app: "excalidraw".into(),
                resource_uri: RESOURCE.into(),
                title: "Native".into(),
                state_json: "{}".into(),
                document: Some(wrong),
            }),
            note: "invalid descriptor".into(),
        },
        14,
    )?;
    client.receive(&invalid, 15).unwrap();
    let before = pending_state(&client);
    assert!(client
        .document_request(
            &json!({"op":"app_document_read","item_id":"it_received"}).to_string(),
            16
        )
        .is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

#[test]
fn large_scene_requests_are_selective_and_invalid_scenes_leave_every_saved_intent_unchanged(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    let mut value: Value = serde_json::from_str(&scene("large")).unwrap();
    value["passiveFuture"] = json!("\\\"".repeat(600_000));
    let text = value.to_string();
    let command = save_request(&item, &base, &text, "large");
    assert!(command.to_string().len() > 1 << 20);
    let saved = doc(&mut client, command, 13).result;
    assert_eq!(saved["sha256"], hash(&text));
    let before = pending_state(&client);
    assert!(client
        .document_request(
            &json!({"op":"page_parse_markdown","markdown":"x".repeat((1<<20)+1)}).to_string(),
            14
        )
        .is_err());
    assert!(client.document_request(&save_request(&item, &saved, r#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{"x":{"id":"x","mimeType":"image/png","dataURL":"https://outside.invalid/x.png"}}}"#, "bad").to_string(),15).is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

#[test]
fn revoked_own_device_cannot_read_cached_drawings_or_replay_saved_intents(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    let request = save_request(&item, &base, &scene("Previously accepted"), "accepted");
    doc(&mut client, request.clone(), 13);
    upload_and_confirm(&mut client, &mut log, 14)?;
    assert!(!read(&mut client, &item, 15)["scene_json"]
        .as_str()
        .unwrap()
        .is_empty());
    client
        .saved
        .membership
        .revoked
        .insert(client.account.device.clone());
    let before = pending_state(&client);
    for command in [
        json!({"op":"app_document_read","item_id":item}),
        json!({"op":"file_manifest","item_id":item,"version":null}),
        json!({"op":"file_manifest","item_id":item,"version":1}),
        request,
        json!({"op":"app_document_copy","item_id":item,"scene_json":scene("Copy"),"mutation_id":"copy"}),
    ] {
        assert!(client.document_request(&command.to_string(), 16).is_err());
    }
    assert!(client.app_request(&json!({"op":"install_app","space_id":log.space(),"app_id":"excalidraw","args_json":"{}"}).to_string(),17,"en").is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

#[test]
fn removed_member_and_broken_log_cannot_export_current_or_historical_drawing_chunk_keys(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for removed in [false, true] {
        let (mut client, author, mut log) = document_fixture()?;
        let (item, _) = install(&mut client, log.space(), 10);
        upload_and_confirm(&mut client, &mut log, 11)?;
        let base = read(&mut client, &item, 12);
        doc(
            &mut client,
            save_request(&item, &base, &scene("Latest"), "save"),
            13,
        );
        upload_and_confirm(&mut client, &mut log, 14)?;
        for version in [Value::Null, json!(1)] {
            assert!(!doc(
                &mut client,
                json!({"op":"file_manifest","item_id":item,"version":version}),
                15
            )
            .result["doc"]
                .is_null());
        }
        if removed {
            let removal = clear(
                &mut log,
                &author,
                EventBody::MemberRemoved {
                    identity: author.identity.clone(),
                },
                16,
            )?;
            client.receive(&removal, 17).unwrap();
        } else {
            client
                .saved
                .integrity_errors
                .insert(log.space().into(), "broken log".into());
        }
        let before = pending_state(&client);
        for command in [
            json!({"op":"app_document_read","item_id":item}),
            json!({"op":"file_manifest","item_id":item,"version":null}),
            json!({"op":"file_manifest","item_id":item,"version":1}),
        ] {
            assert!(client.document_request(&command.to_string(), 18).is_err());
        }
        assert_eq!(pending_state(&client), before);
    }
    Ok(())
}

#[test]
fn unchanged_drawing_can_record_a_receipt_while_new_encryption_is_unavailable(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11)?;
    let base = read(&mut client, &item, 12);
    let upgraded = clear(&mut log, &author, EventBody::SpaceEncrypted, 13)?;
    client.receive(&upgraded, 14).unwrap();
    let epoch = client.device().unwrap().epoch(log.space()).unwrap();
    client
        .saved
        .blocked_epochs
        .insert(log.space().into(), epoch);
    assert!(client.queue_epoch(log.space()).is_err());
    let outbox = serde_json::to_string(&client.saved.outbox).unwrap();
    let noop = doc(
        &mut client,
        save_request(&item, &base, EMPTY, "unchanged"),
        15,
    )
    .result;
    assert_eq!(noop["version"], 1);
    assert_eq!(noop["scene_json"], EMPTY);
    assert_eq!(serde_json::to_string(&client.saved.outbox).unwrap(), outbox);
    let before = pending_state(&client);
    assert!(client
        .document_request(
            &save_request(&item, &base, &scene("needs epoch"), "fresh").to_string(),
            16
        )
        .is_err());
    assert_eq!(pending_state(&client), before);
    Ok(())
}

