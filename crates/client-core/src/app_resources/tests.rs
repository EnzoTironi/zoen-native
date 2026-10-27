use super::*;
use crate::{Client, PrivacyDto, SecretVault};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    input: Install,
    html: String,
}
fn fixture(path: &str) -> Fixture {
    let client = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "App owner".into(),
            "app_owner".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let space = client
        .create_group_with("Apps".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let html = "<!doctype html><html><head></head><body>Owned MCP App</body></html>".to_owned();
    let package = client
        .file_add(
            space.clone(),
            "".into(),
            "editor.html".into(),
            MIME.into(),
            html.as_bytes().to_vec(),
            None,
        )
        .unwrap();
    let account = client.account().unwrap();
    let input = Install {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space,
        package_item_id: package.id,
        package_version: 1,
        package_sha256: hash(html.as_bytes()),
        title: "Shared drawing".into(),
        mutation_id: "install_once".into(),
    };
    Fixture {
        client,
        vault,
        input,
        html,
    }
}
async fn install(f: &Fixture) -> ItemDetail {
    let result: serde_json::Value = serde_json::from_str(
        &f.client
            .client_request(json!({"op":"app_resource_install","input":f.input}).to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    // The real app contract stringifies wide integers; select its canonical ID,
    // then inspect the same typed state rather than coercing response numbers.
    f.client
        .item(result["id"].as_str().unwrap().to_owned())
        .unwrap()
}
async fn save(
    f: &Fixture,
    item: &str,
    pin: &str,
    base: &crate::AppDocumentDto,
    scene: &str,
    mutation: &str,
) -> R<crate::AppDocumentDto> {
    let result = f
        .client
        .client_request(
            json!({"op":"app_document_save_pinned", "item_id":item,
        "expected_resource_uri":pin, "expected_version":base.version, "expected_sha256":base.sha256,
        "scene_json":scene,"mutation_id":mutation})
            .to_string(),
        )
        .await?;
    serde_json::from_str(&result).map_err(|_| invalid())
}
fn scene(text: &str) -> String {
    json!({"type":"excalidraw","version":2,"elements":[{"id":"label","type":"text","x":0,"y":0,
        "width":200,"height":20,"text":text,"isDeleted":false}],"files":{},"appState":{},"unknown":{"kept":true}}).to_string()
}

#[tokio::test]
async fn canonical_package_card_save_retry_and_cold_reopen_are_one_owned_artifact() {
    let directory = std::env::temp_dir().join(types::new_id("owned_app_test").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("core.sqlite");
    let f = fixture(path.to_str().unwrap());
    let item = install(&f).await;
    let pin = item.app.as_ref().unwrap().resource_uri.clone();
    let base = f.client.app_document_read(item.id.clone()).unwrap();
    assert_eq!(item.version, 1);
    assert_eq!(install(&f).await.id, item.id);
    let resource = f
        .client
        .lock()
        .app_document_resource(&item.id, &pin)
        .unwrap();
    assert_eq!(resource.text, f.html);
    let edited = scene("This is the same artifact.");
    let accepted = save(&f, &item.id, &pin, &base, &edited, "manual_one")
        .await
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(
        save(&f, &item.id, &pin, &base, &edited, "manual_one")
            .await
            .unwrap(),
        accepted
    );
    assert!(
        save(&f, &item.id, &pin, &base, &scene("stale"), "new_stale")
            .await
            .is_err()
    );
    assert_eq!(
        f.client
            .items()
            .iter()
            .filter(|it| it.id == item.id)
            .count(),
        1
    );
    let found = f
        .client
        .lock()
        .find_app_resource(&f.input)
        .unwrap()
        .unwrap();
    assert_eq!(found.id, item.id);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    assert_eq!(client.app_document_read(item.id.clone()).unwrap(), accepted);
    assert_eq!(
        client
            .lock()
            .app_document_resource(&item.id, &pin)
            .unwrap()
            .text,
        resource.text
    );
    drop(client);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn release_update_is_explicit_and_old_renderer_cannot_save_against_new_document_head() {
    let f = fixture(":memory:");
    let item = install(&f).await;
    let old = item.app.unwrap().resource_uri;
    let base = f.client.app_document_read(item.id.clone()).unwrap();
    let text = scene("Accepted before the release update");
    let accepted = save(&f, &item.id, &old, &base, &text, "before_update")
        .await
        .unwrap();
    let html = "<!doctype html><p>Release two</p>";
    f.client
        .file_new_version(
            f.input.package_item_id.clone(),
            html.as_bytes().to_vec(),
            None,
            "New UI release".into(),
        )
        .unwrap();
    assert_eq!(
        f.client
            .lock()
            .app_document_resource(&item.id, &old)
            .unwrap()
            .text,
        f.html
    );
    let admission = Install {
        package_version: 2,
        package_sha256: hash(html.as_bytes()),
        mutation_id: "update_one".into(),
        ..f.input.clone()
    };
    let update = Update {
        admission,
        item_id: item.id.clone(),
        expected_version: accepted.version,
        expected_sha256: accepted.sha256.clone(),
        expected_resource_uri: old.clone(),
    };
    let changed = f.client.lock().update_app_resource(update.clone()).unwrap();
    assert_eq!(changed.scene_json, text);
    assert_eq!(changed.version, accepted.version + 1);
    let pin = f
        .client
        .item(item.id.clone())
        .unwrap()
        .app
        .unwrap()
        .resource_uri;
    assert_ne!(pin, old);
    assert!(f
        .client
        .lock()
        .check_app_resource_pin(&item.id, &old)
        .is_err());
    assert!(save(
        &f,
        &item.id,
        &old,
        &changed,
        &scene("Wrong renderer"),
        "wrong_renderer"
    )
    .await
    .is_err());
    let replay = save(&f, &item.id, &old, &base, &text, "before_update")
        .await
        .unwrap();
    assert_eq!(replay.version, accepted.version);
    assert_eq!(replay.current_version, changed.version);
    let next = save(
        &f,
        &item.id,
        &pin,
        &changed,
        &scene("Fresh renderer edit"),
        "fresh_renderer",
    )
    .await
    .unwrap();
    let recovered = f.client.lock().update_app_resource(update).unwrap();
    assert_eq!(recovered.version, changed.version);
    assert_eq!(recovered.current_version, next.version);
    assert_eq!(recovered.scene_json, text);
    let cold_intent = Install {
        mutation_id: "cold_after_lost_ack_and_release_update".into(),
        ..f.input.clone()
    };
    let found = f
        .client
        .lock()
        .find_app_resource(&cold_intent)
        .unwrap()
        .unwrap();
    assert_eq!(found.id, item.id);
    assert_eq!(found.app.unwrap().resource_uri, pin);
}

#[tokio::test]
async fn admission_rejects_foreign_owner_device_space_hash_mime_and_reused_intent() {
    let f = fixture(":memory:");
    let before = f.client.items().len();
    for change in ["owner", "device", "space", "hash", "version"] {
        let mut input = f.input.clone();
        match change {
            "owner" => input.expected_identity_id = "foreign".into(),
            "device" => input.expected_device_id = "foreign".into(),
            "space" => input.space_id = "foreign".into(),
            "hash" => input.package_sha256 = "0".repeat(64),
            _ => input.package_version = 999,
        }
        assert!(
            f.client.lock().install_app_resource(input).is_err(),
            "{change}"
        );
        assert_eq!(f.client.items().len(), before);
    }
    let plain = f
        .client
        .file_add(
            f.input.space_id.clone(),
            "".into(),
            "plain.html".into(),
            "text/html".into(),
            b"<p>Not an MCP package</p>".to_vec(),
            None,
        )
        .unwrap();
    let mut wrong = f.input.clone();
    wrong.package_item_id = plain.id;
    wrong.package_sha256 = plain.file.unwrap().sha256;
    assert!(f.client.lock().install_app_resource(wrong).is_err());
    let other_space = f
        .client
        .create_group_with("Other Apps".into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut cross_space = f.input.clone();
    cross_space.space_id = other_space;
    // Membership in both Spaces does not authorize a package through the wrong one.
    assert!(f.client.lock().install_app_resource(cross_space).is_err());
    install(&f).await;
    let mut reused = f.input.clone();
    reused.title = "Changed intent".into();
    assert!(f.client.lock().install_app_resource(reused).is_err());
}

#[tokio::test]
async fn owned_resource_survives_source_absence_and_copies_rekey_both_document_and_view() {
    let f = fixture(":memory:");
    let item = install(&f).await;
    let pin = item.app.unwrap().resource_uri;
    let original = f.client.app_document_read(item.id.clone()).unwrap();
    let copied = f
        .client
        .lock()
        .copy_app_document(&item.id, &original.scene_json, "copy_owned", Some(&pin))
        .unwrap();
    let target = f
        .client
        .lock()
        .app_document_resource(&copied.item_id, &pin)
        .unwrap();
    assert_eq!(target.text, f.html);
    assert!(
        f.client.lock().find_app_resource(&f.input).is_err(),
        "two existing cards require a choice, never an automatic installation"
    );
    {
        let mut e = f.client.lock();
        let source = e.item_state(&item.id).unwrap().current().content.clone();
        let copy = e
            .item_state(&copied.item_id)
            .unwrap()
            .current()
            .content
            .clone();
        let (ItemContent::App(source), ItemContent::App(copy)) = (source, copy) else {
            panic!("two owned Apps");
        };
        assert_eq!(
            source.view.as_ref().unwrap().sha256,
            copy.view.as_ref().unwrap().sha256
        );
        assert_ne!(
            source.view.as_ref().unwrap().chunks[0].key,
            copy.view.as_ref().unwrap().chunks[0].key
        );
        // Dependency disappearance: the signed adopted view is still owned by each App.
        e.state.items.remove(&f.input.package_item_id);
        assert_eq!(
            e.app_document_resource(&item.id, &pin).unwrap().text,
            f.html
        );
        let grant = e
            .state
            .app_trust_baseline(&item.id)
            .unwrap()
            .grant
            .id
            .clone();
        e.append(
            &f.input.space_id,
            &f.input.expected_identity_id,
            EventBody::GrantRevoked { grant },
        )
        .unwrap();
        assert!(e.app_document_resource(&item.id, &pin).is_err());
        assert!(e
            .save_app_document(
                &item.id,
                original.version,
                (&original.sha256, Some(&pin)),
                &scene("Refused"),
                "after_revoke"
            )
            .is_err());
        assert!(e
            .copy_app_document(
                &item.id,
                &original.scene_json,
                "copy_after_revoke",
                Some(&pin)
            )
            .is_err());
        assert_eq!(
            e.copy_app_document(&item.id, &original.scene_json, "copy_owned", Some(&pin))
                .unwrap()
                .item_id,
            copied.item_id,
            "an accepted receipt recovers without creating a new copy"
        );
    }
}

#[tokio::test]
async fn owned_renderer_pin_cannot_be_bypassed_by_legacy_writes_or_forged_projection() {
    let f = fixture(":memory:");
    let item = install(&f).await;
    let pin = item.app.unwrap().resource_uri;
    let base = f.client.app_document_read(item.id.clone()).unwrap();
    for op in ["app_document_resource", "app_document_read_pinned"] {
        assert!(f
            .client
            .client_request(
                json!({"op":op,"item_id":item.id,
            "expected_resource_uri":format!("{pin}/forged")})
                .to_string()
            )
            .await
            .is_err());
    }
    assert!(f
        .client
        .lock()
        .app_document_save(
            &item.id,
            base.version,
            &base.sha256,
            &scene("Unpinned"),
            "legacy_write"
        )
        .is_err());
    let e = f.client.lock();
    let content = e.item_state(&item.id).unwrap().current().content.clone();
    let ItemContent::App(mut app) = content else {
        panic!("owned App");
    };
    let owned = app.clone();
    app.view = None;
    app.resource_uri = miniapps::excalidraw::RESOURCE.into();
    let event = |body| {
        e.author_for(&f.input.expected_identity_id)
            .unwrap()
            .sign_event(&f.input.space_id, "pin_admission", 0, None, body)
            .unwrap()
    };
    let wrong_kind = event(EventBody::ItemCreated {
        item: item.id.clone(),
        kind: ItemKind::File,
        content: ItemContent::App(owned.clone()),
        origin: "Not an App".into(),
    });
    assert!(!event_valid(&e.state, &wrong_kind, &e.identities));
    let transmute_file = event(EventBody::ItemVersioned {
        item: f.input.package_item_id.clone(),
        content: ItemContent::App(owned),
        note: "Bypass the App factory".into(),
    });
    assert!(!event_valid(&e.state, &transmute_file, &e.identities));
    let drop_pin = event(EventBody::ItemVersioned {
        item: item.id.clone(),
        content: ItemContent::App(app.clone()),
        note: "Drop the pin".into(),
    });
    assert!(!event_valid(&e.state, &drop_pin, &e.identities));
    let change_type = event(EventBody::ItemVersioned {
        item: item.id.clone(),
        content: ItemContent::File(app.document.unwrap()),
        note: "Change type".into(),
    });
    assert!(!event_valid(&e.state, &change_type, &e.identities));
    assert_eq!(e.app_document_read(&item.id).unwrap(), base);
    // Document Undo within the admitted release remains valid.
    let undo = event(EventBody::ItemReverted {
        item: item.id,
        to_version: 1,
        note: "Undo".into(),
    });
    assert!(event_valid(&e.state, &undo, &e.identities));
}

#[tokio::test]
async fn owned_updates_require_the_actor_profile_and_can_reproject_when_it_arrives() {
    let f = fixture(":memory:");
    let item = install(&f).await;
    let e = f.client.lock();
    let content = e.item_state(&item.id).unwrap().current().content.clone();
    let event = e
        .author_for(&f.input.expected_identity_id)
        .unwrap()
        .sign_event(
            &f.input.space_id,
            "profile_probe",
            0,
            None,
            EventBody::ItemVersioned {
                item: item.id,
                content,
                note: "Authenticated update".into(),
            },
        )
        .unwrap();
    assert!(event_valid(&e.state, &event, &e.identities));
    let mut pending_profile = e.identities.clone();
    pending_profile.remove(&f.input.expected_identity_id);
    assert!(!event_valid(&e.state, &event, &pending_profile));
    assert!(event_valid(&e.state, &event, &e.identities));
}
