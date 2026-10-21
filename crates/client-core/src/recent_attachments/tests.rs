use super::*;
use crate::{MediaDraftAssetInput, PrivacyDto, SecretVault};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use types::{EventBody, Role};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
struct Fixture {
    core: Arc<Client>,
    scope: MediaDraftScope,
    source: String,
}
fn fixture() -> Fixture {
    let core = Client::open(":memory:".into(), "en".into()).unwrap();
    core.create_account(
        "Recent author".into(),
        "recent_author".into(),
        "http://127.0.0.1:1".into(),
        Arc::new(Vault::default()),
    )
    .unwrap();
    let account = core.account().unwrap();
    let source = core
        .create_group_with("Source".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let destination = core
        .create_group_with("Destination".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let f = Fixture {
        core,
        source,
        scope: MediaDraftScope {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: destination,
            thread_root_id: None,
        },
    };
    confirm(&f);
    f
}
fn confirm(f: &Fixture) {
    let pending = f.core.lock().store.outbox().unwrap();
    for row in pending.into_iter().filter(|row| !row.failed) {
        let mut core = f.core.lock();
        let mut relay = core
            .logs
            .get(&row.event.space)
            .cloned()
            .unwrap_or_else(|| event_log::SpaceLog::new(&row.event.space));
        let event = relay.sequence(row.event);
        let _ = core.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event),
        });
    }
}
fn receive(f: &Fixture, author: &str, body: EventBody) {
    let mut core = f.core.lock();
    let mut relay = core.logs[&f.source].clone();
    let signed = core.author_for(author).unwrap().sign_event(
        &f.source,
        &types::new_id("received"),
        crate::engine::now_ms(),
        relay.head(),
        body,
    );
    let event = relay.sequence(signed);
    let _ = core.ingest(protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event),
    });
}
fn add(f: &Fixture, mime: &str, name: &str, explicit_file: bool) -> (String, String) {
    let mut core = f.core.lock();
    let item = core
        .file_add(&f.source, "", name, mime, name.as_bytes(), None)
        .unwrap();
    let event = core
        .append(
            &f.source,
            &f.scope.expected_identity_id,
            EventBody::MessagePosted {
                message: types::new_id("msg"),
                text: "".into(),
                attaches: Some(item.clone()),
                reply: None,
                forwarded: false,
                media: explicit_file.then_some(types::MessageMedia {
                    presentation: AttachmentPresentation::File,
                    group: None,
                }),
            },
        )
        .unwrap();
    (item, event.client_id)
}
fn list(f: &Fixture, kind: RecentAttachmentKind) -> RecentAttachmentPage {
    f.core
        .recent_attachments(f.scope.clone(), kind, "".into(), None, 50)
        .unwrap()
}
fn selected(f: &Fixture) -> RecentAttachment {
    list(f, RecentAttachmentKind::File).items.remove(0)
}
fn input(source: &RecentAttachment, asset_id: &str) -> MediaDraftAssetInput {
    MediaDraftAssetInput {
        id: asset_id.into(),
        name: source.name.clone(),
        mime: source.mime.clone(),
        bytes: source.name.as_bytes().to_vec(),
    }
}
fn content(asset_id: &str) -> String {
    serde_json::json!({"kind":"attachments","reply_to":null,"items":[{"asset_id":asset_id,"caption":"copied","mutation_id":format!("send-{asset_id}")}]}).to_string()
}
fn binding(source: &RecentAttachment, asset_id: &str) -> RecentAttachmentSourceBinding {
    RecentAttachmentSourceBinding {
        asset_id: asset_id.into(),
        kind: RecentAttachmentKind::File,
        reference: source.reference.clone(),
    }
}
fn counts(f: &Fixture) -> Vec<i64> {
    let core = f.core.lock();
    let mut counts = ["events", "media", "media_keys", "blob_uploads", "outbox"]
        .map(|table| {
            core.store
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        })
        .to_vec();
    counts.push(
        core.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key NOT LIKE 'search:%'",
                [],
                |row| row.get(0),
            )
            .unwrap(),
    );
    counts
}

