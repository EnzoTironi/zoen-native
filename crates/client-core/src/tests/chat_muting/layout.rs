use super::*;
use crate::chat_layout::{
    ChatLayoutField as Field, ChatLayoutInput as Input, ChatLayoutResult, ChatLayoutScope,
};
use types::EventBody;
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn scope(f: &Fixture) -> ChatLayoutScope {
    ChatLayoutScope {
        space_id: f.space.clone(),
        expected_identity_id: f.identity.clone(),
        expected_device_id: f.device.clone(),
    }
}
fn read(f: &Fixture) -> Result<ChatLayoutResult, CoreError> {
    f.client
        .lock()
        .chat_layout(&Input::Read { scope: scope(f) })
}
fn change(
    f: &Fixture,
    id: &str,
    field: Field,
    value: bool,
    revision: &str,
) -> Result<ChatLayoutResult, CoreError> {
    f.client.lock().chat_layout(&Input::Change {
        scope: scope(f),
        operation_id: id.into(),
        field,
        value,
        observed_revision: revision.into(),
    })
}
fn reconcile(f: &Fixture, id: &str) -> Result<ChatLayoutResult, CoreError> {
    f.client.lock().chat_layout(&Input::Reconcile {
        scope: scope(f),
        operation_id: id.into(),
    })
}
fn undo(
    f: &Fixture,
    id: &str,
    original: &str,
    revision: &str,
) -> Result<ChatLayoutResult, CoreError> {
    f.client.lock().chat_layout(&Input::Undo {
        scope: scope(f),
        operation_id: id.into(),
        original_operation_id: original.into(),
        observed_revision: revision.into(),
    })
}

#[test]
fn field_patch_and_undo_preserve_concurrent_pin_and_raw_finite_mute() -> TestResult {
    let f = Fixture::new();
    let deadline = 1_800_000_000_000;
    f.client
        .set_chat_prefs(f.space.clone(), false, false, Some(deadline))?;
    let start = read(&f)?;
    let changed = change(
        &f,
        "archive-1",
        Field::Archived,
        true,
        &start.current.archive_revision,
    )?;
    let receipt = changed.receipt.ok_or("receipt")?;
    assert!(changed.current.archived && receipt.undo_available);
    let pinned = change(
        &f,
        "pin-1",
        Field::Pinned,
        true,
        &start.current.pin_revision,
    )?;
    assert!(pinned.current.pinned);
    // A later independent mute edit must survive the archive Undo as well.
    let newer_deadline = deadline + 7_000;
    f.client
        .set_chat_prefs(f.space.clone(), true, true, Some(newer_deadline))?;
    let current = reconcile(&f, "archive-1")?;
    assert!(
        current.receipt.ok_or("original")?.undo_available,
        "independent fields do not retire archive undo"
    );
    let reverted = undo(&f, "undo-archive-1", "archive-1", &receipt.revision)?;
    assert!(reverted.current.pinned && !reverted.current.archived);
    assert_eq!(
        f.client.lock().state.chat_prefs[&f.space].muted_until_ms,
        Some(newer_deadline)
    );
    let after = f.counts();
    let retry = undo(&f, "undo-archive-1", "archive-1", &receipt.revision)?;
    assert_eq!(
        retry.receipt.ok_or("undo")?.revision,
        reverted.receipt.ok_or("undo")?.revision
    );
    assert_eq!(
        f.counts(),
        after,
        "lost undo ACK never writes another reversal"
    );
    Ok(())
}

