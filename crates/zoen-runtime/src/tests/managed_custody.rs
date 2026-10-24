//! The public constructor, using a fresh file-backed fixture vault and the real
//! SQL directory/FDB capsule primitives. This is not deployed KMS qualification.
use super::vault::Vault;
use super::*;
use client_core::SecretVault;

async fn open(
    w: &World,
    vault: Arc<Vault>,
    actor: &Author,
) -> Result<RuntimeAuthority, RuntimeError> {
    RuntimeAuthority::open_with_native_device(
        config(&w.db_url, &w.config_namespace, &w.http.base),
        vault,
        &actor.identity,
        actor.device.as_deref().unwrap(),
        1,
    )
    .await
}

pub(super) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await?;
    let actor = w.agents[0].clone();
    let agent = &actor.identity;
    let device = actor.device.as_deref().unwrap();
    let vault = Vault::new()?;
    let capsule_key = RuntimeAuthority::native_capsule_vault_key(&w.config_namespace, 1).unwrap();
    let device_key =
        RuntimeAuthority::native_device_vault_key(&w.config_namespace, agent, device).unwrap();
    // Only this test provisions fresh fixture secrets. The public constructor
    // does not call save/delete, generate a key, or accept profiles/key maps.
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    assert!(vault.save(capsule_key.clone(), vec![43; 32]));
    assert!(vault.save(device_key.clone(), vec![9; 31]));
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::InvalidBinding)
    ));
    assert!(vault.save(device_key.clone(), Signer::generate()?.secret().to_vec()));
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::Denied)
    ));
    assert!(vault.save(device_key.clone(), actor.key.secret().to_vec()));
    let foreign_namespace = types::new_id("foreign-custody")?;
    assert!(matches!(
        RuntimeAuthority::open_with_native_device(
            config(&w.db_url, &foreign_namespace, &w.http.base),
            vault.clone(),
            agent,
            device,
            1,
        )
        .await,
        Err(RuntimeError::DeploymentMismatch)
    ));
    let runtime = Arc::new(open(&w, vault.clone(), &actor).await.unwrap());
    assert_eq!(runtime.continuity(), Err(RuntimeError::RestoreUnreconciled));
    assert_ne!(
        RuntimeAuthority::native_capsule_vault_key("other-namespace", 1).unwrap(),
        capsule_key
    );
    assert_ne!(
        RuntimeAuthority::native_device_vault_key("other-namespace", agent, device).unwrap(),
        device_key
    );
    assert_eq!(
        runtime
            .inspect_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    // Distinct workers contend on the actual FDB device fence. A loser must
    // not interpret a conflict/uncertain commit as an absent capsule.
    let contender = open(&w, vault.clone(), &actor).await.unwrap();
    let (one, two) = tokio::join!(
        runtime.initialize_native_device(agent, device),
        contender.initialize_native_device(agent, device)
    );
    let first = match (one, two) {
        (Ok(first), Ok(second)) => {
            assert_eq!(first, second);
            first
        }
        (Ok(first), Err(error)) | (Err(error), Ok(first)) => {
            assert!(matches!(
                error,
                RuntimeError::Denied | RuntimeError::Unavailable
            ));
            first
        }
        (Err(one), Err(two)) => panic!("neither initializer succeeded: {one:?}, {two:?}"),
    };
    contender.finance.pool.close().await;
    drop(contender);
    assert_eq!(first.generation, 1);
    assert!(first.image_bytes > 512);
    // Simulate an accepted result whose caller lost the acknowledgement.
    assert_eq!(
        runtime
            .initialize_native_device(agent, device)
            .await
            .unwrap(),
        first
    );
    assert_eq!(
        runtime.inspect_native_device(agent, device).await.unwrap(),
        first
    );
    assert_eq!(
        runtime.prepare_reply_output("not-a-run").await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    assert_eq!(
        runtime.run_model("not-a-run").await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert_eq!(
        runtime.cancel_model("not-a-run").await.unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    let packed = runtime.repack_native_device(agent, device).await.unwrap();
    assert_eq!(packed.generation, 2);
    assert_eq!(packed.image_bytes, first.image_bytes);
    runtime.finance.pool.close().await;
    drop(runtime);
    // New RuntimeAuthority, loaded from file-backed vault, SQL and sealed FDB;
    // no testing::custody setter or fixture continuity flag reaches this worker.
    let reopened = Arc::new(open(&w, vault.clone(), &actor).await.unwrap());
    assert_eq!(
        reopened.inspect_native_device(agent, device).await.unwrap(),
        packed
    );
    assert_eq!(
        reopened
            .initialize_native_device(agent, device)
            .await
            .unwrap(),
        packed
    );
    assert_eq!(
        reopened.continuity(),
        Err(RuntimeError::RestoreUnreconciled)
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    // Only this fresh runtime namespace is inspected, never another deployment.
    let trx = reopened.execution.transaction().await.unwrap();
    let mut range = foundationdb::RangeOption::from(reopened.execution.root.range());
    range.limit = Some(512);
    let values = trx.get_range(&range, 1, false).await.unwrap();
    assert!(!values.is_empty());
    for value in values.iter() {
        assert!(!value
            .value()
            .windows(15)
            .any(|bytes| bytes == b"SQLite format 3"));
        assert!(!value
            .value()
            .windows(32)
            .any(|bytes| bytes == actor.key.secret()));
        assert!(!value.value().windows(32).any(|bytes| bytes == [43; 32]));
    }
    drop(trx);
    // A missing or changed retained key cannot silently reset the capsule.
    vault.delete(capsule_key.clone());
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    assert!(vault.save(capsule_key.clone(), vec![44; 32]));
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::InvalidBinding)
    ));
    assert!(vault.save(capsule_key, vec![43; 32]));
    let restored = open(&w, vault.clone(), &actor).await.unwrap();
    assert_eq!(
        restored.inspect_native_device(agent, device).await.unwrap(),
        packed
    );
    // A locator is not permission to attach another registered device.
    let other = &w.agents[1];
    assert!(matches!(
        open(&w, vault.clone(), other).await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    assert!(matches!(
        reopened
            .initialize_native_device(&other.identity, other.device.as_deref().unwrap())
            .await,
        Err(RuntimeError::CoreAuthorityUnavailable)
    ));
    let mut tx = w.runtime.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(&mut tx, agent, device)
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert!(matches!(
        open(&w, vault.clone(), &actor).await,
        Err(RuntimeError::Denied)
    ));
    assert_eq!(
        reopened
            .inspect_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        reopened
            .initialize_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        reopened
            .repack_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    restored.finance.pool.close().await;
    reopened.finance.pool.close().await;
    drop(restored);
    drop(reopened);
    drop(vault);
    w.finish().await;
    println!("managed native constructor: real directory and fresh file-vault, empty sealed capsule, lost-ACK/cold retry, wrong/missing key and revoked device PASS; continuity/model/output remain closed");
    Ok(())
}
