//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn fixture(path: &str) -> Fixture {
    let engine = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    engine
        .create_account(
            "Media draft author".into(),
            "media_draft_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = engine
        .create_group_with("Media draft QA".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let account = engine.account().unwrap();
    Fixture {
        engine,
        vault,
        scope: MediaDraftScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            thread_root_id: None,
        },
    }
}

pub(super) fn input(id: &str) -> MediaDraftAssetInput {
    MediaDraftAssetInput {
        id: id.into(),
        name: format!("{id}.pdf"),
        mime: "application/pdf".into(),
        bytes: format!("Private media bytes {id}").into_bytes(),
    }
}

pub(super) fn attachments(ids: &[&str], reply: Option<&str>) -> String {
    serde_json::json!({"kind":"attachments", "reply_to":reply,
        "items":ids.iter().map(|id| serde_json::json!({"asset_id":id,"caption":format!("Private caption {id}"),"mutation_id":format!("send-{id}")})).collect::<Vec<_>>()}).to_string()
}

pub(super) fn save(
    f: &Fixture,
    revision: &str,
    json: &str,
    assets: Vec<MediaDraftAssetInput>,
) -> MediaDraftDto {
    let result = save_ack(f, revision, json, assets);
    assert_eq!(result.accepted_revision, result.draft.revision);
    result.draft
}

pub(super) fn save_ack(
    f: &Fixture,
    revision: &str,
    json: &str,
    assets: Vec<MediaDraftAssetInput>,
) -> MediaDraftSaveResult {
    f.engine
        .save_media_draft(f.scope.clone(), revision.into(), json.into(), assets)
        .unwrap()
}

pub(super) fn read(f: &Fixture) -> MediaDraftDto {
    f.engine.media_draft(f.scope.clone()).unwrap()
}

pub(super) fn counts(e: &Client) -> Vec<i64> {
    let e = e.lock();
    let mut counts = ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .map(|table| {
            e.store
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .to_vec();
    counts.push(
        e.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key NOT LIKE 'media.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
    );
    counts
}

pub(super) fn rows(f: &Fixture) -> Vec<(String, String)> {
    let e = f.engine.lock();
    let mut statement = e
        .store
        .conn()
        .prepare("SELECT key,value FROM meta WHERE key LIKE 'media.draft.%' ORDER BY key")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

pub(super) fn temp() -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(types::new_id("media-draft-test")?);
    std::fs::create_dir_all(&path).unwrap();
    Ok(path)
}

pub(super) fn asset(f: &Fixture, draft: &MediaDraftDto, id: &str) -> Vec<u8> {
    f.engine
        .media_draft_asset(f.scope.clone(), draft.revision.clone(), id.into())
        .unwrap()
}

pub(super) fn voice(render: bool) -> String {
    serde_json::json!({"kind":"voice","reply_to":null,"source_asset_id":"original",
        "editor":{"trim":[0,12000],"cuts":[[1000,2000]],"transcript":"Private transcript"},
        "render":if render { serde_json::json!({"asset_id":"edited","mime":"audio/mp4","duration_ms":900.5,"levels":[0.1,0.8],"transcript":"spoken result"}) } else { serde_json::Value::Null }}).to_string()
}

pub(super) fn audio(id: &str) -> MediaDraftAssetInput {
    let mut input = input(id);
    input.name = format!("{id}.m4a");
    input.mime = "audio/mp4".into();
    input
}

pub(super) fn media_options(ids: &[&str], presentation: &str, grouping: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&attachments(ids, None)).unwrap();
    value["options"] = serde_json::json!({"presentation":presentation,"grouping":grouping});
    value.to_string()
}

pub(super) fn media_contract_refused_duplicate_creation(
    presentation: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    media_contract_confirm_scope(&f)?;
    let draft = save(
        &f,
        "0",
        &media_options(&["a"], presentation, "separate"),
        vec![input("a")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    media_contract_upload_all(&f);
    let file = f
        .engine
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| {
            matches!(
                row.event.body,
                types::EventBody::ItemCreated {
                    kind: types::ItemKind::File,
                    ..
                }
            )
        })
        .unwrap()
        .event;
    media_contract_confirm_id(&f, &file.client_id)?;
    let duplicate = f
        .engine
        .lock()
        .author_for(&f.scope.expected_identity_id)
        .unwrap()
        .sign_event(
            &f.scope.space_id,
            &types::new_id("duplicate")?,
            file.at_ms + 1,
            None,
            file.body,
        )?;
    f.engine.lock().store.outbox_put(&duplicate).unwrap();
    assert!(f
        .engine
        .lock()
        .reject(
            &duplicate.client_id,
            "duplicate creation refused",
            true,
            None
        )
        .user_failed());
    let rows = f.engine.lock().store.outbox().unwrap();
    assert!(
        !rows
            .iter()
            .find(|row| row.event.client_id == sent.entries[0].id)
            .unwrap()
            .failed
    );
    assert!(f
        .engine
        .lock()
        .outbox_envelopes()
        .iter()
        .any(|env| env.client_id() == sent.entries[0].id));
    media_contract_confirm_id(&f, &sent.entries[0].id)?;
    assert_eq!(
        f.engine
            .timeline(f.scope.space_id.clone())
            .unwrap()
            .iter()
            .find(|entry| entry.id == sent.entries[0].id)
            .unwrap()
            .delivery,
        crate::Delivery::Sent
    );

    Ok(())
}

pub(super) fn media_contract_confirm_scope(
    f: &Fixture,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let pending = f.engine.lock().store.outbox().unwrap();
    let mut relay = f
        .engine
        .lock()
        .logs
        .get(&f.scope.space_id)
        .cloned()
        .unwrap_or_else(|| event_log::SpaceLog::new(&f.scope.space_id));
    for row in pending
        .into_iter()
        .filter(|row| row.event.space == f.scope.space_id && !row.failed)
    {
        let event = relay.sequence(row.event)?;
        let _ = f.engine.lock().ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event),
        });
    }
    Ok(())
}