#[test]
fn classifications_follow_current_file_presentation_and_all_author_music_without_voice() {
    let f = fixture();
    let (report, _) = add(&f, "application/pdf", "report.pdf", false);
    add(&f, "image/png", "photo.png", false);
    add(&f, "video/mp4", "movie.mp4", false);
    add(&f, "image/png", "as-file.png", true);
    let (audio, _) = add(&f, "audio/mpeg", "song.mp3", true);
    f.core
        .lock()
        .file_add(
            &f.source,
            "",
            "library-only.pdf",
            "application/pdf",
            b"quiet",
            None,
        )
        .unwrap();
    f.core
        .send_voice_note(
            f.source.clone(),
            vec![1; 32],
            "audio/aac".into(),
            1000,
            vec![0.2, 0.8],
            "voice".into(),
            None,
            false,
        )
        .unwrap();
    confirm(&f);
    let other = {
        let mut core = f.core.lock();
        let mut identity = core.identities[&f.scope.expected_identity_id].clone();
        identity.name = "Other author".into();
        identity.handle = "other_author".into();
        core.create_identity(identity, true).unwrap()
    };
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::MemberAdded {
            identity: other.clone(),
            role: Role::Member,
        },
    );
    receive(
        &f,
        &other,
        EventBody::MessagePosted {
            message: types::new_id("msg"),
            text: "incoming music".into(),
            attaches: Some(audio),
            media: None,
            reply: None,
            forwarded: false,
        },
    );
    receive(
        &f,
        &other,
        EventBody::MessagePosted {
            message: types::new_id("msg"),
            text: "incoming ordinary document".into(),
            attaches: Some(report),
            media: None,
            reply: None,
            forwarded: false,
        },
    );
    let files = list(&f, RecentAttachmentKind::File);
    assert_eq!(
        files.items.len(),
        2,
        "Files are previously sent by self, rather than all documents"
    );
    assert_eq!(
        files
            .items
            .iter()
            .map(|row| row.name.as_str())
            .collect::<std::collections::HashSet<_>>(),
        ["report.pdf", "as-file.png"].into_iter().collect()
    );
    let music = list(&f, RecentAttachmentKind::Audio);
    assert_eq!(music.items.len(), 2);
    assert!(music
        .items
        .iter()
        .any(|row| row.author_title == "Other author"));
    assert!(music.items.iter().all(|row| row.mime == "audio/mpeg"));
}

#[test]
fn bounded_search_filters_before_limit_and_cursors_bind_scope_kind_query_and_generation() {
    let f = fixture();
    for index in 0..65 {
        add(
            &f,
            "application/pdf",
            &format!(
                "{}-{index}.pdf",
                if index % 11 == 0 { "needle" } else { "other" }
            ),
            false,
        );
    }
    confirm(&f);
    let mut cursor = None;
    let mut ids = std::collections::HashSet::new();
    loop {
        let page = f
            .core
            .recent_attachments(
                f.scope.clone(),
                RecentAttachmentKind::File,
                "needle".into(),
                cursor,
                2,
            )
            .unwrap();
        assert!(page.items.len() <= 2);
        for row in &page.items {
            assert!(row.name.starts_with("needle"));
            assert!(ids.insert(row.reference.message_id.clone()));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 6);
    let page = f
        .core
        .recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::File,
            "".into(),
            None,
            1,
        )
        .unwrap();
    let cursor = page.next_cursor.unwrap();
    assert!(f
        .core
        .recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::Audio,
            "".into(),
            Some(cursor.clone()),
            1
        )
        .is_err());
    assert!(f
        .core
        .recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::File,
            "needle".into(),
            Some(cursor.clone()),
            1
        )
        .is_err());
    let mut scope = f.scope.clone();
    scope.space_id = f.source.clone();
    assert!(f
        .core
        .recent_attachments(
            scope,
            RecentAttachmentKind::File,
            "".into(),
            Some(cursor.clone()),
            1
        )
        .is_err());
    add(&f, "application/pdf", "new.pdf", false);
    confirm(&f);
    assert!(matches!(
        f.core.recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::File,
            "".into(),
            Some(cursor),
            1
        ),
        Err(CoreError::Stale { .. })
    ));
    for limit in [0, 51, u32::MAX] {
        assert!(f
            .core
            .recent_attachments(
                f.scope.clone(),
                RecentAttachmentKind::File,
                "".into(),
                None,
                limit
            )
            .is_err());
    }
    assert!(f
        .core
        .recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::File,
            "x".repeat(513),
            None,
            1
        )
        .is_err());
}

