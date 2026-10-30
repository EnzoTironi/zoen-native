use super::*;
use crate::{PrivacyDto, SecretVault};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if let Ok(mut values) = self.0.lock() {
            values.insert(key, value);
            true
        } else {
            false
        }
    }
    fn delete(&self, key: String) {
        if let Ok(mut values) = self.0.lock() {
            values.remove(&key);
        }
    }
}
fn fixture(path: &str) -> TestResult<(Arc<Client>, Arc<Vault>, MessageEdit)> {
    let client = Client::open(path.into(), "en".into())?;
    let vault = Arc::new(Vault::default());
    let account = client.create_account(
        "Editor".into(),
        "reviewed_editor".into(),
        "http://127.0.0.1:1".into(),
        vault.clone(),
    )?;
    let space = client.create_group_with("Edits".into(), Vec::new(), PrivacyDto::Closed)?;
    let entry = client.send_message(space.clone(), "Original 😀".into())?;
    Ok((
        client,
        vault,
        MessageEdit {
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            message_id: entry.id,
            expected_revision: entry.text_revision.ok_or("revision")?,
            text: "Reviewed change 😀".into(),
        },
    ))
}
fn counts(client: &Client) -> TestResult<(u64, usize)> {
    let e = client.lock();
    Ok((e.store.event_count()?, e.store.outbox()?.len()))
}
fn text(client: &Client, input: &MessageEdit) -> TestResult<String> {
    let row = client
        .timeline(input.space_id.clone())?
        .into_iter()
        .find(|e| e.id == input.message_id)
        .ok_or("source")?;
    let crate::EntryKind::Message { text, .. } = row.kind else {
        return Err("not a message".into());
    };
    Ok(text)
}
#[test]
fn reviewed_edit_and_unknown_ack_retry_create_one_annotation() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    let before = counts(&client)?;
    client.edit_reviewed_message(input.clone())?;
    assert_eq!(text(&client, &input)?, input.text);
    assert_eq!(counts(&client)?, (before.0, before.1 + 1));
    client.edit_reviewed_message(input.clone())?;
    assert_eq!(counts(&client)?, (before.0, before.1 + 1));
    Ok(())
}
#[test]
fn observed_new_revision_refuses_old_editor_without_a_write() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    client.edit_message(
        input.space_id.clone(),
        input.message_id.clone(),
        "Newer accepted content".into(),
    )?;
    let before = counts(&client)?;
    assert!(matches!(
        client.edit_reviewed_message(input.clone()),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&client)?, before);
    assert_eq!(text(&client, &input)?, "Newer accepted content");
    Ok(())
}
#[test]
fn wrong_identity_device_and_missing_source_cannot_consume_a_review() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    let before = counts(&client)?;
    for field in ["identity", "device", "source", "revision"] {
        let mut changed = input.clone();
        match field {
            "identity" => changed.expected_identity_id.push('x'),
            "device" => changed.expected_device_id.push('x'),
            "source" => changed.message_id.push('x'),
            _ => changed.expected_revision.push('x'),
        }
        assert!(client.edit_reviewed_message(changed).is_err(), "{field}");
        assert_eq!(counts(&client)?, before);
    }
    Ok(())
}
#[test]
fn deleted_or_privately_removed_message_refuses_even_an_identical_text_retry() -> TestResult {
    for private in [false, true] {
        let (client, _, input) = fixture(":memory:")?;
        client.edit_reviewed_message(input.clone())?;
        let row = client
            .timeline(input.space_id.clone())?
            .into_iter()
            .find(|e| e.id == input.message_id)
            .ok_or("source")?;
        client.delete_messages(crate::MessageDeletion {
            expected_identity_id: input.expected_identity_id.clone(),
            expected_device_id: input.expected_device_id.clone(),
            space_id: input.space_id.clone(),
            for_me: Some(private),
            targets: vec![crate::MessageDeletionTarget {
                message_id: row.id,
                expected_revision: row.text_revision.ok_or("edited revision")?,
            }],
        })?;
        let before = counts(&client)?;
        assert!(client.edit_reviewed_message(input).is_err());
        assert_eq!(counts(&client)?, before);
    }
    Ok(())
}
#[test]
fn storage_refusal_preserves_source_and_allows_the_same_review_to_retry() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    let before = counts(&client)?;
    client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER fail_edit BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'reviewed edit storage fault'); END;")?;
    assert!(client.edit_reviewed_message(input.clone()).is_err());
    assert_eq!(counts(&client)?, before);
    assert_eq!(text(&client, &input)?, "Original 😀");
    client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_edit;")?;
    client.edit_reviewed_message(input.clone())?;
    assert_eq!(counts(&client)?, (before.0, before.1 + 1));
    Ok(())
}
#[test]
fn cold_retry_retains_accepted_text_without_another_annotation() -> TestResult {
    let dir = std::env::temp_dir().join(types::new_id("reviewed_edit")?);
    std::fs::create_dir(&dir)?;
    let path = dir.join("client.sqlite").to_string_lossy().into_owned();
    let (client, vault, input) = fixture(&path)?;
    client.edit_reviewed_message(input.clone())?;
    let before = counts(&client)?;
    drop(client);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    client.edit_reviewed_message(input.clone())?;
    assert_eq!(text(&client, &input)?, input.text);
    assert_eq!(counts(&client)?, before);
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[tokio::test]
async fn public_command_requires_the_reviewed_scope() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    let before = counts(&client)?;
    assert!(client.client_request(serde_json::json!({"op":"edit_message", "space_id":input.space_id, "message_id":input.message_id, "text":input.text}).to_string()).await.is_err());
    assert_eq!(counts(&client)?, before);
    let mut request = serde_json::to_value(&input)?;
    request["op"] = "edit_message".into();
    client.client_request(request.to_string()).await?;
    assert_eq!(text(&client, &input)?, input.text);
    Ok(())
}

