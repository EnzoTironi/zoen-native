use super::*;
use miniapps::excalidraw::{EMPTY, MIME, RESOURCE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{AppDoc, ItemContent, ItemKind};

fn scene(text: &str) -> String {
    format!(
        " \n{}\n",
        json!({"type":"excalidraw","version":2,"elements":[{"id":"text_1","type":"text","x":10,"y":20,"width":100,"height":24,"text":text,"isDeleted":false}],"appState":{},"files":{},"passiveFuture":{"retained":[1,"unknown"]}})
    )
}
fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}
fn install(client: &mut Client<HostSigner>, space: &str, now: i64) -> (String, DocumentResponse) {
    let response = client.app_request(&json!({"op":"install_app","space_id":space,"app_id":"excalidraw","args_json":"{\"title\":\"Canvas\"}"}).to_string(), now, "en").unwrap();
    (response.result["id"].as_str().unwrap().into(), response)
}
fn read(client: &mut Client<HostSigner>, item: &str, now: i64) -> Value {
    doc(
        client,
        json!({"op":"app_document_read","item_id":item}),
        now,
    )
    .result
}
fn save_request(item: &str, base: &Value, text: &str, id: &str) -> Value {
    json!({"op":"app_document_save","item_id":item,"expected_version":base["version"],"expected_sha256":base["sha256"],"scene_json":text,"mutation_id":id})
}
fn upload_and_confirm(client: &mut Client<HostSigner>, log: &mut SpaceLog, now: i64) {
    let blobs: Vec<String> = serde_json::from_str(&client.document_uploads().unwrap()).unwrap();
    for blob in &blobs {
        assert!(!client.local_chunk(blob).is_empty());
    }
    let update = client.document_uploaded(&blobs).unwrap();
    confirm_documents(client, log, update, now);
}
fn pending_state(client: &Client<HostSigner>) -> String {
    serde_json::to_string(&client.saved).unwrap()
}

#[test]
fn canonical_drawing_install_save_reload_and_ciphertext_barrier_preserve_original_bytes() {
    let (mut client, _, mut log) = document_fixture();
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
    upload_and_confirm(&mut client, &mut log, 12);
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
    upload_and_confirm(&mut reopened, &mut log, 18);
    let canonical = read(&mut reopened, &item, 19);
    assert_eq!(canonical["scene_json"], text);
    let detail = doc(&mut reopened, json!({"op":"item","item_id":item}), 20).result;
    assert_eq!(detail["version"], 2);
    assert!(detail["file"].is_null());
    assert!(!detail["app"]["view_json"]
        .as_str()
        .unwrap()
        .contains("Browser keeps exact"));
}

#[test]
fn drawing_save_has_stale_cas_and_exact_immutable_retry_before_later_reader_permission() {
    let (mut client, author, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    let first = scene("First");
    let request = save_request(&item, &base, &first, "save1");
    let accepted = doc(&mut client, request.clone(), 13).result;
    upload_and_confirm(&mut client, &mut log, 14);
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
    upload_and_confirm(&mut client, &mut log, 17);
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
    );
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
}

#[test]
fn drawing_copy_is_a_new_owned_item_and_exact_retries_never_make_another_copy() {
    let (mut client, author, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let text = scene("Copy exact bytes");
    let request =
        json!({"op":"app_document_copy","item_id":item,"scene_json":text,"mutation_id":"copy1"});
    let copied = doc(&mut client, request.clone(), 12);
    let copied_id = copied.result["item_id"].as_str().unwrap().to_string();
    assert_ne!(copied_id, item);
    assert_eq!(copied.result["version"], 1);
    assert_eq!(read(&mut client, &copied_id, 13)["scene_json"], text);
    upload_and_confirm(&mut client, &mut log, 14);
    let downgraded = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Reader,
        },
        15,
    );
    client.receive(&downgraded, 16).unwrap();
    let before = pending_state(&client);
    assert_eq!(doc(&mut client, request.clone(), 17).result, copied.result);
    assert!(client.document_request(&json!({"op":"app_document_copy","item_id":item,"scene_json":scene("changed intent"),"mutation_id":"copy1"}).to_string(),18).is_err());
    assert_eq!(pending_state(&client), before);
}

