use super::*;
use relay_admission::managed_images::{AnchorError, AnchorWriter};
mod runtimes;

pub(crate) struct Home(pub(crate) PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn fixture() -> Result<(Home, Deployment, AnchorWriter), Box<dyn std::error::Error>> {
    let home = Home(std::env::temp_dir().join(types::new_id("managed-relay")?));
    let deployment = Deployment {
        namespace: "test-managed".into(),
        witness: "a".repeat(64),
        source: "test-cell".into(),
        agent: "test-agent".into(),
        device: "b".repeat(64),
    };
    let writer = AnchorWriter::create(&home.0, deployment.clone())?;
    Ok((home, deployment, writer))
}

#[test]
fn registry_is_explicit_unique_and_never_adopts_another_path(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, writer) = fixture()?;
    let registry = ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    assert!(registry.get(&deployment.device).is_some());
    assert!(registry.get(&"c".repeat(64)).is_none());
    assert!(registry.source_matches("test-cell"));
    assert!(!registry.source_matches("replacement-cell"));
    assert!(ManagedDevices::open([
        (deployment.clone(), home.0.clone()),
        (deployment.clone(), home.0.clone())
    ])
    .is_err());
    let changed = Deployment {
        witness: "d".repeat(64),
        ..deployment.clone()
    };
    assert!(ManagedDevices::open([(changed, home.0.clone())]).is_err());
    let missing = home.0.join("caller-supplied-replacement");
    assert!(ManagedDevices::open([(deployment.clone(), missing.clone())]).is_err());
    assert!(
        !missing.exists(),
        "configuration must not bootstrap replacement history"
    );
    drop(writer);
    Ok(())
}

#[test]
fn registered_reader_refuses_missing_or_replaced_anchor_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, writer) = fixture()?;
    let registry = ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    let configured = registry
        .get(&deployment.device)
        .ok_or("missing registered device")?;
    let checkpoint = configured.anchor().checkpoint()?;
    std::fs::rename(
        home.0.join("checkpoint.json"),
        home.0.join("old-checkpoint.json"),
    )?;
    assert!(configured.upload_guard(&checkpoint).is_err());
    std::fs::write(home.0.join("checkpoint.json"), b"{}")?;
    assert!(matches!(
        configured.anchor().checkpoint(),
        Err(AnchorError::Invalid)
    ));
    drop(writer);
    Ok(())
}