#[test]
fn stale_same_field_and_aba_are_refused_while_original_receipt_remains_readable() -> TestResult {
    let f = Fixture::new();
    let first = change(&f, "archive-a", Field::Archived, true, "0")?
        .receipt
        .ok_or("a")?;
    let second = change(&f, "archive-b", Field::Archived, false, &first.revision)?
        .receipt
        .ok_or("b")?;
    change(&f, "archive-c", Field::Archived, true, &second.revision)?;
    let before = f.counts();
    assert!(matches!(
        undo(&f, "undo-a", "archive-a", &first.revision),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        change(&f, "stale-change", Field::Archived, false, &first.revision),
        Err(CoreError::Stale { .. })
    ));
    let old = reconcile(&f, "archive-a")?.receipt.ok_or("original")?;
    assert_eq!(old.revision, first.revision);
    assert!(!old.undo_available);
    assert!(read(&f)?.current.archived);
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn legacy_full_preference_events_advance_changed_field_revisions_and_preserve_others() -> TestResult
{
    let f = Fixture::new();
    let first = change(&f, "pin", Field::Pinned, true, "0")?
        .receipt
        .ok_or("pin")?;
    f.client
        .set_chat_prefs(f.space.clone(), true, false, Some(i64::MAX))?;
    assert_eq!(
        read(&f)?.current.pin_revision,
        first.revision,
        "mute-only event is not a pin change"
    );
    f.client
        .set_chat_prefs(f.space.clone(), false, false, Some(1))?;
    f.client
        .set_chat_prefs(f.space.clone(), true, false, Some(1))?;
    let before = f.counts();
    assert!(
        undo(&f, "undo-pin", "pin", &first.revision).is_err(),
        "same bool after ABA cannot authorize rollback"
    );
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn retry_reconciles_original_event_after_lost_ack_and_concurrent_edits_without_reapply(
) -> TestResult {
    let f = Fixture::new();
    let original = change(&f, "lost-ack", Field::Archived, true, "0")?
        .receipt
        .ok_or("original")?;
    change(&f, "later", Field::Archived, false, &original.revision)?;
    let before = f.counts();
    let replay = change(&f, "lost-ack", Field::Archived, true, "0")?;
    assert_eq!(replay.receipt.ok_or("receipt")?.revision, original.revision);
    assert!(
        !replay.current.archived,
        "a retry returns its receipt without restoring its old effect"
    );
    assert_eq!(f.counts(), before);
    assert!(reconcile(&f, "never-admitted")?.receipt.is_none());
    assert_eq!(f.counts(), before, "reconciliation only reads");
    Ok(())
}

#[test]
fn replay_and_undo_survive_cold_reopen_using_only_the_signed_personal_log() -> TestResult {
    let directory = std::env::temp_dir().join(types::new_id("chat-layout").unwrap());
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
    let draft = f.client.lock().save_message_draft(
        &draft_scope,
        "0",
        "Rascunho privado 🪷\nAinda sem envio.".into(),
        None,
    )?;
    let saved_draft = serde_json::to_value(draft)?;
    let original = change(&f, "persisted", Field::Archived, true, "0")?
        .receipt
        .ok_or("receipt")?;
    let before = f.counts();
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    {
        let mut engine = client.lock();
        assert_eq!(
            serde_json::to_value(engine.message_draft(&draft_scope)?)?,
            saved_draft
        );
        let result = engine.chat_layout(&Input::Reconcile {
            scope: target.clone(),
            operation_id: "persisted".into(),
        })?;
        assert_eq!(
            result.receipt.ok_or("reopened")?.revision,
            original.revision
        );
        assert_eq!(engine.store.event_count()?, before.0);
        assert_eq!(engine.store.outbox()?.len(), before.1);
        let result = engine.chat_layout(&Input::Undo {
            scope: target,
            operation_id: "undo-cold".into(),
            original_operation_id: "persisted".into(),
            observed_revision: original.revision,
        })?;
        assert!(!result.current.archived);
        assert_eq!(
            serde_json::to_value(engine.message_draft(&draft_scope)?)?,
            saved_draft
        );
    }
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn unchanged_field_does_not_write_or_fabricate_an_undo_receipt() -> TestResult {
    let f = Fixture::new();
    let before = f.counts();
    let result = change(&f, "no-change", Field::Pinned, false, "0")?;
    assert!(result.receipt.is_none());
    assert_eq!(f.counts(), before);
    assert!(reconcile(&f, "no-change")?.receipt.is_none());
    assert!(undo(&f, "undo-no-change", "no-change", "0").is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn receipt_ids_cannot_be_rebound_to_another_field_chat_value_or_revision() -> TestResult {
    let f = Fixture::new();
    change(&f, "intent", Field::Pinned, true, "0")?;
    let before = f.counts();
    for (field, value, revision) in [
        (Field::Archived, true, "0"),
        (Field::Pinned, false, "0"),
        (Field::Pinned, true, "bad"),
    ] {
        assert!(change(&f, "intent", field, value, revision).is_err());
        assert_eq!(f.counts(), before);
    }
    let other = f
        .client
        .create_group_with("Other".into(), vec![], PrivacyDto::Closed)?;
    let mut foreign = scope(&f);
    foreign.space_id = other;
    let before = f.counts();
    assert!(f
        .client
        .lock()
        .chat_layout(&Input::Reconcile {
            scope: foreign,
            operation_id: "intent".into()
        })
        .is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn owner_device_revocation_lock_and_membership_are_checked_before_receipt_or_mutation() -> TestResult
{
    let f = Fixture::new();
    let receipt = change(&f, "owned", Field::Archived, true, "0")?
        .receipt
        .ok_or("owned")?;
    let mut scopes = [scope(&f), scope(&f)];
    scopes[0].expected_identity_id = "other".into();
    scopes[1].expected_device_id = "other".into();
    let before = f.counts();
    for target in scopes {
        assert!(f
            .client
            .lock()
            .chat_layout(&Input::Reconcile {
                scope: target,
                operation_id: "owned".into()
            })
            .is_err());
    }
    for value in [json!([f.device]).to_string(), "broken".into()] {
        f.client.lock().store.set_meta("devices.revoked", &value)?;
        assert!(reconcile(&f, "owned").is_err());
        assert!(undo(&f, "undo", "owned", &receipt.revision).is_err());
    }
    f.client.lock().store.meta_delete("devices.revoked")?;
    let signer = f.client.lock().net.author.take();
    assert!(read(&f).is_err());
    f.client.lock().net.author = signer;
    assert_eq!(f.counts(), before);
    f.client.leave_space(f.space.clone())?;
    let before = f.counts();
    assert!(read(&f).is_err());
    assert!(reconcile(&f, "owned").is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}

#[test]
fn failed_append_never_produces_a_receipt_or_changes_revision() -> TestResult {
    let f = Fixture::new();
    f.client.lock().ensure_personal_space()?;
    let before = f.counts();
    f.client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER fail_layout BEFORE INSERT ON events WHEN NEW.json LIKE '%ChatPrefsSet%' BEGIN SELECT RAISE(ABORT,'injected preference append failure'); END;")?;
    assert!(change(&f, "failed", Field::Archived, true, "0").is_err());
    assert_eq!(f.counts(), before);
    assert!(!read(&f)?.current.archived);
    assert!(reconcile(&f, "failed")?.receipt.is_none());
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_layout;")?;
    assert!(change(&f, "failed", Field::Archived, true, "0")?
        .receipt
        .is_some());
    Ok(())
}

#[test]
fn local_receipts_never_treat_an_optimistic_outbox_as_an_accepted_personal_commit() -> TestResult {
    let f = Fixture::new();
    let personal = f.client.lock().ensure_personal_space()?;
    f.client.lock().net.synced.insert(personal);
    let before = f.counts();
    assert!(change(&f, "not-local", Field::Archived, true, "0").is_err());
    assert_eq!(f.counts(), before);
    assert!(reconcile(&f, "not-local")?.receipt.is_none());
    Ok(())
}

#[tokio::test]
async fn typed_wire_roundtrip_preserves_legacy_flat_commands_and_refuses_mixed_payloads(
) -> TestResult {
    let f = Fixture::new();
    let input = json!({"kind":"change","scope":scope(&f),"operation_id":"wire","field":"archived","value":true,"observed_revision":"0"});
    let command = json!({"op":"chat_layout","input":input});
    let value: serde_json::Value =
        serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
    assert_eq!(value["current"]["archived"], true);
    assert_eq!(value["receipt"]["operation_id"], "wire");
    let before = f.counts();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &f.client.client_request(command.to_string()).await?
        )?,
        value
    );
    assert_eq!(f.counts(), before);
    for bad in [
        json!({"op":"chat_layout","input":{"kind":"read","scope":scope(&f),"value":true}}),
        json!({"op":"chat_layout","input":input,"pinned":true}),
    ] {
        assert!(f.client.client_request(bad.to_string()).await.is_err());
        assert_eq!(f.counts(), before);
    }
    let legacy = json!({"op":"set_chat_prefs","space_id":f.space,"pinned":true,"archived":true,"muted":false,"preserve_mute":true});
    assert_eq!(f.client.client_request(legacy.to_string()).await?, "null");
    assert!(read(&f)?.current.pinned);
    let legacy_event =
        json!({"ChatPrefsSet":{"space":f.space,"prefs":{"pinned":true,"archived":false}}});
    let body: EventBody = serde_json::from_value(legacy_event.clone())?;
    assert_eq!(serde_json::to_value(body)?, legacy_event);
    let engine = f.client.lock();
    let personal = engine.personal_space().ok_or("personal")?;
    let event = engine.logs[&personal].events().iter().find(|event| matches!(&event.body,EventBody::ChatPrefsSet {operation:Some(op),..} if op.id=="wire")).ok_or("event")?;
    let frame = protocol::ClientFrame::Publish {
        env: protocol::Envelope::plain(event)?,
    };
    let decoded =
        protocol::ClientFrame::decode(&frame.encode()).map_err(|error| error.to_string())?;
    let protocol::ClientFrame::Publish { env } = decoded else {
        return Err("publish roundtrip".into());
    };
    assert_eq!(env.content(), event.content.as_slice());
    env.verify()?;
    Ok(())
}