#[test]
fn selection_revalidates_deletion_membership_version_hash_and_account_without_writes() {
    for change in [
        "delete",
        "membership",
        "version",
        "hash",
        "account",
        "bytes",
        "name",
        "mime",
    ] {
        let f = fixture();
        let (item, message) = add(&f, "application/pdf", "source.pdf", false);
        confirm(&f);
        let mut source = selected(&f);
        let mut asset = input(&source, "copied");
        let mut scope = f.scope.clone();
        match change {
            "delete" => f.core.delete_message(f.source.clone(), message).unwrap(),
            "membership" => receive(
                &f,
                &f.scope.expected_identity_id,
                EventBody::MemberRemoved {
                    identity: f.scope.expected_identity_id.clone(),
                },
            ),
            "version" => f
                .core
                .lock()
                .file_new_version_typed(
                    &item,
                    b"new",
                    None,
                    "changed",
                    "source.pdf",
                    "application/pdf",
                )
                .unwrap(),
            "hash" => source.reference.sha256 = "0".repeat(64),
            "account" => scope.expected_device_id.push('x'),
            "bytes" => asset.bytes = b"different".to_vec(),
            "name" => asset.name = "other.pdf".into(),
            "mime" => asset.mime = "image/png".into(),
            _ => unreachable!(),
        }
        let before = counts(&f);
        assert!(
            f.core
                .save_media_draft_with_recent_sources(
                    scope,
                    "0".into(),
                    content("copied"),
                    vec![asset],
                    vec![binding(&source, "copied")]
                )
                .is_err(),
            "{change}"
        );
        assert_eq!(counts(&f), before, "{change} wrote durable state");
        assert_eq!(
            f.core.media_draft(f.scope.clone()).unwrap().content_json,
            "null"
        );
    }
}

#[test]
fn accepted_copy_has_exact_retry_receipt_and_fresh_destination_file_and_keys() {
    let f = fixture();
    let (item, message) = add(&f, "application/pdf", "source.pdf", false);
    confirm(&f);
    let source = selected(&f);
    let save = || {
        f.core.save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("copied"),
            vec![input(&source, "copied")],
            vec![binding(&source, "copied")],
        )
    };
    let accepted = save().unwrap();
    f.core.delete_message(f.source.clone(), message).unwrap();
    assert_eq!(
        save().unwrap(),
        accepted,
        "lost ACK replays the already-authorized copy"
    );
    let sent = f
        .core
        .send_media_draft(f.scope.clone(), accepted.draft.revision)
        .unwrap();
    let crate::EntryKind::Message {
        card: Some(card), ..
    } = &sent.entries[0].kind
    else {
        panic!("missing copied file")
    };
    assert_ne!(card.item_id, item);
    let core = f.core.lock();
    let ItemContent::File(old) = &core.state.items[&item].current().content else {
        unreachable!()
    };
    let ItemContent::File(new) = &core.state.items[&card.item_id].current().content else {
        unreachable!()
    };
    assert_eq!(
        (new.mime.as_str(), new.sha256.as_str()),
        (old.mime.as_str(), old.sha256.as_str())
    );
    assert_ne!(new.chunks[0].key, old.chunks[0].key);
}

