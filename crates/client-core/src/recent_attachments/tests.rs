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
fn fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
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
    confirm(&f)?;
    Ok(f)
}
fn confirm(f: &Fixture) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let pending = f.core.lock().store.outbox().unwrap();
    for row in pending.into_iter().filter(|row| !row.failed) {
        let mut core = f.core.lock();
        let mut relay = core
            .logs
            .get(&row.event.space)
            .cloned()
            .unwrap_or_else(|| event_log::SpaceLog::new(&row.event.space));
        let event = relay.sequence(row.event)?;
        let _ = core.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(event)?,
        });
    }
    Ok(())
}
fn receive(
    f: &Fixture,
    author: &str,
    body: EventBody,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut core = f.core.lock();
    let mut relay = core.logs[&f.source].clone();
    let signed = core.author_for(author).unwrap().sign_event(
        &f.source,
        &types::new_id("received")?,
        crate::engine::now_ms(),
        relay.head(),
        body,
    )?;
    let event = relay.sequence(signed)?;
    let _ = core.ingest(protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event)?,
    });

    Ok(())
}
fn add(
    f: &Fixture,
    mime: &str,
    name: &str,
    explicit_file: bool,
) -> std::result::Result<(String, String), Box<dyn std::error::Error>> {
    let mut core = f.core.lock();
    let item = core
        .file_add(&f.source, "", name, mime, name.as_bytes(), None)
        .unwrap();
    let event = core
        .append(
            &f.source,
            &f.scope.expected_identity_id,
            EventBody::MessagePosted {
                message: types::new_id("msg")?,
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
    Ok((item, event.client_id))
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
fn classifications_follow_current_file_presentation_and_all_author_music_without_voice(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let (report, _) = add(&f, "application/pdf", "report.pdf", false).unwrap();
    add(&f, "image/png", "photo.png", false).unwrap();
    add(&f, "video/mp4", "movie.mp4", false).unwrap();
    add(&f, "image/png", "as-file.png", true).unwrap();
    let (audio, _) = add(&f, "audio/mpeg", "song.mp3", true).unwrap();
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
    confirm(&f)?;
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
    )
    .unwrap();
    receive(
        &f,
        &other,
        EventBody::MessagePosted {
            message: types::new_id("msg").unwrap(),
            text: "incoming music".into(),
            attaches: Some(audio),
            media: None,
            reply: None,
            forwarded: false,
        },
    )
    .unwrap();
    receive(
        &f,
        &other,
        EventBody::MessagePosted {
            message: types::new_id("msg").unwrap(),
            text: "incoming ordinary document".into(),
            attaches: Some(report),
            media: None,
            reply: None,
            forwarded: false,
        },
    )
    .unwrap();
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
    Ok(())
}

#[test]
fn current_source_resolution_does_not_probe_history_as_versions_increase(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let (item, _) = add(&f, "application/pdf", "history.pdf", false).unwrap();
    confirm(&f)?;
    let original = {
        let core = f.core.lock();
        let ItemContent::File(file) = &core.state.items[&item].current().content else {
            panic!("missing file");
        };
        file.clone()
    };
    let mut versions = 1;
    for target in [1, 8, 64, 256] {
        while versions < target {
            let mut file = original.clone();
            file.name = format!("history-{}.pdf", versions + 1);
            receive(
                &f,
                &f.scope.expected_identity_id,
                EventBody::ItemVersioned {
                    item: item.clone(),
                    content: ItemContent::File(file),
                    note: "rename".into(),
                },
            )
            .unwrap();
            versions += 1;
        }
        crate::files::FILE_VERSION_HISTORY_PROBES.with(|count| count.set(0));
        // Rebuild classification, row resolution and the final source check all
        // use the actual current manifest, regardless of its history length.
        f.core.lock().index_dirty = true;
        let source = selected(&f);
        assert_eq!(source.reference.version, target);
        let checked = f
            .core
            .attachment_reference_validate(
                f.scope.clone(),
                RecentAttachmentKind::File,
                source.reference,
            )
            .unwrap();
        let current = f.core.lock().file_info_at(&item, Some(target)).unwrap();
        assert_eq!(checked.reference.sha256, current.sha256);
        let probes = crate::files::FILE_VERSION_HISTORY_PROBES.with(|count| count.get());
        assert_eq!(
            probes, 0,
            "current version must not inspect older manifests"
        );
        eprintln!("recent current-source history={target} historical_comparisons={probes}");
    }
    // Confirm the counter observes the real arbitrary-version fallback; the
    // optimization must preserve historical reads rather than hiding all probes.
    crate::files::FILE_VERSION_HISTORY_PROBES.with(|count| count.set(0));
    let older = f.core.lock().file_info_at(&item, Some(128)).unwrap();
    assert_eq!(older.name, "history-128.pdf");
    assert_eq!(
        crate::files::FILE_VERSION_HISTORY_PROBES.with(|count| count.get()),
        128
    );
    assert!(f.core.lock().file_info_at(&item, Some(257)).is_err());
    Ok(())
}

#[test]
fn accepted_voice_history_survives_rename_revert_and_signed_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    let (item, _) = add(&f, "audio/mpeg", "ordinary.mp3", false).unwrap();
    confirm(&f)?;
    assert_eq!(list(&f, RecentAttachmentKind::Audio).items.len(), 1);
    let ordinary = {
        let core = f.core.lock();
        let ItemContent::File(file) = &core.state.items[&item].current().content else {
            panic!("missing audio file");
        };
        file.clone()
    };
    let mut voice = ordinary.clone();
    voice.path = "VoiceNotes/recording.mp3".into();
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::ItemVersioned {
            item: item.clone(),
            content: ItemContent::File(voice),
            note: "recording".into(),
        },
    )
    .unwrap();
    assert!(f.core.lock().state.items[&item].has_voice_history);
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::ItemVersioned {
            item: item.clone(),
            content: ItemContent::File(ordinary),
            note: "ordinary path again".into(),
        },
    )
    .unwrap();
    receive(
        &f,
        &f.scope.expected_identity_id,
        EventBody::ItemReverted {
            item: item.clone(),
            to_version: 1,
            note: "restore ordinary version".into(),
        },
    )
    .unwrap();
    assert_eq!(f.core.lock().state.items[&item].current().number, 4);
    assert!(list(&f, RecentAttachmentKind::Audio).items.is_empty());
    f.core.lock().reload().unwrap();
    assert!(f.core.lock().state.items[&item].has_voice_history);
    assert!(list(&f, RecentAttachmentKind::Audio).items.is_empty());
    Ok(())
}

#[test]
fn bounded_search_filters_before_limit_and_cursors_bind_scope_kind_query_and_generation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    for index in 0..65 {
        add(
            &f,
            "application/pdf",
            &format!(
                "{}-{index}.pdf",
                if index % 11 == 0 { "needle" } else { "other" }
            ),
            false,
        )
        .unwrap();
    }
    confirm(&f)?;
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
    add(&f, "application/pdf", "new.pdf", false).unwrap();
    confirm(&f)?;
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
    Ok(())
}

#[test]
fn disposable_acceleration_repairs_absence_and_refuses_corrupt_rows_explicitly(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture()?;
    add(&f, "application/pdf", "source.pdf", false).unwrap();
    confirm(&f)?;
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
    Ok(())
}

mod reuse_cases;
