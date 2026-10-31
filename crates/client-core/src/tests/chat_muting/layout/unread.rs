use super::*;

fn mark(
    f: &Fixture,
    through: Option<String>,
    revision: &str,
) -> Result<ChatLayoutResult, CoreError> {
    f.client.lock().chat_layout(&Input::MarkRead {
        scope: scope(f),
        through_seq: through,
        observed_manual_revision: revision.into(),
    })
}
fn peer_message(f: &Fixture, text: &str) -> Result<u64, Box<dyn std::error::Error>> {
    confirm_media_outbox(&f.client, &f.space)?;
    let peer = {
        let mut engine = f.client.lock();
        let identity = engine.identities[&f.identity].clone();
        let peer = engine.create_identity(identity, true)?;
        engine.add_member(&f.space, &peer, types::Role::Member)?;
        peer
    };
    confirm_media_outbox(&f.client, &f.space)?;
    let mut engine = f.client.lock();
    let signed = engine.author_for(&peer)?.sign_event(
        &f.space,
        &types::new_id("incoming")?,
        crate::engine::now_ms(),
        engine.logs[&f.space].head(),
        EventBody::MessagePosted {
            message: types::new_id("msg")?,
            text: text.into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    let mut relay = engine.logs[&f.space].clone();
    let event = relay.sequence(signed)?.clone();
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(&event)?,
        }),
        crate::sync::Ingest::Applied
    );
    Ok(event.seq)
}

