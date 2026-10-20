use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use miniapps::deckcraft::{self, registration};
use serde_json::json;

#[test]
fn deckcraft_runtime_profile_gates_catalog_resource_and_native_install_together() {
    let client = seeded();
    let space = find_space(&client, "Paraty com a Marina");
    let listed = client
        .app_specs()
        .into_iter()
        .any(|s| s.id == deckcraft::APP);
    assert_eq!(listed, registration::available());
    if let Ok(expected) = std::env::var("ZOEN_DECKCRAFT_TEST_EXPECT_AVAILABLE") {
        assert_eq!(
            listed,
            expected == "1",
            "the compiled runtime profile must be exercised"
        );
    }
    let resource = client.read_app_resource(deckcraft::RESOURCE.into());
    assert_eq!(resource.is_ok(), listed);
    let install = client.install_app(
        space.id,
        "deckcraft".into(),
        json!({"title":"Deck","mutationId":"create_once"}).to_string(),
    );
    assert_eq!(install.is_ok(), listed);
    if let Ok(resource) = resource {
        assert!(registration::valid_resource(
            &resource.text,
            &resource.manifest_json
        ));
    }
}
#[test]
fn deckcraft_install_has_one_native_owner_and_retries_the_same_immutable_intent() {
    let client = seeded();
    let space = find_space(&client, "Paraty com a Marina");
    let input = json!({"title":"A real presentation","mutationId":"stable_create"}).to_string();
    if !registration::available() {
        assert!(client
            .install_app(space.id, "deckcraft".into(), input)
            .is_err());
        return;
    }
    let first = client
        .install_app(space.id.clone(), "deckcraft".into(), input.clone())
        .unwrap();
    let retry = client
        .install_app(space.id.clone(), "deckcraft".into(), input)
        .unwrap();
    assert_eq!(first.id, retry.id);
    assert_eq!(retry.version, 1);
    let doc = client
        .app_binary_read(first.id.clone(), None, None)
        .unwrap();
    assert_eq!(
        STANDARD.decode(doc.data_base64).unwrap(),
        registration::EMPTY
    );
    assert_eq!(doc.author_id, doc.actor_id);
    assert_eq!(first.app.unwrap().trust, TrustLevelDto::Act);
    let cards=client.timeline(space.id.clone()).unwrap().into_iter().filter(|entry| matches!(&entry.kind,EntryKind::Message{card:Some(c),..} if c.item_id==first.id)).count();
    assert_eq!(cards, 1);
    for changed in [
        json!({"title":"Changed","mutationId":"stable_create"}),
        json!({"title":"Deck"}),
        json!({"title":null,"mutationId":"null_title"}),
        json!({"title":"Deck","mutationId":"x","actorId":"spoof"}),
    ] {
        assert!(client
            .install_app(space.id.clone(), "deckcraft".into(), changed.to_string())
            .is_err());
    }
    assert_eq!(client.item(first.id).unwrap().version, 1);
}
#[test]
fn authorized_existing_agent_creation_attaches_the_same_encrypted_native_seed() {
    let client = seeded();
    let space = find_space(&client, "Paraty com a Marina");
    let agent = zoen(&client).id;
    let result = client.agent_create_app(
        space.id,
        agent.clone(),
        "create_deckcraft".into(),
        "{}".into(),
        "test".into(),
        "create presentation".into(),
    );
    if !registration::available() {
        assert!(result.is_err());
        return;
    }
    let outcome = result.unwrap();
    assert_eq!(outcome.kind, DecisionKind::ActWithUndo);
    let item = outcome.item.unwrap();
    let doc = client.app_binary_read(item.id.clone(), None, None).unwrap();
    assert_eq!(
        STANDARD.decode(doc.data_base64).unwrap(),
        registration::EMPTY
    );
    assert_eq!(doc.author_id, agent);
    assert_eq!(item.app.unwrap().app_id, "deckcraft");
    assert!(client.verify_all().iter().all(|r| r.valid));
}
