//! Complete save receipts scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn second_file_failure_rolls_back_entire_batch_and_freezes_exact_retry() {
    let f = fixture(":memory:");
    let draft = save(
        &f,
        "0",
        &attachments(&["a", "b"], None),
        vec![input("a"), input("b")],
    );
    let before = counts(&f.engine);
    let timeline = f.engine.timeline(f.scope.space_id.clone()).unwrap();
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_second BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' AND NEW.json LIKE '%Private caption b%' BEGIN SELECT RAISE(ABORT,'second failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert_eq!(
        f.engine.timeline(f.scope.space_id.clone()).unwrap(),
        timeline
    );
    let attempted = read(&f);
    assert!(attempted.attempted);
    assert_eq!(attempted.assets, draft.assets);
    assert!(matches!(
        f.engine.save_media_draft(
            f.scope.clone(),
            draft.revision.clone(),
            attachments(&["a"], None),
            vec![]
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(asset(&f, &attempted, "b"), input("b").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_second")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 2);
    assert_eq!(sent.draft.content_json, "null");
    for (entry, id) in sent.entries.iter().zip(["a", "b"]) {
        match &entry.kind {
            crate::EntryKind::Message {
                text,
                card: Some(card),
            } => {
                assert_eq!(text, &format!("Private caption {id}"));
                assert_eq!(
                    f.engine
                        .file_bytes(card.item_id.clone(), None)
                        .unwrap()
                        .unwrap(),
                    input(id).bytes
                );
            }
            other => panic!("expected file message, got {other:?}"),
        }
    }
    assert_eq!(
        rows(&f).len(),
        1,
        "accepted batch must release its private asset rows"
    );
    let accepted_counts = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), accepted_counts);
}

pub(super) fn private_asset_and_record_storage_failures_rollback_and_can_retry_without_orphans() {
    let f = fixture(":memory:");
    for trigger in [
        "CREATE TRIGGER fail_private BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.record.%' BEGIN SELECT RAISE(ABORT,'record failed'); END;",
        "CREATE TRIGGER fail_private BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.asset.%' BEGIN SELECT RAISE(ABORT,'asset failed'); END;",
    ] {
        f.engine.lock().store.conn().execute_batch(trigger).unwrap();
        let before = counts(&f.engine);
        assert!(f.engine.save_media_draft(f.scope.clone(), "0".into(), attachments(&["a"], None), vec![input("a")]).is_err());
        assert!(rows(&f).is_empty()); assert_eq!(counts(&f.engine), before);
        f.engine.lock().store.conn().execute_batch("DROP TRIGGER fail_private").unwrap();
    }
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
}

pub(super) fn backup_restore_and_device_erase_never_carry_a_private_media_draft_to_another_account(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture(":memory:");
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    // Confirm the legitimate group history so a restored account can see its normal membership.
    let pending = f.engine.lock().store.outbox().unwrap();
    let mut relay = event_log::SpaceLog::new(f.scope.space_id.clone());
    for pending in pending
        .into_iter()
        .filter(|p| p.event.space == f.scope.space_id)
    {
        let event = relay.sequence(pending.event);
        assert!(matches!(
            f.engine.lock().ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(event)
            }),
            crate::sync::Ingest::Confirmed
        ));
    }
    let private = rows(&f);
    let backup = f.engine.lock().backup_snapshot().unwrap();
    let dir = temp();
    let path = dir.join("backup.sqlite");
    std::fs::write(&path, backup).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'media.draft.%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(rows(&f), private);
    for (key, value) in &private {
        conn.execute(
            "INSERT INTO meta(key,value) VALUES (?1,?2)",
            rusqlite::params![key, value],
        )
        .unwrap();
    }
    drop(conn);
    let header = crate::backup::Header {
        version: 1,
        identity: f.scope.expected_identity_id.clone(),
        relay_url: "http://127.0.0.1:1".into(),
        identity_secret: hex::encode(f.vault.load(crate::sync::VAULT_IDENTITY.into()).unwrap()),
        agreement_secret: hex::encode(
            f.vault
                .load(crate::profile::VAULT_AGREEMENT.into())
                .unwrap(),
        ),
        created_ms: 1,
    };
    let restored = Client::open(":memory:".into(), "en".into()).unwrap();
    restored
        .lock()
        .restore_snapshot(
            &header,
            &std::fs::read(&path).unwrap(),
            event_log::Signer::generate()?,
        )
        .unwrap();
    assert_eq!(
        restored
            .lock()
            .store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'media.draft.%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let mut new_scope = f.scope.clone();
    new_scope.expected_device_id = restored.account().unwrap().device_id;
    assert_eq!(restored.media_draft(new_scope).unwrap().revision, "0");
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    f.engine.erase_device(f.vault.clone()).unwrap();
    assert!(rows(&f).is_empty());
    assert!(f.engine.media_draft(f.scope.clone()).is_err());
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn failed_receipt_commit_rolls_back_canonical_send_and_retains_the_attempted_payload() {
    let f = fixture(":memory:");
    let draft = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'media.draft.record.%' AND EXISTS(SELECT 1 FROM outbox WHERE json LIKE '%MessagePosted%' AND json LIKE '%Private caption a%') BEGIN SELECT RAISE(ABORT,'receipt failed'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_receipt")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision)
        .unwrap();
    assert_eq!(sent.entries.len(), 1);
    assert_eq!(sent.draft.content_json, "null");
}

pub(super) fn exact_lost_save_ack_replay_returns_newer_current_and_checks_supplied_bytes_metadata_and_order(
) {
    let f = fixture(":memory:");
    let original_json = attachments(&["a", "b"], None);
    let first = save(&f, "0", &original_json, vec![input("a"), input("b")]);
    let current = save(
        &f,
        &first.revision,
        &attachments(&["b", "c"], None),
        vec![input("c")],
    );
    let before = rows(&f);
    let counts_before = counts(&f.engine);
    // Whitespace/property ordering is not part of normalized content identity.
    let pretty = serde_json::to_string_pretty(
        &serde_json::from_str::<serde_json::Value>(&original_json).unwrap(),
    )
    .unwrap();
    let replay = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            "0".into(),
            pretty,
            vec![input("a"), input("b")],
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, first.revision);
    assert_eq!(replay.draft, current);
    assert_eq!(
        rows(&f),
        before,
        "replay cannot resurrect pruned a or rewrite newer c"
    );
    let mut changed_bytes = input("a");
    changed_bytes.bytes[0] ^= 1;
    let mut changed_name = input("a");
    changed_name.name = "renamed.pdf".into();
    let mut changed_mime = input("a");
    changed_mime.mime = "image/png".into();
    for inputs in [
        vec![changed_bytes, input("b")],
        vec![changed_name, input("b")],
        vec![changed_mime, input("b")],
        vec![input("b"), input("a")],
        vec![],
    ] {
        assert!(
            matches!(
                f.engine.save_media_draft(
                    f.scope.clone(),
                    "0".into(),
                    original_json.clone(),
                    inputs
                ),
                Err(CoreError::Stale { .. })
            ),
            "JSON equality alone must never acknowledge a different ingress selection"
        );
        assert_eq!(rows(&f), before);
    }
    assert_eq!(read(&f), current);
    assert_eq!(asset(&f, &current, "c"), input("c").bytes);
    assert_eq!(counts(&f.engine), counts_before);
}

pub(super) fn exact_clear_replay_preserves_newer_content_and_save_receipts_survive_send() {
    let f = fixture(":memory:");
    let first = save(&f, "0", &attachments(&["a"], None), vec![input("a")]);
    let clear = save(&f, &first.revision, "null", vec![]);
    let current = save(
        &f,
        &clear.revision,
        &attachments(&["b"], None),
        vec![input("b")],
    );
    let before = rows(&f);
    let replay = f
        .engine
        .save_media_draft(f.scope.clone(), first.revision, " null ".into(), vec![])
        .unwrap();
    assert_eq!(replay.accepted_revision, clear.revision);
    assert_eq!(replay.draft, current);
    assert_eq!(rows(&f), before);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), current.revision.clone())
        .unwrap();
    let accepted_counts = counts(&f.engine);
    let replay = f
        .engine
        .save_media_draft(
            f.scope.clone(),
            clear.revision,
            attachments(&["b"], None),
            vec![input("b")],
        )
        .unwrap();
    assert_eq!(replay.accepted_revision, current.revision);
    assert_eq!(replay.draft, sent.draft);
    assert_eq!(counts(&f.engine), accepted_counts);
    assert!(read(&f).assets.is_empty());
}