#[test]
fn empty_manual_marker_is_not_a_message_or_receipt_and_open_clears_it() -> TestResult {
    let f = Fixture::new();
    let initial = read(&f)?;
    let marked = change(&f, "manual", Field::ManualUnread, true, "0")?;
    assert!(marked.current.manual_unread);
    assert!(!marked.receipt.ok_or("receipt")?.undo_available);
    assert_eq!(f.summary().unread, 0);
    assert!(f.summary().manual_unread);
    assert_eq!(
        marked.current.read_up_to_seq,
        initial.current.read_up_to_seq
    );
    assert!(f.client.lock().net.receipts.is_empty());
    f.client.mark_read(f.space.clone())?;
    assert!(!f.summary().manual_unread);
    assert_eq!(f.summary().unread, 0);
    assert!(f.client.lock().net.receipts.is_empty());
    let before = f.counts();
    assert!(!reconcile(&f, "manual")?.current.manual_unread);
    assert!(
        !change(&f, "manual", Field::ManualUnread, true, "0")?
            .current
            .manual_unread
    );
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn captured_prefix_keeps_new_messages_unread_and_old_read_cannot_clear_new_marker() -> TestResult {
    let f = Fixture::new();
    let first = peer_message(&f, "First")?;
    let original = change(&f, "first-marker", Field::ManualUnread, true, "0")?.current;
    let second = peer_message(&f, "Later")?;
    assert!(second > first);
    assert_eq!(f.summary().unread, 2);
    let result = mark(
        &f,
        Some(first.to_string()),
        &original.manual_unread_revision,
    )?;
    assert!(!result.current.manual_unread);
    assert_eq!(result.current.read_up_to_seq, Some(first.to_string()));
    assert_eq!(f.summary().unread, 1);
    let before = f.counts();
    mark(
        &f,
        Some(first.to_string()),
        &original.manual_unread_revision,
    )?;
    assert_eq!(f.counts(), before);
    change(
        &f,
        "new-marker",
        Field::ManualUnread,
        true,
        &result.current.manual_unread_revision,
    )?;
    assert!(mark(
        &f,
        Some(first.to_string()),
        &original.manual_unread_revision
    )
    .is_err());
    assert!(f.summary().manual_unread);
    assert_eq!(f.summary().unread, 1);
    assert!(change(
        &f,
        "aba",
        Field::ManualUnread,
        false,
        &original.manual_unread_revision
    )
    .is_err());
    Ok(())
}

#[test]
fn failed_read_watermark_commit_rolls_back_marker_and_signed_log() -> TestResult {
    let f = Fixture::new();
    let seq = peer_message(&f, "Unread")?;
    let current = change(&f, "marker", Field::ManualUnread, true, "0")?.current;
    let before = f.counts();
    f.client.lock().store.raw_execute("CREATE TRIGGER fail_read BEFORE INSERT ON meta WHEN NEW.key LIKE 'read:%' BEGIN SELECT RAISE(ABORT,'read fault'); END;")?;
    assert!(mark(&f, Some(seq.to_string()), &current.manual_unread_revision).is_err());
    assert_eq!(f.counts(), before);
    let after = read(&f)?.current;
    assert!(after.manual_unread);
    assert_eq!(after.manual_unread_revision, current.manual_unread_revision);
    assert_eq!(after.read_up_to_seq, current.read_up_to_seq);
    assert_eq!(f.summary().unread, 1);
    f.client
        .lock()
        .store
        .raw_execute("DROP TRIGGER fail_read")?;
    mark(&f, Some(seq.to_string()), &current.manual_unread_revision)?;
    assert_eq!(f.summary().unread, 0);
    Ok(())
}

#[test]
fn manual_marker_survives_cold_reopen_and_independent_archive_undo() -> TestResult {
    let directory = std::env::temp_dir().join(types::new_id("manual-unread")?);
    std::fs::create_dir(&directory)?;
    let path = directory
        .join("owned.sqlite")
        .to_string_lossy()
        .into_owned();
    let f = Fixture::at(path.clone());
    let target = scope(&f);
    let draft_scope = crate::message_drafts::DraftScope {
        expected_identity_id: f.identity.clone(),
        expected_device_id: f.device.clone(),
        space_id: f.space.clone(),
        thread_root_id: None,
    };
    let draft = serde_json::to_value(f.client.lock().save_message_draft(
        &draft_scope,
        "0",
        "Rascunho 🪷".into(),
        None,
    )?)?;
    let archived = change(&f, "archive", Field::Archived, true, "0")?
        .receipt
        .ok_or("receipt")?;
    change(&f, "marker", Field::ManualUnread, true, "0")?;
    f.client
        .set_chat_prefs(f.space.clone(), true, true, Some(1_900_000_000_000))?;
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    assert_eq!(
        serde_json::to_value(client.lock().message_draft(&draft_scope)?)?,
        draft
    );
    let result = client.lock().chat_layout(&Input::Undo {
        scope: target.clone(),
        operation_id: "undo".into(),
        original_operation_id: "archive".into(),
        observed_revision: archived.revision,
    })?;
    assert!(result.current.manual_unread && result.current.pinned);
    assert!(!result.current.archived);
    assert_eq!(
        client
            .lock()
            .space_summary(&target.space_id)?
            .muted_until_ms,
        Some(1_900_000_000_000)
    );
    assert_eq!(client.lock().space_summary(&target.space_id)?.unread, 0);
    client.mark_read(target.space_id.clone())?;
    assert!(!client.lock().space_summary(&target.space_id)?.manual_unread);
    assert_eq!(
        serde_json::to_value(client.lock().message_draft(&draft_scope)?)?,
        draft
    );
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn unread_and_read_admission_reject_retired_scope_corrupt_state_and_bad_prefix() -> TestResult {
    let f = Fixture::new();
    let marked = change(&f, "manual", Field::ManualUnread, true, "0")?.current;
    let before = f.counts();
    for field in [true, false] {
        let mut target = scope(&f);
        if field {
            target.expected_identity_id = "other".into();
        } else {
            target.expected_device_id = "other".into();
        }
        for input in [
            Input::MarkRead {
                scope: target.clone(),
                through_seq: None,
                observed_manual_revision: marked.manual_unread_revision.clone(),
            },
            Input::Change {
                scope: target,
                operation_id: "foreign".into(),
                field: Field::ManualUnread,
                value: false,
                observed_revision: marked.manual_unread_revision.clone(),
            },
        ] {
            assert!(f.client.lock().chat_layout(&input).is_err());
        }
    }
    for bad in ["0", "01", "18446744073709551616", "1"] {
        assert!(mark(&f, Some(bad.into()), &marked.manual_unread_revision).is_err());
    }
    f.client
        .lock()
        .store
        .set_meta(&format!("read:{}", f.space), "corrupt")?;
    assert!(mark(&f, None, &marked.manual_unread_revision).is_err());
    f.client
        .lock()
        .store
        .meta_delete(&format!("read:{}", f.space))?;
    for revoked in [json!([f.device]).to_string(), "broken".into()] {
        f.client
            .lock()
            .store
            .set_meta("devices.revoked", &revoked)?;
        assert!(mark(&f, None, &marked.manual_unread_revision).is_err());
        assert!(change(
            &f,
            "retired",
            Field::ManualUnread,
            false,
            &marked.manual_unread_revision
        )
        .is_err());
    }
    f.client.lock().store.meta_delete("devices.revoked")?;
    assert_eq!(f.counts(), before);
    f.client.leave_space(f.space.clone())?;
    let before = f.counts();
    assert!(mark(&f, None, &marked.manual_unread_revision).is_err());
    assert!(change(
        &f,
        "left",
        Field::ManualUnread,
        false,
        &marked.manual_unread_revision
    )
    .is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}

#[tokio::test]
async fn unread_and_scoped_read_cross_the_actual_closed_command_wire() -> TestResult {
    let f = Fixture::new();
    let command = json!({"op":"chat_layout","input":{"kind":"change","scope":scope(&f),"operation_id":"wire-manual","field":"manual_unread","value":true,"observed_revision":"0"}});
    let result: serde_json::Value =
        serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
    assert_eq!(result["current"]["manual_unread"], true);
    let command = json!({"op":"chat_layout","input":{"kind":"mark_read","scope":scope(&f),"through_seq":null,"observed_manual_revision":result["current"]["manual_unread_revision"]}});
    let result: serde_json::Value =
        serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
    assert_eq!(result["current"]["manual_unread"], false);
    assert_eq!(f.summary().unread, 0);
    let before = f.counts();
    let mut bad = command;
    bad["input"]["unexpected"] = json!(true);
    assert!(f.client.client_request(bad.to_string()).await.is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}