#[test]
fn received_drawing_downloads_verify_signed_chunks_and_whole_scene_before_reading() {
    let (mut client, author, mut log) = document_fixture();
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
    );
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
    );
    client.receive(&invalid, 15).unwrap();
    let before = pending_state(&client);
    assert!(client
        .document_request(
            &json!({"op":"app_document_read","item_id":"it_received"}).to_string(),
            16
        )
        .is_err());
    assert_eq!(pending_state(&client), before);
}

#[test]
fn large_scene_requests_are_selective_and_invalid_scenes_leave_every_saved_intent_unchanged() {
    let (mut client, _, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
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
}

#[test]
fn revoked_own_device_cannot_read_cached_drawings_or_replay_saved_intents() {
    let (mut client, _, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    let request = save_request(&item, &base, &scene("Previously accepted"), "accepted");
    doc(&mut client, request.clone(), 13);
    upload_and_confirm(&mut client, &mut log, 14);
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
}

#[test]
fn removed_member_and_broken_log_cannot_export_current_or_historical_drawing_chunk_keys() {
    for removed in [false, true] {
        let (mut client, author, mut log) = document_fixture();
        let (item, _) = install(&mut client, log.space(), 10);
        upload_and_confirm(&mut client, &mut log, 11);
        let base = read(&mut client, &item, 12);
        doc(
            &mut client,
            save_request(&item, &base, &scene("Latest"), "save"),
            13,
        );
        upload_and_confirm(&mut client, &mut log, 14);
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
            );
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
}

#[test]
fn unchanged_drawing_can_record_a_receipt_while_new_encryption_is_unavailable() {
    let (mut client, author, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    let upgraded = clear(&mut log, &author, EventBody::SpaceEncrypted, 13);
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
}

#[test]
fn signed_drawing_activity_keeps_each_canonical_version_and_survives_browser_reload() {
    let (mut client, _, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    for index in 0..3 {
        let now = 20 + index * 10;
        let base = read(&mut client, &item, now);
        doc(
            &mut client,
            save_request(
                &item,
                &base,
                &scene(&format!("Drawing {index}")),
                &format!("activity-{index}"),
            ),
            now + 1,
        );
        let pending = client.conversations();
        let activity = pending[0]
            .messages
            .iter()
            .find(|message| message.delivery == "sending" && message.item_edit.is_some())
            .unwrap();
        assert_eq!(
            activity.item_edit.as_ref().unwrap().version,
            index as u32 + 2
        );
        upload_and_confirm(&mut client, &mut log, now + 2);
    }
    let human = client
        .queue_message(log.space(), "Updated the drawing", 60)
        .unwrap();
    confirm_documents(&mut client, &mut log, human, 61);
    let before = pending_state(&client);
    let views = client.conversations();
    let canonical = log
        .events()
        .iter()
        .filter(|event| matches!(event.body, EventBody::ItemVersioned { .. }))
        .collect::<Vec<_>>();
    let edits = views[0]
        .messages
        .iter()
        .filter(|message| message.item_edit.is_some())
        .collect::<Vec<_>>();
    assert_eq!(edits.len(), 3);
    for (index, (view, event)) in edits.iter().zip(canonical).enumerate() {
        let edit = view.item_edit.as_ref().unwrap();
        assert_eq!(view.id, event.client_id);
        assert_eq!(view.entry.as_deref(), Some(event.hash.as_str()));
        assert_eq!(view.seq, Some(event.seq));
        assert_eq!(edit.space_id, log.space());
        assert_eq!(edit.item_id, item);
        assert_eq!(edit.version, index as u32 + 2);
        assert_eq!(edit.note, view.text);
        assert!(!edit.is_undo);
        assert!(
            view.card.is_none()
                && view.file.is_none()
                && view.quote.is_none()
                && view.thread.is_none()
        );
    }
    let same_words = views[0].messages.last().unwrap();
    assert_eq!(same_words.text, "Updated the drawing");
    assert!(
        same_words.item_edit.is_none(),
        "Human wording must never manufacture activity"
    );
    assert_eq!(
        pending_state(&client),
        before,
        "Projection must not mutate canonical or saved state"
    );
    let restored = Client::new(client.account.clone(), client.signer.clone(), &before).unwrap();
    assert_eq!(
        serde_json::to_value(restored.conversations()).unwrap(),
        serde_json::to_value(views).unwrap()
    );
}

#[test]
fn activity_uses_accepted_scoped_versions_and_keeps_undo_distinct_from_its_restore_target() {
    let (mut client, author, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Version two"), "activity-save"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14);
    let invalid = clear(
        &mut log,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 99,
            note: "Invalid target".into(),
        },
        15,
    );
    client.receive(&invalid, 16).unwrap();
    assert_eq!(
        client.conversations()[0]
            .messages
            .iter()
            .filter(|message| message.item_edit.is_some())
            .count(),
        1
    );
    let restore = clear(
        &mut log,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 1,
            note: "Restored v1".into(),
        },
        17,
    );
    client.receive(&restore, 18).unwrap();
    let views = client.conversations();
    let restored = views[0]
        .messages
        .iter()
        .find_map(|message| message.item_edit.as_ref().filter(|edit| edit.is_undo))
        .unwrap();
    assert_eq!(
        restored.version, 3,
        "Undo is a new canonical version, never version 1"
    );
    assert_eq!(restored.note, "Restored v1");
    let mut foreign = SpaceLog::new("sp_other_activity_scope");
    let created = clear(
        &mut foreign,
        &author,
        EventBody::SpaceCreated {
            title: "Other".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        19,
    );
    client.receive(&created, 20).unwrap();
    let forged_scope = clear(
        &mut foreign,
        &author,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 1,
            note: "Other space".into(),
        },
        21,
    );
    client.receive(&forged_scope, 22).unwrap();
    let views = client.conversations();
    assert!(views
        .iter()
        .find(|chat| chat.id == foreign.space())
        .unwrap()
        .messages
        .is_empty());
    assert_eq!(
        views
            .iter()
            .find(|chat| chat.id == log.space())
            .unwrap()
            .messages
            .iter()
            .filter(|message| message.item_edit.is_some())
            .count(),
        2
    );
}

#[test]
fn signed_second_device_thread_on_activity_survives_annotations_and_exports_boundary_fixture() {
    let (mut client, _, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Keep this drawing"), "activity-thread"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14);
    let activity_event = log
        .events()
        .iter()
        .find(|event| matches!(event.body, EventBody::ItemVersioned { .. }))
        .unwrap()
        .clone();
    // Public clients reject a new reply to a non-message. An independently signed
    // device can still publish this protocol-valid body; readers must stay usable.
    assert!(client
        .queue_post(
            log.space(),
            "public API refuses",
            Some((activity_event.hash.clone(), true)),
            false,
            15
        )
        .is_err());
    let second = Author::device(
        &NativeSigner::from_secret(&[71; 32]),
        NativeSigner::from_secret(&[74; 32]),
    );
    assert_ne!(second.key.id(), client.account.device);
    for (index, note) in [
        types::Annotation::React {
            emoji: "❤️".into()
        },
        types::Annotation::Edit {
            text: "Changed the signed activity".into(),
        },
        types::Annotation::Delete,
    ]
    .into_iter()
    .enumerate()
    {
        let frame = clear(
            &mut log,
            &second,
            EventBody::MessageAnnotated {
                target: activity_event.hash.clone(),
                note,
            },
            20 + index as i64,
        );
        client.receive(&frame, 30 + index as i64).unwrap();
    }
    let reply = clear(
        &mut log,
        &second,
        EventBody::MessagePosted {
            message: "msg_signed_activity_reply".into(),
            text: "Please keep the blue arrow.".into(),
            attaches: None,
            reply: Some(types::ReplyRef::thread(&activity_event.hash)),
            forwarded: false,
        },
        40,
    );
    client.receive(&reply, 41).unwrap();
    let reply_event = log.events().last().unwrap();
    let before = pending_state(&client);
    let conversations = client.conversations();
    let activity = conversations[0]
        .messages
        .iter()
        .find(|message| message.id == activity_event.client_id)
        .unwrap();
    let edit = activity.item_edit.as_ref().unwrap();
    assert_eq!(
        activity.entry.as_deref(),
        Some(activity_event.hash.as_str())
    );
    assert_eq!(
        activity.replies, 1,
        "Never erase the signed human thread to satisfy a DTO parser"
    );
    assert!(!activity.deleted && !activity.edited && activity.reactions.is_empty());
    assert_eq!(activity.text, edit.note);
    let human = conversations[0]
        .messages
        .iter()
        .find(|message| message.id == reply_event.client_id)
        .unwrap();
    assert_eq!(human.thread.as_deref(), Some(activity_event.hash.as_str()));
    assert_eq!(human.text, "Please keep the blue arrow.");
    assert!(human.item_edit.is_none());
    assert_eq!(pending_state(&client), before);
    if let Some(path) = std::env::var_os("ZOEN_WEB_ACTIVITY_FIXTURE") {
        let detail = client
            .item_value(&client.document_item(&item).unwrap(), None, &mut Vec::new())
            .unwrap();
        let fixture = json!({"conversations":conversations,"space_id":log.space(),"item":detail,"activity_id":activity_event.client_id,"activity_hash":activity_event.hash,
            "reply_id":reply_event.client_id,"reply_hash":reply_event.hash});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut output, &serde_json::to_vec_pretty(&fixture).unwrap())
            .unwrap();
    }
}

#[test]
fn signed_duplicate_item_origins_surface_projection_error_without_dropping_human_history() {
    let (mut client, author, mut log) = document_fixture();
    let (item, _) = install(&mut client, log.space(), 10);
    upload_and_confirm(&mut client, &mut log, 11);
    let base = read(&mut client, &item, 12);
    doc(
        &mut client,
        save_request(&item, &base, &scene("Original scope"), "collision-save"),
        13,
    );
    upload_and_confirm(&mut client, &mut log, 14);
    let original = client
        .document_item(&item)
        .unwrap()
        .current()
        .content
        .clone();
    let human = client
        .queue_message(log.space(), "Human history remains available", 15)
        .unwrap();
    confirm_documents(&mut client, &mut log, human, 16);
    let human_id = log.events().last().unwrap().client_id.clone();
    let mut foreign = SpaceLog::new("sp_item_collision");
    let created = clear(
        &mut foreign,
        &author,
        EventBody::SpaceCreated {
            title: "Other legitimate scope".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        17,
    );
    client.receive(&created, 18).unwrap();
    let duplicate = clear(
        &mut foreign,
        &author,
        EventBody::ItemCreated {
            item: item.clone(),
            kind: ItemKind::App,
            content: original,
            origin: "Conflicting origin".into(),
        },
        19,
    );
    client.receive(&duplicate, 20).unwrap();
    let other_human = clear(
        &mut foreign,
        &author,
        EventBody::MessagePosted {
            message: "msg_collision_human".into(),
            text: "Other human history remains available".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
        21,
    );
    client.receive(&other_human, 22).unwrap();
    let reason = client
        .document_items()
        .err()
        .expect("The existing fold must refuse ambiguous ownership");
    let before = pending_state(&client);
    let conversations = client.conversations();
    assert_eq!(conversations.len(), 2);
    for view in &conversations {
        assert_eq!(view.integrity_error.as_deref(), Some(reason.as_str()));
        assert!(view
            .messages
            .iter()
            .any(|message| message.text.contains("human history")
                || message.text.contains("Human history")));
        assert!(
            view.messages
                .iter()
                .all(|message| message.item_edit.is_none()),
            "No ambiguous Item projection is invented"
        );
        let report = client
            .integrity_request(&json!({"op":"verify_log","space_id":view.id}).to_string())
            .unwrap();
        assert_eq!(report["valid"], false);
        assert_eq!(report["error"], reason);
        assert_eq!(report["head_hash"], "");
    }
    assert!(conversations
        .iter()
        .find(|view| view.id == log.space())
        .unwrap()
        .messages
        .iter()
        .any(|message| message.id == human_id));
    assert_eq!(
        pending_state(&client),
        before,
        "Read-only diagnostics never mutate canonical history"
    );
    client
        .saved
        .integrity_errors
        .insert(log.space().into(), "Existing saved fork".into());
    let views = client.conversations();
    assert_eq!(
        views
            .iter()
            .find(|view| view.id == log.space())
            .unwrap()
            .integrity_error
            .as_deref(),
        Some("Existing saved fork")
    );
    assert_eq!(
        client
            .integrity_request(r#"{"op":"integrity_warnings"}"#)
            .unwrap()[log.space()],
        "Existing saved fork"
    );
    if let Some(path) = std::env::var_os("ZOEN_WEB_ACTIVITY_CONFLICT_FIXTURE") {
        let fixture = json!({"conversations":views,"reason":reason,"saved_warnings":client.integrity_request(r#"{"op":"integrity_warnings"}"#).unwrap(),"human_id":human_id});
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut output, &serde_json::to_vec_pretty(&fixture).unwrap())
            .unwrap();
    }
}
