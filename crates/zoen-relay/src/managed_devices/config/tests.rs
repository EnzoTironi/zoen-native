use super::*;
use std::io::Write;

fn write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[test]
fn absent_operator_config_preserves_ordinary_startup() -> anyhow::Result<()> {
    assert!(ManagedDevices::load_operator_file(None)?.is_empty());
    Ok(())
}

#[test]
fn private_config_opens_existing_anchor_once_and_rejects_unknown_duplicate_or_oversized_input(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, writer) = crate::managed_devices::tests::fixture()?;
    let path = home.0.join("operator.json");
    let entry = serde_json::json!({"deployment":deployment,"anchor_directory":home.0});
    write(
        &path,
        &serde_json::to_vec(&serde_json::json!({"devices":[entry.clone()]}))?,
    )?;
    let registry = ManagedDevices::load_operator_file(Some(&path))?;
    assert!(registry.get(&deployment.device).is_some());
    // Altering the file after startup does not replace the live OnceLock input.
    write(&path, b"{not valid}")?;
    assert!(registry.get(&deployment.device).is_some());
    assert!(ManagedDevices::load_operator_file(Some(&path)).is_err());
    for invalid_config in [
        serde_json::json!({"devices":[entry.clone(),entry]}),
        serde_json::json!({"devices":[],"caller_path":"do not adopt"}),
        serde_json::json!({"devices":[{"deployment":deployment,"anchor_directory":"relative"}]}),
    ] {
        write(&path, &serde_json::to_vec(&invalid_config)?)?;
        let error = ManagedDevices::load_operator_file(Some(&path))
            .err()
            .ok_or("expected refusal")?;
        assert!(!error.to_string().contains(&deployment.witness));
        assert!(!error.to_string().contains("relative"));
    }
    write(&path, &vec![b' '; 65_537])?;
    assert!(ManagedDevices::load_operator_file(Some(&path)).is_err());
    assert!(ManagedDevices::load_operator_file(Some(Path::new("relative.json"))).is_err());
    drop(writer);
    Ok(())
}

#[cfg(unix)]
#[test]
fn public_or_symlinked_operator_config_is_not_adopted() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (home, _deployment, writer) = crate::managed_devices::tests::fixture()?;
    let path = home.0.join("operator.json");
    write(&path, br#"{"devices":[]}"#)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    assert!(ManagedDevices::load_operator_file(Some(&path)).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let link = home.0.join("alias.json");
    symlink(&path, &link)?;
    assert!(ManagedDevices::load_operator_file(Some(&link)).is_err());
    drop(writer);
    Ok(())
}

#[test]
fn operator_config_loads_one_runtime_anchor_for_multiple_explicit_principals(
) -> Result<(), Box<dyn std::error::Error>> {
    use relay_admission::managed_images::{AnchorWriter, RuntimeDeployment};
    let home = crate::managed_devices::tests::Home(
        std::env::temp_dir().join(types::new_id("runtime-operator")?),
    );
    let deployment = RuntimeDeployment {
        namespace: "shared-config".into(),
        witness: "a".repeat(64),
        source: "test-cell".into(),
    };
    let _writer = AnchorWriter::create_runtime(&home.0, deployment.clone())?;
    let path = home.0.join("operator.json");
    let entry = serde_json::json!({
        "deployment": deployment,
        "anchor_directory": home.0,
        "principals": [
            {"agent":"first-agent", "device":"b".repeat(64)},
            {"agent":"second-agent", "device":"c".repeat(64)}
        ]
    });
    write(
        &path,
        &serde_json::to_vec(&serde_json::json!({"devices":[], "runtimes":[entry.clone()]}))?,
    )?;
    let registry = ManagedDevices::load_operator_file(Some(&path))?;
    for device in ["b".repeat(64), "c".repeat(64)] {
        assert!(registry.get(&device).is_some());
    }
    for invalid_entry in [
        serde_json::json!({"deployment":deployment,"anchor_directory":home.0,"principals":[]}),
        serde_json::json!({"deployment":deployment,"anchor_directory":home.0,"principals":[{"agent":"first-agent","device":"b".repeat(64),"grant":true}]}),
    ] {
        write(
            &path,
            &serde_json::to_vec(&serde_json::json!({"devices":[], "runtimes":[invalid_entry]}))?,
        )?;
        assert!(ManagedDevices::load_operator_file(Some(&path)).is_err());
    }
    write(
        &path,
        &serde_json::to_vec(&serde_json::json!({"devices":[], "runtimes":[entry.clone(),entry]}))?,
    )?;
    assert!(ManagedDevices::load_operator_file(Some(&path)).is_err());
    assert!(
        registry.get(&"b".repeat(64)).is_some(),
        "a rewritten config cannot replace the live registry"
    );
    Ok(())
}
