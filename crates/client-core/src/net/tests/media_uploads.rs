//! Public draft sends through the real HTTP upload worker and WebSocket session.
//! Only the relay is a fixture; both clients persist their canonical state normally.
use super::*;
use crate::{Client, MediaDraftAssetInput, MediaDraftScope, PrivacyDto, SecretVault};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

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
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_400_never_confirms_unrecoverable_attachment() {
    terminal_upload(400, true, false).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_413_never_confirms_unrecoverable_attachment() {
    terminal_upload(413, true, false).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn public_media_upload_transient_cold_retry_keeps_exact_intent_and_remote_bytes() {
    terminal_upload(503, true, false).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn public_default_media_upload_refusal_retains_caption_card_and_bytes_after_cold_reopen() {
    terminal_upload(400, false, false).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn public_voice_upload_refusal_retains_voice_metadata_controls_and_bytes_after_cold_reopen() {
    terminal_upload(400, false, true).await.unwrap();
}

#[test]
fn terminal_upload_rolls_back_outbox_and_blob_together(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let sender = Client::open(":memory:".into(), "en".into()).unwrap();
    sender
        .create_account(
            "Rollback".into(),
            "rollback".into(),
            "http://127.0.0.1:1".into(),
            Arc::new(Vault::default()),
        )
        .unwrap();
    let space = sender
        .create_group_with("Atomic upload failure".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut head = None;
    let setup = sender.lock().store.outbox().unwrap();
    for row in setup {
        assert_eq!(
            sender
                .lock()
                .ingest(sequence(Envelope::plain(&row.event), &mut head)?),
            Ingest::Confirmed
        );
    }
    let account = sender.account().unwrap();
    let scope = MediaDraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.clone(),
        thread_root_id: None,
    };
    let saved = sender.save_media_draft(scope.clone(), "0".into(), serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":"grouped"}, "items":[{"asset_id":"a", "caption":"Retain me", "mutation_id":"atomic-a"},{"asset_id":"b", "caption":"Second member", "mutation_id":"atomic-b"}]}).to_string(), vec![MediaDraftAssetInput { id:"a".into(), name:"a.pdf".into(), mime:"application/pdf".into(), bytes:b"private atomic PDF".to_vec() }, MediaDraftAssetInput { id:"b".into(), name:"b.pdf".into(), mime:"application/pdf".into(), bytes:b"second pending blob".to_vec() }]).unwrap();
    let sent = sender
        .send_media_draft(scope, saved.draft.revision)
        .unwrap();
    let later = sender
        .send_message(space.clone(), "Ordinary after aborted album".into())
        .unwrap();
    let mut engine = sender.lock();
    let uploads = engine.pending_uploads(8);
    assert_eq!(uploads.len(), 2);
    let blob = &uploads[0].0;
    assert!(
        engine.outbox_envelopes().is_empty(),
        "a queued File must wait for real upload success"
    );
    engine.store.conn().execute_batch("CREATE TEMP TRIGGER fail_upload_outbox BEFORE UPDATE OF failed ON outbox WHEN NEW.failed = 1 BEGIN SELECT RAISE(ABORT, 'injected refusal transaction failure'); END;").unwrap();
    assert!(engine.upload_refused(blob).is_err());
    assert_eq!(engine.pending_uploads(8), uploads);
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .all(|row| !row.failed && row.last_error.is_none() && row.attempts == 0));
    assert_ne!(
        engine
            .timeline(&space)
            .unwrap()
            .iter()
            .find(|entry| entry.id == sent.entries[0].id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
    engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_upload_outbox")
        .unwrap();
    assert_eq!(engine.upload_refused(blob).unwrap(), vec![space.clone()]);
    assert_eq!(
        engine.pending_uploads(8).len(),
        1,
        "the second blob is deliberately not acknowledged"
    );
    assert!(
        engine
            .outbox_envelopes()
            .iter()
            .any(|env| env.client_id() == later.id),
        "an aborted album's orphan File must not hold a later ordinary head"
    );
    assert!(engine.store.upload_pending(blob).unwrap());
    assert_eq!(
        engine
            .timeline(&space)
            .unwrap()
            .iter()
            .find(|entry| entry.id == sent.entries[0].id)
            .unwrap()
            .delivery,
        crate::Delivery::Failed
    );
    Ok(())
}

#[test]
fn failed_nonfile_attachment_keeps_its_canonical_app_card_after_cold_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = Directory(std::env::temp_dir().join(types::new_id("failed-app-card").unwrap()));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("sender.sqlite");
    let sender = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    sender
        .create_account(
            "App owner".into(),
            "app_owner".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = sender
        .create_group_with("Canonical App".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let me = sender.account().unwrap().identity_id;
    let mut head = None;
    let setup = sender.lock().store.outbox().unwrap();
    for row in setup {
        assert_eq!(
            sender
                .lock()
                .ingest(sequence(Envelope::plain(&row.event), &mut head)?),
            Ingest::Confirmed
        );
    }
    let item = types::new_id("app").unwrap();
    let created = sender.lock().append(&space, &me, EventBody::ItemCreated {
        item: item.clone(), kind: types::ItemKind::App, content: types::ItemContent::App(types::AppDoc {
            app:"countdown".into(), resource_uri:"ui://builtin/countdown".into(), title:"Retained App".into(),
            state_json:serde_json::json!({"title":"Retained App","place":"QA","target":1_800_000_000_000i64,"log":[]}).to_string(), document:None,
        }), origin:"local canonical fixture".into(),
    }).unwrap();
    assert_eq!(
        sender
            .lock()
            .ingest(sequence(Envelope::plain(&created), &mut head)?),
        Ingest::Confirmed
    );
    let sent = sender
        .lock()
        .post_as(&space, &me, "Keep my App message", Some(item))
        .unwrap();
    assert!(matches!(
        &sent.kind,
        crate::EntryKind::Message { card: Some(_), .. }
    ));
    assert!(sender
        .lock()
        .reject(&sent.id, "terminal message refusal", true, None)
        .user_failed());
    let check = |sender: &Client| {
        let timeline = sender.timeline(space.clone()).unwrap();
        let failed = timeline
            .iter()
            .find(|entry| entry.id == sent.id)
            .expect("a refused canonical App message must remain visible");
        assert_eq!(failed.delivery, crate::Delivery::Failed);
        assert_eq!(failed.kind, sent.kind);
    };
    check(&sender);
    drop(sender);
    let sender = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(sender.unlock(vault).unwrap());
    check(&sender);
    Ok(())
}

fn http_request(socket: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut headers = Vec::new();
    let mut byte = [0];
    while !headers.ends_with(b"\r\n\r\n") {
        socket.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16 * 1024);
    }
    let headers = String::from_utf8(headers).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    socket.read_exact(&mut body).unwrap();
    (headers, body)
}

// Valid250ms, mono16-bit PCM WAV. This fixture qualifies retained bytes/DTO,
// not playback hardware or codec support on a device.
fn pcm_wave() -> Vec<u8> {
    let pcm = vec![0u8; 4000];
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(36u32 + pcm.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8000u32.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&pcm);
    bytes
}

mod scenario;
use scenario::terminal_upload;