pub(super) fn media_contract_confirm_id(
    f: &Fixture,
    id: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = f.engine.lock();
    let pending = engine.store.outbox_get(id).unwrap().unwrap();
    let mut relay = engine.logs[&pending.space].clone();
    let event = relay.sequence(pending)?;
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event)
        }),
        crate::sync::Ingest::Confirmed
    );
    Ok(())
}

pub(super) fn media_contract_receive(
    f: &Fixture,
    body: types::EventBody,
    foreign: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut engine = f.engine.lock();
    let author = if foreign {
        event_log::Author::root(event_log::Signer::generate()?)
    } else {
        engine.author_for(&f.scope.expected_identity_id).unwrap()
    };
    let mut relay = engine.logs[&f.scope.space_id].clone();
    let signed = author.sign_event(
        &f.scope.space_id,
        &types::new_id("received")?,
        crate::engine::now_ms(),
        relay.head(),
        body,
    )?;
    let event = relay.sequence(signed)?;
    let _ = engine.ingest(protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event),
    });

    Ok(())
}

pub(super) fn media_contract_recovered_writer(
) -> std::result::Result<(Fixture, String), Box<dyn std::error::Error>> {
    Ok(media_contract_recovered_writer_at(":memory:")?)
}

pub(super) fn media_contract_recovered_writer_at(
    path: &str,
) -> std::result::Result<(Fixture, String), Box<dyn std::error::Error>> {
    let f = fixture(path);
    media_contract_confirm_scope(&f)?;
    let draft = save(
        &f,
        "0",
        &media_options(&["recovery"], "file", "separate"),
        vec![input("recovery")],
    );
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    let crate::EntryKind::Message {
        card: Some(card), ..
    } = &sent.entries[0].kind
    else {
        panic!("missing original file")
    };
    let item = card.item_id.clone();
    media_contract_receive(
        &f,
        types::EventBody::MemberRemoved {
            identity: f.scope.expected_identity_id.clone(),
        },
        false,
    )?;
    let pending = f.engine.lock().store.outbox().unwrap();
    for row in pending {
        assert!(f
            .engine
            .lock()
            .reject(&row.event.client_id, "permission denied", true, None)
            .user_failed());
    }
    media_contract_receive(
        &f,
        types::EventBody::MemberAdded {
            identity: f.scope.expected_identity_id.clone(),
            role: types::Role::Member,
        },
        false,
    )?;
    assert_eq!(
        f.engine.file_bytes(item.clone(), None).unwrap().unwrap(),
        input("recovery").bytes
    );
    Ok((f, item))
}

pub(super) fn media_contract_recovery_command_refused(
    command: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (f, item) = media_contract_recovered_writer()?;
    let before = counts(&f.engine);
    let result = {
        let mut engine = f.engine.lock();
        match command {
            "legacy" => engine
                .post_as(
                    &f.scope.space_id,
                    &f.scope.expected_identity_id,
                    "new legacy claim on recovery bytes",
                    Some(item.clone()),
                )
                .map(|_| ()),
            "version" => engine.file_new_version_typed(
                &item,
                b"must never replace recovery bytes",
                None,
                "unauthorized version",
                "changed.pdf",
                "application/pdf",
            ),
            "revert" => engine
                .restore_version(&item, 1, "unauthorized restore")
                .map(|_| ()),
            _ => unreachable!(),
        }
    };
    assert!(result.is_err(), "recovery-only target authorized {command}");
    assert_eq!(counts(&f.engine), before, "{command} wrote before refusal");
    let file = f.engine.file_info(item.clone()).unwrap().unwrap();
    assert!(!f
        .engine
        .lock()
        .file_edit_allowed(&item, 1, &file.sha256)
        .unwrap());
    assert_eq!(
        f.engine.file_bytes(item, None).unwrap().unwrap(),
        input("recovery").bytes
    );

    Ok(())
}

pub(super) fn media_contract_upload_all(f: &Fixture) {
    let uploads = f.engine.lock().pending_uploads(64);
    for (blob, _) in uploads {
        f.engine.lock().upload_done(&blob);
    }
}
