//! Public normal constructor with real directory/vault and shared anchor.
use super::*;
use client_core::SecretVault;
use vault::Vault;
type TestResult = Result<(), Box<dyn std::error::Error>>;
async fn open(w: &World, vault: Arc<Vault>) -> Result<RuntimeAuthority, RuntimeError> {
    let actor = &w.agents[0];
    let mut cfg = config(&w.db_url, &w.config_namespace, &w.http.base);
    cfg.relay_cell = w.relay_cell.clone();
    RuntimeAuthority::open_with_native_device(
        cfg,
        vault,
        &actor.identity,
        actor
            .device
            .as_deref()
            .ok_or(RuntimeError::InvalidBinding)?,
        1,
    )
    .await
}
pub(super) async fn run() -> TestResult {
    for fault in [0, 90, 91, 92] {
        let w = World::without_policy().await?;
        let actor = &w.agents[0];
        let device = actor.device.as_deref().ok_or("fixture device")?;
        let vault = Vault::new()?;
        let capsule_key = RuntimeAuthority::native_capsule_vault_key(&w.config_namespace, 1)?;
        assert!(vault.save(capsule_key.clone(), vec![43; 32]));
        assert!(vault.save(
            RuntimeAuthority::native_device_vault_key(
                &w.config_namespace,
                &actor.identity,
                device
            )?,
            actor.key.secret().to_vec()
        ));
        let home = managed_images::Directory::new()?;
        let runtime = open(&w, vault.clone())
            .await?
            .initialize_runtime_continuity(&home.0)
            .await?;
        runtime.install_policy(w.signed.clone()).await?;
        runtime.fault.store(fault, Ordering::SeqCst);
        eprintln!("shared native public fault {fault}: phase=initialize");
        let result = runtime
            .initialize_native_device(&actor.identity, device)
            .await;
        runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert_eq!(result?.generation, 1);
        } else {
            assert!(matches!(result, Err(RuntimeError::Unavailable)));
        }
        runtime.finance.pool.close().await;
        drop(runtime);
        if fault == 91 {
            vault.delete(capsule_key.clone());
            assert!(matches!(
                open(&w, vault.clone()).await,
                Err(RuntimeError::CoreAuthorityUnavailable)
            ));
            assert!(vault.save(capsule_key, vec![43; 32]));
        }
        eprintln!("shared native public fault {fault}: phase=reopen");
        let runtime = open(&w, vault.clone())
            .await?
            .open_runtime_continuity(&home.0)
            .await?;
        assert_eq!(
            runtime
                .initialize_native_device(&actor.identity, device)
                .await?
                .generation,
            1
        );
        eprintln!("shared native public fault {fault}: phase=packages");
        let packages = runtime
            .publish_native_key_packages(&actor.identity, device)
            .await?;
        assert!(packages.published);
        assert_eq!(
            runtime
                .publish_native_key_packages(&actor.identity, device)
                .await?,
            packages
        );
        let before = runtime
            .inspect_native_device(&actor.identity, device)
            .await?;
        let packed = runtime
            .repack_native_device(&actor.identity, device)
            .await?;
        assert_eq!(packed.generation, before.generation + 1);
        assert_eq!(packed.image_bytes, before.image_bytes);
        runtime.check_shared_runtime().await?;
        // Current unconfigured writers are refused before staging or resetting
        // a native device in this shared namespace.
        let legacy = open(&w, vault.clone()).await?;
        assert_eq!(
            legacy
                .prepare_native_key_packages(&actor.identity, device)
                .await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        legacy.finance.pool.close().await;
        drop(legacy);
        assert_eq!(
            runtime
                .inspect_native_device(&actor.identity, device)
                .await?,
            packed
        );
        assert_eq!(runtime.continuity(), Err(RuntimeError::RestoreUnreconciled));
        assert!(runtime.production_output_scope().is_err());
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
        runtime.finance.pool.close().await;
        drop(runtime);
        w.finish().await;
        println!("public shared native setup fault {fault}: real file-vault/certified directory, exact bootstrap/key-package/repack recovery, missing key refused, unconfigured current writer refused before staging, no paid/output authority PASS");
    }
    Ok(())
}