pub(super) fn evicted_save_ack_is_stale_without_guessing_or_overwriting_current_payload() {
    let f = fixture(":memory:");
    let original = attachments(&["a"], None);
    let mut current = save(&f, "0", &original, vec![input("a")]);
    for index in 0..MAX_SAVE_RECEIPTS {
        let mut content: serde_json::Value = serde_json::from_str(&original).unwrap();
        content["items"][0]["caption"] = format!("Changed caption {index}").into();
        current = save(&f, &current.revision, &content.to_string(), vec![]);
    }
    let storage = f.engine.lock().media_draft_scope(&f.scope).unwrap();
    assert_eq!(
        f.engine
            .lock()
            .load_media_draft(&storage)
            .unwrap()
            .save_receipts
            .len(),
        MAX_SAVE_RECEIPTS
    );
    let before = rows(&f);
    assert!(matches!(
        f.engine
            .save_media_draft(f.scope.clone(), "0".into(), original, vec![input("a")]),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(rows(&f), before);
    assert_eq!(read(&f), current);
}

pub(super) fn legacy_receipt_without_output_revision_preserves_cold_draft_but_cannot_guess_ack() {
    let dir = temp();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let json = attachments(&["a"], None);
    let draft = save(&f, "0", &json, vec![input("a")]);
    {
        let engine = f.engine.lock();
        let storage = engine.media_draft_scope(&f.scope).unwrap();
        let record = engine.load_media_draft(&storage).unwrap();
        let mut legacy = serde_json::to_value(record).unwrap();
        for receipt in legacy["save_receipts"].as_array_mut().unwrap() {
            receipt.as_object_mut().unwrap().remove("accepted_revision");
        }
        let plain = serde_json::to_vec(&legacy).unwrap();
        let encoded = crate::local_sealed::seal(&storage.key, &storage.aad, &plain).unwrap();
        engine.store.set_meta(&storage.id, &encoded).unwrap();
    }
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f);
    let engine = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(engine.unlock(vault.clone()).unwrap());
    let f = Fixture {
        engine,
        vault,
        scope,
    };
    assert_eq!(read(&f), draft);
    assert_eq!(asset(&f, &draft, "a"), input("a").bytes);
    let retained = rows(&f);
    assert!(matches!(
        f.engine
            .save_media_draft(f.scope.clone(), "0".into(), json.clone(), vec![input("a")]),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        rows(&f),
        retained,
        "unknown receipt output cannot rewrite retained media"
    );
    let next = save_ack(&f, &draft.revision, &json, vec![]);
    assert_eq!(next.accepted_revision, next.draft.revision);
    assert_eq!(asset(&f, &next.draft, "a"), input("a").bytes);
    drop(f);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn media_contract_defaults_preserve_legacy_save_receipts_and_signed_bytes() {
    let f = fixture(":memory:");
    let old = attachments(&["a"], None);
    let draft = save(&f, "0", &old, vec![input("a")]);
    assert!(!draft.content_json.contains("options"));
    let replay = save_ack(
        &f,
        "0",
        &media_options(&["a"], "media", "separate"),
        vec![input("a")],
    );
    assert_eq!(replay.accepted_revision, draft.revision);
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert!(sent.entries[0].media.is_none());
    let pending = f.engine.lock().store.outbox().unwrap();
    let posted = pending
        .iter()
        .find(|pending| pending.event.client_id == sent.entries[0].id)
        .unwrap();
    assert!(!serde_json::to_string(&posted.event.body)
        .unwrap()
        .contains("media"));
    event_log::verify_author(&posted.event).unwrap();
    let before = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), before);
}

pub(super) fn media_contract_group_failure_rolls_back_and_retains_frozen_options() {
    let f = fixture(":memory:");
    let json = media_options(&["a", "b"], "media", "grouped");
    let draft = save(&f, "0", &json, vec![input("a"), input("b")]);
    let before = counts(&f.engine);
    f.engine.lock().store.conn().execute_batch("CREATE TRIGGER fail_album BEFORE INSERT ON outbox WHEN NEW.json LIKE '%MessagePosted%' AND NEW.json LIKE '%Private caption b%' BEGIN SELECT RAISE(ABORT,'fail album'); END;").unwrap();
    assert!(f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .is_err());
    assert_eq!(counts(&f.engine), before);
    assert!(read(&f).attempted);
    assert_eq!(read(&f).content_json, draft.content_json);
    assert!(f
        .engine
        .save_media_draft(
            f.scope.clone(),
            draft.revision.clone(),
            media_options(&["a", "b"], "file", "separate"),
            vec![]
        )
        .is_err());
    f.engine
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_album")
        .unwrap();
    let sent = f
        .engine
        .send_media_draft(f.scope.clone(), draft.revision.clone())
        .unwrap();
    assert_eq!(sent.entries.len(), 2);
    assert_eq!(
        sent.entries[0]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        0
    );
    assert_eq!(
        sent.entries[1]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        1
    );
    let after = counts(&f.engine);
    assert_eq!(
        f.engine
            .send_media_draft(f.scope.clone(), draft.revision)
            .unwrap()
            .entries,
        sent.entries
    );
    assert_eq!(counts(&f.engine), after);
}
