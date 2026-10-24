use super::*;
use std::{collections::HashMap, sync::Mutex};

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

#[tokio::test]
async fn account_commands_require_the_injected_vault_and_never_expose_its_keys() {
    let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
    let request =
        serde_json::json!({"op":"link_request","relay_url":"http://127.0.0.1:9000"}).to_string();
    assert!(
        core.client_request(request.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("protected native vault")
    );
    let vault = Arc::new(Vault::default());
    let result: serde_json::Value = serde_json::from_str(
        &core
            .client_vault_request(request, vault.clone())
            .await
            .unwrap(),
    )
    .unwrap();
    let code = result["code"].as_str().unwrap();
    assert_eq!(result["check"], core.link_code_check(code.into()).unwrap());
    assert_eq!(result.as_object().unwrap().len(), 2);
    {
        let keys = vault.0.lock().unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains_key(crate::link::VAULT_PENDING));
    }
    let invalid = serde_json::json!({"op":"backup_password","password":"keep-this-secret","vault_key":"arbitrary"}).to_string();
    let error = core
        .client_vault_request(invalid, vault.clone())
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("keep-this-secret"));
    assert_eq!(vault.0.lock().unwrap().len(), 1);
    assert!(
        core.client_vault_request("x".repeat(MAX_REQUEST_BYTES + 1), vault)
            .await
            .is_err()
    );
}

#[test]
fn transfer_waits_are_bounded_before_network_work() {
    assert!(transfer_timeout(999).is_err());
    assert_eq!(transfer_timeout(1_000).unwrap(), 1_000);
    assert_eq!(transfer_timeout(120_000).unwrap(), 120_000);
    assert!(transfer_timeout(120_001).is_err());
}

#[test]
fn commands_refuse_unknown_fields_and_unsafe_cent_numbers() {
    assert!(
        serde_json::from_str::<Command>(r#"{"op":"snapshot","database":"/private/keys"}"#).is_err()
    );
    assert!(
        serde_json::from_str::<Command>(
            r#"{"op":"raise_budget","agent_id":"a","extra_cents":9007199254740993}"#
        )
        .is_err()
    );
    assert_eq!(cents("9223372036854775807".into()).unwrap(), i64::MAX);
    assert_eq!(cents("-9223372036854775808".into()).unwrap(), i64::MIN);
    assert!(cents("9223372036854775808".into()).is_err());
    assert!(cents(" 1".into()).is_err());
}

#[tokio::test]
async fn application_boundary_reads_and_mutates_the_real_engine() {
    let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
    core.seed_demo_if_empty().unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(
        &core
            .client_request(r#"{"op":"snapshot"}"#.into())
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!snapshot["conversations"].as_array().unwrap().is_empty());
    let space = snapshot["conversations"][0]["id"].as_str().unwrap();
    let request = serde_json::json!({ "op": "send_message", "space_id": space, "text": "Olá do cliente compartilhado 🪴" });
    let sent: serde_json::Value =
        serde_json::from_str(&core.client_request(request.to_string()).await.unwrap()).unwrap();
    assert!(sent["seq"].is_string());
    let entries = core.timeline(space.into()).unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.id == sent["id"].as_str().unwrap())
    );
    assert!(
        core.client_request(r#"{"op":"timeline","space_id":"missing","limit":0}"#.into())
            .await
            .is_err()
    );
    assert!(
        core.client_request("x".repeat(MAX_REQUEST_BYTES + 1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn main_timeline_excludes_thread_replies_but_exact_message_lookup_keeps_them() {
    let core = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
    core.seed_demo_if_empty().unwrap();
    let space = core.spaces()[0].id.clone();
    let root = core
        .send_message(space.clone(), "Mensagem original".into())
        .unwrap();
    let reply = core
        .send_reply(space.clone(), "Só na thread".into(), root.id.clone(), true)
        .unwrap();
    let timeline: serde_json::Value = serde_json::from_str(
        &core
            .client_request(serde_json::json!({"op":"timeline", "space_id":space}).to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        timeline["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == root.id)
    );
    assert!(
        !timeline["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == reply.id)
    );
    let exact: serde_json::Value = serde_json::from_str(
        &core
            .client_request(
                serde_json::json!({"op":"message", "space_id":space, "message_id":reply.id})
                    .to_string(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(exact["in_thread"], root.id);
}

#[test]
fn native_editor_markdown_roundtrips_unicode_marks_and_block_structure() {
    for markdown in [
        "## Ação **junta** 🪴",
        "- [x] Conferir *amanhã*",
        "> Uma [referência](https://example.com)",
        "```rust\nfn main() {}\n```",
        "Antes `do código` e ~~depois~~",
    ] {
        let blocks = crate::pages::parse_editor_markdown(markdown).unwrap();
        assert!(!blocks.is_empty());
        for block in blocks {
            let encoded = crate::pages::editor_block_markdown(&block).unwrap();
            let decoded = crate::pages::parse_editor_markdown(&encoded).unwrap();
            assert_eq!(decoded.len(), 1, "{encoded}");
            let mut restored = decoded.into_iter().next().unwrap();
            restored.id = block.id.clone();
            assert_eq!(restored, block, "{encoded}");
        }
    }
}