#[test]
fn disposable_acceleration_repairs_absence_and_refuses_corrupt_rows_explicitly() {
    let f = fixture();
    add(&f, "application/pdf", "source.pdf", false);
    confirm(&f);
    assert_eq!(list(&f, RecentAttachmentKind::File).items.len(), 1);
    f.core
        .lock()
        .store
        .conn()
        .execute_batch("DROP TABLE attachment_recent")
        .unwrap();
    assert_eq!(
        list(&f, RecentAttachmentKind::File).items.len(),
        1,
        "missing projection is rebuilt"
    );
    f.core
        .lock()
        .store
        .conn()
        .execute_batch("UPDATE attachment_recent SET entry_index=9223372036854775807")
        .unwrap();
    assert!(matches!(
        f.core.recent_attachments(
            f.scope.clone(),
            RecentAttachmentKind::File,
            "".into(),
            None,
            10
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        list(&f, RecentAttachmentKind::File).items.len(),
        1,
        "corrupt projection is repaired, never a false empty"
    );
}

#[test]
fn unconfirmed_and_recovery_only_sources_never_become_publishable_recent_files() {
    let f = fixture();
    let (item, message) = add(&f, "application/pdf", "refused.pdf", true);
    assert!(list(&f, RecentAttachmentKind::File).items.is_empty());
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::MemberRemoved {
            identity: f.scope.expected_identity_id.clone(),
        },
    );
    let pending = f.core.lock().store.outbox().unwrap();
    for row in pending {
        assert!(f
            .core
            .lock()
            .reject(&row.event.client_id, "permission denied", true, None)
            .user_failed());
    }
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::MemberAdded {
            identity: f.scope.expected_identity_id.clone(),
            role: Role::Member,
        },
    );
    assert!(f.core.lock().state.is_recovery_item(&item));
    assert!(list(&f, RecentAttachmentKind::File).items.is_empty());
    let file = f.core.file_info_at(item.clone(), Some(1)).unwrap();
    let reference = RecentAttachmentReference {
        space_id: f.source.clone(),
        message_id: message,
        item_id: item,
        version: 1,
        sha256: file.sha256,
    };
    let before = counts(&f);
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("copied"),
            vec![MediaDraftAssetInput {
                id: "copied".into(),
                name: "refused.pdf".into(),
                mime: "application/pdf".into(),
                bytes: b"refused.pdf".to_vec()
            }],
            vec![RecentAttachmentSourceBinding {
                asset_id: "copied".into(),
                kind: RecentAttachmentKind::File,
                reference
            }],
        )
        .is_err());
    assert_eq!(counts(&f), before);
}

#[test]
fn all_source_bindings_are_validated_before_any_member_is_staged() {
    let f = fixture();
    add(&f, "application/pdf", "first.pdf", false);
    add(&f, "application/pdf", "second.pdf", false);
    confirm(&f);
    let rows = list(&f, RecentAttachmentKind::File).items;
    let mut invalid_second = binding(&rows[1], "second");
    invalid_second.reference.version += 1;
    let json = serde_json::json!({"kind":"attachments","reply_to":null,"items":[
        {"asset_id":"first","caption":"","mutation_id":"first"},
        {"asset_id":"second","caption":"","mutation_id":"second"}
    ]})
    .to_string();
    let before = counts(&f);
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            json,
            vec![input(&rows[0], "first"), input(&rows[1], "second")],
            vec![binding(&rows[0], "first"), invalid_second],
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert!(f
        .core
        .media_draft(f.scope.clone())
        .unwrap()
        .assets
        .is_empty());
    assert!(f
        .core
        .save_media_draft_with_recent_sources(
            f.scope.clone(),
            "0".into(),
            content("first"),
            vec![input(&rows[0], "first")],
            vec![binding(&rows[0], "first"), binding(&rows[0], "first")],
        )
        .is_err());
    assert_eq!(counts(&f), before);
}

#[tokio::test]
async fn json_contract_keeps_metadata_bounded_and_rejects_unknown_or_forged_scope_fields() {
    let f = fixture();
    add(&f, "application/pdf", "source.pdf", false);
    confirm(&f);
    let request = serde_json::json!({"op":"recent_attachments","scope":f.scope,"kind":"file","query":"","cursor":null,"limit":1});
    let output: serde_json::Value =
        serde_json::from_str(&f.core.client_request(request.to_string()).await.unwrap()).unwrap();
    assert_eq!(output["items"].as_array().unwrap().len(), 1);
    assert_eq!(output["items"][0]["name"], "source.pdf");
    assert_eq!(output["items"][0]["bytes"], "10");
    assert!(output["items"][0].get("uri").is_none());
    let reference = output["items"][0]["reference"].clone();
    let validate = serde_json::json!({"op":"attachment_reference_validate","scope":f.scope,"kind":"file","reference":reference});
    assert!(f.core.client_request(validate.to_string()).await.is_ok());
    let mut unknown = request.clone();
    unknown["unexpected"] = true.into();
    assert!(f.core.client_request(unknown.to_string()).await.is_err());
    let mut forged = request;
    forged["scope"]["expected_device_id"] = "another-device".into();
    assert!(f.core.client_request(forged.to_string()).await.is_err());
    let mut unknown_ref = validate;
    unknown_ref["reference"]["uri"] = "file:///private/forged".into();
    assert!(f
        .core
        .client_request(unknown_ref.to_string())
        .await
        .is_err());
}
