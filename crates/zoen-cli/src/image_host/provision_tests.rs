use super::*;
use std::{collections::BTreeMap, path::PathBuf};

struct Home(PathBuf);
impl Home {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let home = std::env::temp_dir().join(types::new_id("image-provision")?);
        std::fs::create_dir(&home).unwrap();
        Ok(Self(home))
    }
    fn vault(&self) -> FileVault {
        FileVault {
            dir: self.0.join("vault"),
        }
    }
    fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(self.0.join("vault"))
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect()
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(home: &Home) -> Config {
    serde_json::from_value(serde_json::json!({
        "namespace":"image-provision-test", "agent":"agent-test", "device":"", "home":home.0,
        "continuity":home.0.join("anchor"), "postgres_key":"pg", "evidence_key":"evidence",
        "fdb_cluster_file":null, "source_cell":"cell", "relay_url":"https://relay.invalid",
        "relay_key":"relay", "key_version":1, "definition_version":"image-v1",
        "image_credential_key":"image", "chat_credential_key":"chat",
        "chat_price": {
            "version":"chat-v1","endpoint":"https://api.openai.com/v1/chat/completions","model":"model",
            "credential_ref":"chat","currency":"USD","scale":6,"input_units_per_million":1,
            "output_units_per_million":1,"max_billable_input_tokens":1,"max_output_tokens":1,
            "max_request_bytes":4096,"max_response_bytes":4096
        },
        "image_price": {
            "version":"image-v1","endpoint":"https://api.openai.com/v1/images/generations",
            "model":"gpt-image-fixture","credential_ref":"images","currency":"USD","scale":6,
            "max_request_bytes":16384,"max_response_bytes":100000,"reservation_units":10,
            "input_text_units_per_million":1,"input_image_units_per_million":1,
            "output_text_units_per_million":1,"output_image_units_per_million":1,
            "image":{"count":1,"format":"png","size":"1024x1024","quality":"low","background":"opaque","max_image_bytes":1000}
        }
    })).unwrap()
}
#[test]
fn cold_retry_retains_the_same_dedicated_signer_code_and_capsule_without_client_state(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let home = Home::new()?;
    let c = config(&home);
    let original = prepare(&c, &home.vault()).unwrap();
    let snapshot = home.snapshot();
    let retry = prepare(&c, &home.vault()).unwrap();
    assert_eq!(retry, original);
    assert_eq!(home.snapshot(), snapshot);
    let device_key =
        RuntimeAuthority::native_device_vault_key(&c.namespace, &c.agent, &original.device)
            .unwrap();
    let key: [u8; 32] = home
        .vault()
        .read_bytes(&device_key)
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(Signer::from_secret(&key).id(), original.device);
    assert!(!home.0.join("zoen.sqlite").exists());
    assert!(!home.snapshot().contains_key("zoen.identity.v1"));
    assert!(!home.snapshot().contains_key("zoen.device.v1"));
    Ok(())
}
#[test]
fn foreign_device_and_malformed_original_never_replace_retained_setup(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let home = Home::new()?;
    let mut c = config(&home);
    prepare(&c, &home.vault()).unwrap();
    let snapshot = home.snapshot();
    c.device = Signer::generate()?.id();
    assert!(prepare(&c, &home.vault()).is_err());
    assert_eq!(home.snapshot(), snapshot);
    c.device.clear();
    let slot = snapshot
        .keys()
        .find(|key| key.starts_with("runtime_enroll_"))
        .unwrap();
    home.vault().write_bytes(slot, b"broken").unwrap();
    let broken = home.snapshot();
    assert!(prepare(&c, &home.vault()).is_err());
    assert_eq!(home.snapshot(), broken);
    Ok(())
}
#[cfg(unix)]
#[test]
fn unreadable_symlink_original_is_not_absence_and_is_not_reseeded(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let home = Home::new()?;
    let c = config(&home);
    prepare(&c, &home.vault()).unwrap();
    let snapshot = home.snapshot();
    let slot = snapshot
        .keys()
        .find(|key| key.starts_with("runtime_enroll_"))
        .unwrap();
    let path = home.vault().dir.join(slot);
    std::fs::remove_file(&path).unwrap();
    let sentinel = home.0.join("sentinel");
    std::fs::write(&sentinel, b"preserve").unwrap();
    std::os::unix::fs::symlink(&sentinel, &path).unwrap();
    assert!(prepare(&c, &home.vault()).is_err());
    assert!(std::fs::symlink_metadata(path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(sentinel).unwrap(), b"preserve");
    Ok(())
}
