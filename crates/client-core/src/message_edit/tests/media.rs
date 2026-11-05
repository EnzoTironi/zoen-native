use super::*;
use crate::message_media_edit::MessageMediaEdit;
use types::{MediaChange, MediaReplacement};
fn replacement(client: &Client, space: &str, name: &str) -> TestResult<MediaReplacement> {
    let item = client.file_add(
        space.into(),
        name.into(),
        name.into(),
        "image/png".into(),
        vec![1, 2, 3, 4],
        None,
    )?;
    Ok(MediaReplacement {
        item_id: item.id,
        version: item.version,
        sha256: item.file.ok_or("file")?.sha256,
    })
}
fn input(client: &Client, original: &MessageEdit, mutation: &str) -> TestResult<MessageMediaEdit> {
    Ok(MessageMediaEdit {
        expected_identity_id: original.expected_identity_id.clone(),
        expected_device_id: original.expected_device_id.clone(),
        space_id: original.space_id.clone(),
        message_id: original.message_id.clone(),
        edit: MediaChange {
            mutation_id: mutation.into(),
            expected_revision: original.expected_revision.clone(),
            text: "New caption".into(),
            replacement: replacement(client, &original.space_id, "replacement.png")?,
        },
    })
}
fn card(row: &crate::TimelineEntry) -> TestResult<&crate::ItemCard> {
    let crate::EntryKind::Message {
        card: Some(card), ..
    } = &row.kind
    else {
        return Err("card".into());
    };
    Ok(card)
}
#[test]
fn media_edit_preserves_message_identity_reactions_thread_and_exact_retry_after_later_text_edit(
) -> TestResult {
    let (client, _, original) = fixture(":memory:")?;
    client.react(
        original.space_id.clone(),
        original.message_id.clone(),
        Some("🪁".into()),
    )?;
    let reply = client.send_reply(
        original.space_id.clone(),
        "Thread".into(),
        original.message_id.clone(),
        true,
    )?;
    let change = input(&client, &original, "replace-1")?;
    let before = client.timeline(original.space_id.clone())?;
    let result = client.replace_message_media(change.clone())?;
    assert_eq!(result.entry.id, original.message_id);
    assert_eq!(
        result.entry.at_ms,
        before
            .iter()
            .find(|entry| entry.id == original.message_id)
            .ok_or("original")?
            .at_ms
    );
    assert_eq!(
        card(&result.entry)?.item_id,
        change.edit.replacement.item_id
    );
    assert_eq!(result.entry.reactions[0].emoji, "🪁");
    assert!(client
        .thread(original.space_id.clone(), original.message_id.clone())?
        .iter()
        .any(|entry| entry.id == reply.id));
    client.edit_message(
        original.space_id.clone(),
        original.message_id.clone(),
        "Later caption".into(),
    )?;
    let count = counts(&client)?;
    let retried = client.replace_message_media(change.clone())?;
    assert_eq!(retried.applied_revision, result.applied_revision);
    assert_eq!(text(&client, &original)?, "Later caption");
    assert_eq!(counts(&client)?, count);
    let mut collision = change;
    collision.edit.text = "Different".into();
    assert!(client.replace_message_media(collision).is_err());
    assert_eq!(counts(&client)?, count);
    Ok(())
}
#[test]
fn replacement_pins_file_bytes_title_and_forwarded_copy_across_later_file_versions() -> TestResult {
    let (client, _, original) = fixture(":memory:")?;
    let change = input(&client, &original, "pin-media")?;
    client.replace_message_media(change.clone())?;
    client.file_new_version_typed(
        change.edit.replacement.item_id.clone(),
        vec![9, 8, 7],
        None,
        "Later file".into(),
        "different.txt".into(),
        "text/plain".into(),
    )?;
    let row = client
        .timeline(original.space_id.clone())?
        .into_iter()
        .find(|row| row.id == original.message_id)
        .ok_or("row")?;
    assert_eq!(card(&row)?.version, 1);
    assert_eq!(card(&row)?.title, "replacement.png");
    let forwarded = client.forward_message(
        original.space_id.clone(),
        original.message_id.clone(),
        original.space_id.clone(),
    )?;
    let copied = card(&forwarded)?;
    assert_ne!(copied.item_id, change.edit.replacement.item_id);
    assert_eq!(copied.title, "replacement.png");
    assert_eq!(
        client.file_bytes(copied.item_id.clone(), Some(copied.version))?,
        Some(vec![1, 2, 3, 4])
    );
    let hits = client
        .lock()
        .search_messages(&original.space_id, "replacement.png", None, 50)?;
    assert!(hits
        .hits
        .iter()
        .any(|hit| hit.ref_id == original.message_id));
    Ok(())
}
#[test]
fn media_edit_refuses_stale_source_foreign_device_changed_file_and_deleted_message_without_writes(
) -> TestResult {
    let (client, _, original) = fixture(":memory:")?;
    let change = input(&client, &original, "refuse")?;
    let before = counts(&client)?;
    let mut foreign = change.clone();
    foreign.expected_device_id = "foreign".into();
    assert!(client.replace_message_media(foreign).is_err());
    let mut tampered = change.clone();
    tampered.edit.replacement.sha256 = "a".repeat(64);
    assert!(client.replace_message_media(tampered).is_err());
    let mut stale = change.clone();
    stale.edit.expected_revision = "stale".into();
    assert!(client.replace_message_media(stale).is_err());
    assert_eq!(counts(&client)?, before);
    client.delete_message(original.space_id.clone(), original.message_id.clone())?;
    let before = counts(&client)?;
    assert!(client.replace_message_media(change).is_err());
    assert_eq!(counts(&client)?, before);
    Ok(())
}
#[test]
fn media_edit_and_receipt_survive_cold_reopen_without_a_second_annotation() -> TestResult {
    let directory =
        std::env::temp_dir().join(types::new_id("media-edit").map_err(|e| format!("{e:?}"))?);
    std::fs::create_dir(&directory)?;
    let path = directory
        .join("account.sqlite")
        .to_string_lossy()
        .into_owned();
    let (client, vault, original) = fixture(&path)?;
    let change = input(&client, &original, "cold-media")?;
    let accepted = client.replace_message_media(change.clone())?;
    drop(client);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    let before = counts(&client)?;
    let retry = client.replace_message_media(change)?;
    assert_eq!(retry.applied_revision, accepted.applied_revision);
    assert_eq!(card(&retry.entry)?.item_id, card(&accepted.entry)?.item_id);
    assert_eq!(counts(&client)?, before);
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn replacing_an_existing_photo_does_not_change_another_reference_to_its_original_file() -> TestResult
{
    let (client, _, mut original) = fixture(":memory:")?;
    let photo = client.send_file_message_with_caption(
        original.space_id.clone(),
        "original.png".into(),
        "image/png".into(),
        vec![4, 5, 6],
        None,
        None,
        false,
        "original-photo".into(),
        "Original caption".into(),
    )?;
    let old_item = card(&photo)?.item_id.clone();
    let neighbor = client.forward_message(
        original.space_id.clone(),
        photo.id.clone(),
        original.space_id.clone(),
    )?;
    original.message_id = photo.id.clone();
    original.expected_revision = photo.text_revision.ok_or("revision")?;
    let change = input(&client, &original, "swap-photo")?;
    let before = client.timeline(original.space_id.clone())?.len();
    client.replace_message_media(change.clone())?;
    let rows = client.timeline(original.space_id.clone())?;
    assert_eq!(rows.len(), before);
    assert_eq!(
        card(
            rows.iter()
                .find(|entry| entry.id == photo.id)
                .ok_or("photo")?
        )?
        .item_id,
        change.edit.replacement.item_id
    );
    assert_eq!(
        card(
            rows.iter()
                .find(|entry| entry.id == neighbor.id)
                .ok_or("neighbor")?
        )?
        .item_id,
        old_item
    );
    assert_eq!(client.file_bytes(old_item, Some(1))?, Some(vec![4, 5, 6]));
    Ok(())
}

#[test]
fn a_failed_media_annotation_append_keeps_the_original_message_and_all_canonical_counts(
) -> TestResult {
    let (client, _, original) = fixture(":memory:")?;
    let change = input(&client, &original, "atomic-media")?;
    client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER fail_media_edit BEFORE INSERT ON outbox WHEN NEW.json LIKE '%ReplaceMedia%' BEGIN SELECT RAISE(ABORT,'injected media edit failure'); END;")?;
    let before = counts(&client)?;
    assert!(client.replace_message_media(change).is_err());
    assert_eq!(counts(&client)?, before);
    assert_eq!(text(&client, &original)?, "Original 😀");
    Ok(())
}

#[tokio::test]
async fn the_json_media_edit_boundary_returns_the_canonical_row_and_rejects_unreviewed_fields(
) -> TestResult {
    let (client, _, original) = fixture(":memory:")?;
    let change = input(&client, &original, "json-media")?;
    let mut malformed = serde_json::json!({"op":"replace_message_media", "input": change});
    malformed["input"]["edit"]["unreviewed_path"] = "/tmp/foreign".into();
    let before = counts(&client)?;
    assert!(client.client_request(malformed.to_string()).await.is_err());
    assert_eq!(counts(&client)?, before);
    let response = client
        .client_request(
            serde_json::json!({"op":"replace_message_media", "input":change}).to_string(),
        )
        .await?;
    let response: serde_json::Value = serde_json::from_str(&response)?;
    assert_eq!(response["entry"]["id"], original.message_id);
    assert_eq!(
        response["entry"]["kind"]["Message"]["card"]["item_id"],
        change.edit.replacement.item_id
    );
    assert!(response["applied_revision"]
        .as_str()
        .is_some_and(|revision| revision.len() == 64));
    Ok(())
}
