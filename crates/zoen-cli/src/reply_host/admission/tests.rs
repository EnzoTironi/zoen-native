use super::*;

fn deployment(device: &str) -> Deployment {
    Deployment {
        namespace: "runtime".into(),
        witness: "a".repeat(64),
        source: "cell".into(),
        agent: "agent".into(),
        device: device.repeat(64),
    }
}
#[test]
fn full_registry_accepts_exact_recovery_but_not_a_new_principal() -> Result<(), String> {
    let mut config = OperatorConfig {
        live: true,
        ..Default::default()
    };
    let anchor = Path::new("/private/continuity");
    for number in 0..256 {
        let mut entry = deployment("b");
        entry.device = format!("{number:064x}");
        assert!(append(&mut config, entry, anchor)?);
    }
    let mut existing = deployment("b");
    existing.device = format!("{:064x}", 0);
    assert!(!append(&mut config, existing, anchor)?);
    assert!(append(&mut config, deployment("c"), anchor).is_err());
    assert_eq!(config.runtimes[0].principals.len(), 256);
    Ok(())
}
#[test]
fn append_retains_exact_retries_and_refuses_reassignment_and_anchor_changes() -> Result<(), String>
{
    let mut config = OperatorConfig {
        live: true,
        ..OperatorConfig::default()
    };
    let anchor = Path::new("/private/continuity");
    assert!(append(&mut config, deployment("b"), anchor)?);
    assert!(!append(&mut config, deployment("b"), anchor)?);
    assert!(append(&mut config, deployment("c"), anchor)?);
    assert_eq!(config.runtimes.len(), 1);
    assert_eq!(config.runtimes[0].principals.len(), 2);
    let before = serde_json::to_vec(&config).map_err(|_| "json")?;
    let mut changed = deployment("b");
    changed.agent = "other".into();
    assert!(append(&mut config, changed, anchor).is_err());
    assert!(append(&mut config, deployment("d"), Path::new("/other/continuity")).is_err());
    config.live = false;
    assert!(append(&mut config, deployment("d"), anchor).is_err());
    config.live = true;
    assert_eq!(serde_json::to_vec(&config).map_err(|_| "json")?, before);
    Ok(())
}
#[test]
fn private_atomic_registration_is_idempotent_and_rejects_symlink_and_public_files(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::temp_dir().join(types::new_id("live-admission")?);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&home)?;
    let vault = FileVault { dir: home.clone() };
    vault.write_bytes(
        "registry.json",
        &serde_json::to_vec(&OperatorConfig {
            live: true,
            ..Default::default()
        })?,
    )?;
    let path = home.join("registry.json");
    register(&path, deployment("b"), Path::new("/private/continuity"))?;
    let original = std::fs::read(&path)?;
    register(&path, deployment("b"), Path::new("/private/continuity"))?;
    assert_eq!(std::fs::read(&path)?, original);
    register(&path, deployment("c"), Path::new("/private/continuity"))?;
    let config: OperatorConfig = read_private_limit(&path, 65536)?;
    assert_eq!(config.runtimes[0].principals.len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let link = home.join("link.json");
        symlink(&path, &link)?;
        assert!(register(&link, deployment("d"), Path::new("/private/continuity")).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        assert!(register(&path, deployment("d"), Path::new("/private/continuity")).is_err());
    }
    std::fs::remove_dir_all(home)?;
    Ok(())
}
