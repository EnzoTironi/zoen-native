use super::*;
use crate::managed_devices::tests::fixture;
use serde_json::json;

fn write(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    Ok(())
}
#[test]
fn live_addition_is_shared_and_cannot_forget_or_reassign_admitted_devices(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, _writer) = fixture()?;
    let path = home.0.join("live.json");
    write(&path, &json!({"live":true,"devices":[]}))?;
    let registry = ManagedDevices::load_operator_file(Some(&path))?;
    let clone = registry.clone();
    assert!(!registry.is_empty());
    assert!(registry.source_matches(&deployment.source));
    let entry = json!({"deployment":deployment,"anchor_directory":home.0});
    write(&path, &json!({"live":true,"devices":[entry.clone()]}))?;
    assert!(clone.get(&deployment.device).is_some());
    assert!(registry.get(&deployment.device).is_some());
    write(&path, &json!({"live":true,"devices":[]}))?;
    assert!(registry.get(&"f".repeat(64)).is_none());
    assert!(clone.get(&deployment.device).is_some());
    let mut changed = entry.clone();
    changed["deployment"]["agent"] = "other".into();
    write(&path, &json!({"live":true,"devices":[changed]}))?;
    assert!(registry.get(&"e".repeat(64)).is_none());
    assert_eq!(
        registry
            .get(&deployment.device)
            .ok_or("retained")?
            .deployment()
            .agent,
        deployment.agent
    );
    write(&path, &json!({"live":false,"devices":[entry]}))?;
    assert!(registry.get(&"e".repeat(64)).is_none());
    assert!(clone.get(&deployment.device).is_some());
    std::fs::write(&path, b"corrupt")?;
    assert!(registry.get(&"e".repeat(64)).is_none());
    assert!(clone.get(&deployment.device).is_some());
    Ok(())
}
#[test]
fn live_config_cannot_introduce_another_source_cell() -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, _writer) = fixture()?;
    let path = home.0.join("live.json");
    write(&path, &json!({"live":true,"devices":[]}))?;
    let registry = ManagedDevices::load_operator_file(Some(&path))?;
    assert!(registry.source_matches("other-source"));
    write(
        &path,
        &json!({"live":true,"devices":[{"deployment":deployment,"anchor_directory":home.0}]}),
    )?;
    assert!(registry.get(&deployment.device).is_none());
    Ok(())
}