#[test]
fn lost_write_access_refuses_even_an_identical_text_reconciliation() -> TestResult {
    let (client, _, input) = fixture(":memory:")?;
    client.edit_reviewed_message(input.clone())?;
    client.lock().append(
        &input.space_id,
        &input.expected_identity_id,
        types::EventBody::MemberAdded {
            identity: input.expected_identity_id.clone(),
            role: types::Role::Reader,
        },
    )?;
    let before = counts(&client)?;
    assert!(matches!(
        client.edit_reviewed_message(input),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(counts(&client)?, before);
    Ok(())
}

#[test]
fn reviewed_caption_can_be_cleared_without_replacing_the_file_or_erasing_an_ordinary_message(
) -> TestResult {
    let (client, _, ordinary) = fixture(":memory:")?;
    let file = client.send_file_message_with_caption(
        ordinary.space_id.clone(),
        "notes.txt".into(),
        "text/plain".into(),
        b"Retain these exact bytes".to_vec(),
        None,
        None,
        false,
        types::new_id("caption")?,
        "Original caption".into(),
    )?;
    let files = serde_json::to_value(client.items())?;
    let mut caption = ordinary.clone();
    caption.message_id = file.id;
    caption.expected_revision = file.text_revision.ok_or("caption revision")?;
    caption.text.clear();
    client.edit_reviewed_message(caption.clone())?;
    assert_eq!(text(&client, &caption)?, "");
    assert_eq!(serde_json::to_value(client.items())?, files);
    let before = counts(&client)?;
    client.edit_reviewed_message(caption)?;
    assert_eq!(counts(&client)?, before);
    let mut empty = ordinary.clone();
    empty.text.clear();
    assert!(client.edit_reviewed_message(empty).is_err());
    assert_eq!(counts(&client)?, before);
    assert_eq!(text(&client, &ordinary)?, "Original 😀");
    Ok(())
}

#[test]
fn a_received_message_never_becomes_an_owned_edit_even_if_its_text_already_matches() -> TestResult {
    let (client, _, mut input) = fixture(":memory:")?;
    let peer = {
        let mut e = client.lock();
        let peer = e.create_identity(
            types::Identity {
                id: String::new(),
                kind: types::IdentityKind::Person,
                name: "Peer".into(),
                handle: "edit_peer".into(),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: None,
                owner_proof: None,
                bio: String::new(),
            },
            true,
        )?;
        e.append(
            &input.space_id,
            &input.expected_identity_id,
            types::EventBody::MemberAdded {
                identity: peer.clone(),
                role: types::Role::Member,
            },
        )?;
        let mut relay = event_log::SpaceLog::new(&input.space_id);
        for pending in e
            .store
            .outbox()?
            .into_iter()
            .filter(|p| p.event.space == input.space_id)
        {
            let event = relay.sequence(pending.event)?.clone();
            assert_eq!(
                e.ingest(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(&event)?
                }),
                crate::sync::Ingest::Confirmed
            );
        }
        let event = e.author_for(&peer)?.sign_event(
            &input.space_id,
            &types::new_id("peer_post")?,
            crate::engine::now_ms(),
            relay.head(),
            types::EventBody::MessagePosted {
                message: types::new_id("peer_message")?,
                text: input.text.clone(),
                attaches: None,
                media: None,
                reply: None,
                forwarded: false,
            },
        )?;
        let event = relay.sequence(event)?.clone();
        assert_eq!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)?
            }),
            crate::sync::Ingest::Applied
        );
        peer
    };
    let source = client
        .timeline(input.space_id.clone())?
        .into_iter()
        .find(|row| row.author.id == peer)
        .ok_or("received source")?;
    input.message_id = source.id;
    input.expected_revision = source.text_revision.ok_or("received revision")?;
    let before = counts(&client)?;
    assert!(matches!(
        client.edit_reviewed_message(input),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(counts(&client)?, before);
    Ok(())
}
