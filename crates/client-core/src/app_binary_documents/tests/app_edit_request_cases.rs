//! Real signed request publication, refusal and durable retry. No model calls.
use super::*;
use crate::app_edit_requests::AppEditRequestInput;
use crate::engine::EntryBody;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn input(f: &Fixture, base: &AppBinaryDocumentDto, id: &str) -> R<AppEditRequestInput> {
    let account = f.client.account().ok_or_else(super::super::unavailable)?;
    Ok(AppEditRequestInput {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: f.space.clone(),
        item_id: base.item_id.clone(),
        expected_version: base.version,
        expected_sha256: base.sha256.clone(),
        text: "  Make the title bold  ".into(),
        mutation_id: id.into(),
    })
}
fn post(f: &Fixture, base: &AppBinaryDocumentDto, id: &str) -> R<crate::TimelineEntry> {
    let command = input(f, base, id)?;
    f.client.lock().request_app_edit(command)
}
#[test]
fn request_is_one_signed_thread_reply_to_exact_card_head_without_document_or_grant_write(
) -> TestResult {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let before = counts(&f);
    let posted = post(&f, &base, "request")?;
    let e = f.client.lock();
    let entry = e
        .space_state(&f.space)?
        .entries
        .iter()
        .find(|entry| entry.id() == posted.id)
        .ok_or("missing canonical request")?;
    let EntryBody::Message {
        text,
        attaches,
        reply: Some(reply),
        marks,
    } = &entry.body
    else {
        return Err("request was not projected as a message".into());
    };
    let binding = reply.artifact.as_ref().ok_or("missing artifact binding")?;
    assert_eq!(text, "Make the title bold");
    assert!(attaches.is_none());
    assert!(reply.thread);
    assert!(!marks.forwarded && !marks.deleted);
    assert_eq!(binding.item_id, base.item_id);
    assert_eq!(binding.version, base.version);
    assert_eq!(binding.sha256, base.sha256);
    assert_eq!(binding.text_sha256, hash(text.as_bytes()));
    assert_eq!(
        e.item_discussion(&f.space, &base.item_id)?
            .root
            .ok_or("missing card")?
            .id,
        reply.to
    );
    drop(e);
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), None, None)?
            .sha256,
        base.sha256
    );
    let after = counts(&f);
    // One durable encrypted outbox admission; relay-confirmed events are separate.
    assert_eq!(after[0], before[0]);
    assert_eq!(after[4], before[4] + 1);
    assert_eq!(&after[1..4], &before[1..4]);
    Ok(())
}
#[test]
fn changed_head_owner_space_or_reused_identifier_refuses_without_appending() -> TestResult {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let before = counts(&f);
    for kind in 0..4 {
        let mut changed = input(&f, &base, "bad")?;
        match kind {
            0 => changed.expected_device_id = "wrong".into(),
            1 => changed.space_id = "foreign".into(),
            2 => changed.expected_version += 1,
            _ => changed.expected_sha256 = "0".repeat(64),
        }
        assert!(f.client.lock().request_app_edit(changed).is_err());
        assert_eq!(counts(&f), before);
    }
    post(&f, &base, "once")?;
    let after = counts(&f);
    let mut changed = input(&f, &base, "once")?;
    changed.text = "A different intention".into();
    assert!(f.client.lock().request_app_edit(changed).is_err());
    assert_eq!(counts(&f), after);
    Ok(())
}
#[test]
fn retry_keeps_original_receipt_after_a_later_document_head_and_after_cold_unlock() -> TestResult {
    let dir = std::env::temp_dir().join(new_id("app-edit-request"));
    std::fs::create_dir(&dir)?;
    let path = dir.join("device.sqlite");
    let path = path.to_str().ok_or("non UTF-8 fixture path")?;
    let f = fixture(path);
    let base = create(&f, "create");
    let posted = post(&f, &base, "lost-ack")?;
    let newer = save(&f, &base, CHANGED, "later")?;
    let retry_input = input(&f, &base, "lost-ack")?;
    let vault = f.vault.clone();
    let before = counts(&f);
    drop(f);
    let client = Client::open(path.into(), "en".into())?;
    assert!(client.unlock(vault)?);
    let retry = client.lock().request_app_edit(retry_input)?;
    assert_eq!(retry.id, posted.id);
    assert_eq!(
        client.app_binary_read(base.item_id, None, None)?.sha256,
        newer.sha256
    );
    assert_eq!(
        client
            .lock()
            .store
            .conn()
            .query_row("SELECT count(*) FROM outbox", [], |row| row
                .get::<_, i64>(0))?,
        before[4]
    );
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[tokio::test]
async fn bridge_returns_definitive_head_conflict_without_erasing_or_publishing_prompt() -> TestResult
{
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let value = input(&f, &base, "stale-request")?;
    save(&f, &base, CHANGED, "new-head")?;
    let before = counts(&f);
    let response = f.client.client_request(json!({"op":"request_app_edit", "expected_identity_id":value.expected_identity_id,
        "expected_device_id":value.expected_device_id,"space_id":value.space_id,"item_id":value.item_id,
        "expected_version":value.expected_version,"expected_sha256":value.expected_sha256,"text":value.text,"mutation_id":value.mutation_id}).to_string()).await?;
    let result: Value = serde_json::from_str(&response)?;
    assert!(result["entry"].is_null());
    assert!(result["conflict"].is_string());
    assert_eq!(counts(&f), before);
    Ok(())
}
