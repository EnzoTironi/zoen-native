use super::*;
use crate::{PrivacyDto, SecretVault};
use serde_json::{json, Value};
use std::io::{Cursor, Read, Write};
use std::sync::{Arc, Mutex};
use types::Role;
const ORIGINAL: &[u8] = include_bytes!("fixtures/original.deckcraft");
const CHANGED: &[u8] = include_bytes!("fixtures/changed.deckcraft");
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
    client: Arc<Client>,
    vault: Arc<Vault>,
    space: String,
}
fn fixture(path: &str) -> Fixture {
    let client = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "Presentation QA".into(),
            "presentation_qa".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = client
        .create_group_with("Presentation".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    Fixture {
        client,
        vault,
        space,
    }
}
fn create(f: &Fixture, id: &str) -> AppBinaryDocumentDto {
    f.client
        .app_binary_create(
            f.space.clone(),
            "Deck".into(),
            STANDARD.encode(ORIGINAL),
            id.into(),
        )
        .unwrap()
}
fn save(
    f: &Fixture,
    base: &AppBinaryDocumentDto,
    bytes: &[u8],
    id: &str,
) -> R<AppBinaryDocumentDto> {
    f.client.app_binary_save(
        base.item_id.clone(),
        base.version,
        base.sha256.clone(),
        STANDARD.encode(bytes),
        id.into(),
        hash(b"test operation"),
    )
}
fn counts(f: &Fixture) -> Vec<i64> {
    let e = f.client.lock();
    [
        "events",
        "media",
        "media_keys",
        "blob_uploads",
        "outbox",
        "meta",
    ]
    .iter()
    .map(|table| {
        e.store
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
#[test]
fn one_signed_app_owns_exact_encrypted_file_versions_and_retry_never_recreates_it() {
    let f = fixture(":memory:");
    let base = create(&f, "create_once");
    let before = counts(&f);
    assert_eq!(create(&f, "create_once"), base);
    assert_eq!(counts(&f), before);
    assert_eq!(STANDARD.decode(&base.data_base64).unwrap(), ORIGINAL);
    assert_eq!(base.kind, "deckcraft");
    assert_eq!(base.author_id, base.actor_id);
    assert_eq!(base.actor_kind, "human");
    assert_eq!(base.engine_revision, deckcraft::ENGINE_REVISION);
    let accepted = save(&f, &base, CHANGED, "change").unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.sha256, hash(CHANGED));
    let e = f.client.lock();
    let item = e.item_state(&base.item_id).unwrap();
    assert_eq!(item.versions.len(), 2);
    let ItemContent::App(app) = &item.current().content else {
        panic!("app");
    };
    assert!(!app.state_json.contains("data_base64"));
    assert!(app.state_json.len() < 1024);
    let file = app.document.as_ref().unwrap();
    assert_eq!(file.mime, deckcraft::MIME);
    assert!(!file.chunks.is_empty());
    assert!(file
        .chunks
        .iter()
        .all(|c| c.blob != c.sha256 && !c.key.is_empty()));
    assert_eq!(
        e.file_bytes(&base.item_id, Some(2)).unwrap().unwrap(),
        CHANGED
    );
}
#[test]
fn disk_reopen_lost_response_retry_and_exact_export_source_keep_original_accepted_head() {
    let dir = std::env::temp_dir().join(new_id("deckcraft-canonical").unwrap());
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("device.sqlite");
    let f = fixture(path.to_str().unwrap());
    let base = create(&f, "create");
    let accepted = save(&f, &base, CHANGED, "lost_response").unwrap();
    let newer = save(&f, &accepted, ORIGINAL, "later").unwrap();
    assert_eq!(newer.version, 3);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    let retry = client
        .app_binary_save(
            base.item_id.clone(),
            base.version,
            base.sha256,
            STANDARD.encode(CHANGED),
            "lost_response".into(),
            hash(b"test operation"),
        )
        .unwrap();
    assert_eq!(retry.version, 2);
    assert_eq!(retry.current_version, 3);
    assert_eq!(retry.sha256, accepted.sha256);
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), Some(2), Some(accepted.sha256.clone()))
            .unwrap(),
        retry
    );
    assert!(matches!(
        client.app_binary_read(base.item_id.clone(), Some(2), Some(newer.sha256)),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap()
            .version,
        3
    );
    assert_eq!(
        client
            .lock()
            .item_state(&base.item_id)
            .unwrap()
            .versions
            .len(),
        3
    );
    drop(client);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn bad_document_stale_base_reused_intent_and_wrong_app_do_not_write() {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let before = counts(&f);
    assert!(save(&f, &base, b"not a native document", "bad").is_err());
    assert!(f
        .client
        .app_binary_save(
            base.item_id.clone(),
            base.version,
            "0".repeat(64),
            STANDARD.encode(CHANGED),
            "hash".into(),
            hash(b"test operation")
        )
        .is_err());
    assert_eq!(counts(&f), before);
    let accepted = save(&f, &base, CHANGED, "same_intent").unwrap();
    let before = counts(&f);
    assert!(save(&f, &base, ORIGINAL, "same_intent").is_err());
    assert!(matches!(
        save(&f, &base, ORIGINAL, "stale"),
        Err(CoreError::Stale { .. })
    ));
    assert!(f
        .client
        .app_binary_create(
            f.space.clone(),
            "Different title".into(),
            STANDARD.encode(ORIGINAL),
            "create".into()
        )
        .is_err());
    assert!(f.client.app_document_read(base.item_id.clone()).is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client.app_binary_read(base.item_id, None, None).unwrap(),
        accepted
    );
}
#[test]
fn identical_bytes_are_an_attributed_next_version_and_old_intent_cannot_replay_after_receipt_eviction(
) {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let first = save(&f, &base, ORIGINAL, "first").unwrap();
    assert_eq!(first.version, 2);
    let mut head = first;
    for i in 0..64 {
        head = save(&f, &head, ORIGINAL, &format!("intent_{i}")).unwrap();
    }
    let before = counts(&f);
    assert!(matches!(
        save(&f, &base, ORIGINAL, "first"),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(counts(&f), before);
    assert_eq!(head.version, 66);
}
#[test]
fn receipt_failure_rolls_back_file_key_chunks_event_outbox_and_retry_is_one_version() {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let before = counts(&f);
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER deny_binary_receipt BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(save(&f, &base, CHANGED, "retry").is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap(),
        base
    );
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER deny_binary_receipt")
        .unwrap();
    assert_eq!(save(&f, &base, CHANGED, "retry").unwrap().version, 2);
    assert_eq!(save(&f, &base, CHANGED, "retry").unwrap().version, 2);
}
#[test]
fn create_receipt_failure_never_leaves_an_orphan_item_key_or_message() {
    let f = fixture(":memory:");
    let before = counts(&f);
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER deny_binary_create BEFORE INSERT ON meta WHEN NEW.key LIKE 'app_document_create_receipts:%' BEGIN SELECT RAISE(ABORT,'synthetic create failure'); END;").unwrap();
    assert!(f
        .client
        .app_binary_create(
            f.space.clone(),
            "Deck".into(),
            STANDARD.encode(ORIGINAL),
            "retry".into()
        )
        .is_err());
    assert_eq!(counts(&f), before);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER deny_binary_create")
        .unwrap();
    let created = create(&f, "retry");
    assert_eq!(create(&f, "retry"), created);
}
#[test]
fn revoked_grant_read_only_membership_and_revoked_device_gate_writes_and_late_retries() {
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let accepted = save(&f, &base, CHANGED, "saved").unwrap();
    {
        let mut e = f.client.lock();
        let me = e.me_id().unwrap();
        let grant = e
            .state
            .grants
            .iter()
            .find(|g| g.grant.grantee.as_deref() == Some(format!("app:{}", base.item_id).as_str()))
            .unwrap()
            .grant
            .id
            .clone();
        e.append(&f.space, &me, EventBody::GrantRevoked { grant })
            .unwrap();
    }
    let before = counts(&f);
    assert!(save(&f, &base, CHANGED, "saved").is_err());
    assert!(save(&f, &accepted, ORIGINAL, "deny").is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), None, None)
            .unwrap(),
        accepted
    );
    {
        let mut e = f.client.lock();
        let me = e.me_id().unwrap();
        e.append(
            &f.space,
            &me,
            EventBody::MemberAdded {
                identity: me.clone(),
                role: Role::Reader,
            },
        )
        .unwrap();
    }
    assert!(f
        .client
        .app_binary_create(
            f.space.clone(),
            "Reader".into(),
            STANDARD.encode(ORIGINAL),
            "reader".into()
        )
        .is_err());
    let device = f.client.account().unwrap().device_id;
    f.client
        .lock()
        .store
        .set_meta("devices.revoked", &json!([device]).to_string())
        .unwrap();
    assert!(f.client.app_binary_read(base.item_id, None, None).is_err());
}
fn padded(bytes: usize) -> Vec<u8> {
    let make = |padding: usize| {
        let mut original = zip::ZipArchive::new(Cursor::new(ORIGINAL)).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..original.len() {
            let mut entry = original.by_index(i).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            writer
                .start_file(
                    entry.name(),
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(&data).unwrap();
        }
        writer
            .start_file(
                "opaque-padding.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&vec![42; padding]).unwrap();
        writer.finish().unwrap().into_inner()
    };
    let overhead = make(0).len();
    make(bytes - overhead)
}
#[tokio::test]
async fn actual_json_ffi_handles_the_bounded_binary_budget_and_preserves_other_operation_limits() {
    let f = fixture(":memory:");
    let raw = padded(deckcraft::MAX_BYTES);
    assert_eq!(raw.len(), deckcraft::MAX_BYTES);
    // This probes container/FFI capacity, not upstream editor acceptance of an opaque extra ZIP entry.
    let request=json!({"op":"app_binary_create","space_id":f.space,"title":"Budget","data_base64":STANDARD.encode(&raw),"mutation_id":"maximum"}).to_string();
    assert!(request.len() <= MAX_BASE64_BYTES + 8 * 1024);
    let encoded = f.client.client_request(request).await.unwrap();
    let doc: AppBinaryDocumentDto = serde_json::from_str(&encoded).unwrap();
    assert_eq!(doc.byte_length as usize, deckcraft::MAX_BYTES);
    assert_eq!(STANDARD.decode(&doc.data_base64).unwrap(), raw);
    let read=f.client.client_request(json!({"op":"app_binary_read","item_id":doc.item_id,"version":1,"expected_sha256":doc.sha256}).to_string()).await.unwrap();
    let reopened: AppBinaryDocumentDto = serde_json::from_str(&read).unwrap();
    assert_eq!(reopened, doc);
    let too_large = "A".repeat(MAX_BASE64_BYTES + 4);
    assert!(f.client.client_request(json!({"op":"app_binary_create","space_id":f.space,"title":"Too big","data_base64":too_large,"mutation_id":"too_big"}).to_string()).await.is_err());
    assert!(f
        .client
        .client_request(
            json!({"op":"app_binary_read","item_id":doc.item_id,"actor_id":"spoof"}).to_string()
        )
        .await
        .is_err());
    assert!(f
        .client
        .client_request(json!({"op":"app_specs","padding":"x".repeat(1_048_577)}).to_string())
        .await
        .is_err());
    let _: Value = serde_json::from_str(&encoded).unwrap();
}

#[test]
fn durable_intent_lookup_prevents_reexecuting_lost_commands_and_refuses_changed_or_evicted_intents()
{
    let f = fixture(":memory:");
    let base = create(&f, "create");
    let intent = hash(b"test operation");
    assert!(f
        .client
        .app_binary_result(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            "command".into(),
            intent.clone()
        )
        .unwrap()
        .is_none());
    let accepted = save(&f, &base, CHANGED, "command").unwrap();
    let later = save(&f, &accepted, ORIGINAL, "later").unwrap();
    let result = f
        .client
        .app_binary_result(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            "command".into(),
            intent.clone(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(result.current_version, 3);
    assert_eq!(result.sha256, accepted.sha256);
    assert!(f
        .client
        .app_binary_result(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            "command".into(),
            hash(b"different command")
        )
        .is_err());
    assert!(matches!(
        f.client.app_binary_result(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            "missing".into(),
            intent.clone()
        ),
        Err(CoreError::Stale { .. })
    ));
    let mut current = later;
    for i in 0..64 {
        current = save(&f, &current, ORIGINAL, &format!("evict_{i}")).unwrap();
    }
    assert!(matches!(
        f.client.app_binary_result(
            base.item_id.clone(),
            base.version,
            base.sha256.clone(),
            "command".into(),
            intent
        ),
        Err(CoreError::Stale { .. })
    ));
}

#[test]
fn a_certified_agent_runtime_uses_its_actual_signed_author_instead_of_a_payload_preference() {
    let owner = fixture(":memory:");
    let owner_id = owner.client.account().unwrap().identity_id;
    let root = [42; 32];
    let agent_id = event_log::Signer::from_secret(&root).id();
    let proof = owner.client.authorize_agent_json(agent_id.clone()).unwrap();
    let agent = Client::open(":memory:".into(), "en".into()).unwrap();
    agent
        .create_agent_account(
            "DeckCraft worker".into(),
            "deckcraft_worker".into(),
            "http://127.0.0.1:1".into(),
            root.to_vec(),
            owner_id.clone(),
            proof,
            Arc::new(Vault::default()),
        )
        .unwrap();
    assert_eq!(agent.agent_owner(), Some(owner_id));
    let space = agent
        .create_group_with(
            "Authorized worker artifact".into(),
            vec![],
            PrivacyDto::Closed,
        )
        .unwrap();
    let first = agent
        .app_binary_create(
            space,
            "Agent deck".into(),
            STANDARD.encode(ORIGINAL),
            "create".into(),
        )
        .unwrap();
    assert_eq!(first.actor_id, agent_id);
    assert_eq!(first.author_id, agent_id);
    assert_eq!(first.actor_kind, "agent");
    let next = agent
        .app_binary_save(
            first.item_id.clone(),
            first.version,
            first.sha256,
            STANDARD.encode(CHANGED),
            "agent_edit".into(),
            hash(b"reviewed command"),
        )
        .unwrap();
    assert_eq!(next.author_id, agent_id);
    assert_eq!(next.version, 2);
    assert!(owner
        .client
        .app_binary_read(first.item_id, None, None)
        .is_err()); // ownership proof grants no Space access
}
