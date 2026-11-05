//! Actual shared-directory/FDB/SQL + production relay binding for both devices.
use super::*;
use crate::native::NativeOutputAuthority;
use crate::tests::retained::image_tasks::delivery::relay::Relay;
use zoen_relay::managed_devices::{ManagedDevices, ManagedPrincipal, ManagedRuntime};

async fn configured(f: &Fixture, second: bool) -> Result<(), Box<dyn std::error::Error>> {
    let managed = f
        .world
        .runtime
        .runtime_continuity
        .as_ref()
        .ok_or("managed")?
        .lock()
        .await;
    let mut principals = vec![ManagedPrincipal {
        agent: f.world.agents[0].identity.clone(),
        device: f.world.agents[0].device.clone().ok_or("device")?,
    }];
    if second {
        principals.push(ManagedPrincipal {
            agent: f.second.identity.clone(),
            device: f.second.device.clone().ok_or("second")?,
        });
    }
    let registry = ManagedDevices::open_with_runtimes(
        [],
        [ManagedRuntime {
            deployment: managed.test_deployment(),
            anchor_directory: f.shared_directory.as_ref().ok_or("directory")?.0.clone(),
            principals,
        }],
    )?;
    f.log.configure_managed_devices(registry)?;
    Ok(())
}
async fn no_effects(
    f: &Fixture,
    before: &relay_admission::managed_images::Checkpoint,
) -> TestResult {
    assert_eq!(&checkpoint(f).await?, before);
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    for query in [
        "SELECT count(*) FROM runtime_attempts",
        "SELECT count(*) FROM runtime_claims",
        "SELECT count(*) FROM runtime_journals",
        "SELECT count(*) FROM runtime_postings",
    ] {
        assert_eq!(f.world.count(query).await, 0);
    }
    Ok(())
}
pub(in crate::tests) async fn run() -> TestResult {
    for configuration in ["legacy", "partial", "complete"] {
        let mut f = Fixture::with_shared_runtime().await?;
        if configuration != "legacy" {
            configured(&f, configuration == "complete").await?;
        }
        let relay = Relay::start(&f).await?;
        let target = relay.target(&f, relay.address);
        let before = checkpoint(&f).await?;
        let scope = native::testing::qualify_shared_output(&f.world.runtime, target.clone()).await;
        if configuration != "complete" {
            assert!(
                matches!(scope, Err(RuntimeError::CoreAuthorityUnavailable)),
                "an actual legacy binding was accepted and must not qualify"
            );
            no_effects(&f, &before).await?;
        } else {
            let scope = scope?;
            scope.check_context(&f.world.runtime)?;
            for actor in [&f.world.agents[0], &f.second] {
                scope.check_device(&actor.identity, actor.device.as_deref().ok_or("device")?)?;
            }
            assert!(scope.check_device(&f.owner.id, &f.owner.id).is_err());
            let wrong_target = crate::ImageDeliveryTarget::configured(
                format!("http://{}", relay.address),
                "another-relay".into(),
                f.world.relay_cell.clone().ok_or("source")?,
            )?;
            assert!(scope.check_target(&wrong_target).is_err());
            no_effects(&f, &before).await?;
            let actor = &f.world.agents[0];
            let device = actor.device.as_deref().ok_or("device")?;
            f.world
                .runtime
                .repack_native_device(&actor.identity, device)
                .await?;
            assert!(checkpoint(&f).await?.sequence > before.sequence);
            let tx = f.world.runtime.execution.transaction().await?;
            scope.check_in(&f.world.runtime.execution, &tx).await?;
            drop(tx);
            // Generation/actual-marker drift refuses under the same route.
            let key = f
                .world
                .runtime
                .execution
                .root
                .pack(&("managed-images/checkpoint/1",));
            let tx = f.world.runtime.execution.transaction().await?;
            let original = tx.get(&key, false).await?.ok_or("checkpoint")?.to_vec();
            let mut altered: relay_admission::managed_images::Checkpoint =
                serde_json::from_slice(&original)?;
            altered.generation = altered.generation.checked_add(1).ok_or("generation")?;
            tx.set(&key, &serde_json::to_vec(&altered)?);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            let tx = f.world.runtime.execution.transaction().await?;
            assert!(scope
                .check_in(&f.world.runtime.execution, &tx)
                .await
                .is_err());
            tx.set(&key, &original);
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            // The operator's shared anchor cannot fall back to a device SQL
            // row, even with identical namespace/witness/checkpoint values.
            let current = checkpoint(&f).await?;
            let actor = f.world.agents[0].clone();
            let device = actor.device.as_deref().ok_or("device")?;
            sqlx::query(
                "UPDATE runtime_image_checkpoint SET profile=2,agent=$1,device=$2 WHERE singleton",
            )
            .bind(&actor.identity)
            .bind(device)
            .execute(&f.world.runtime.finance.pool)
            .await?;
            let expected = f
                .world
                .runtime
                .bound_output_expectation(
                    &relay.state.relay_name,
                    &actor.identity,
                    device,
                    Some(protocol::runtime_binding::ManagedCheckpoint {
                        generation: current.generation,
                        sequence: current.sequence,
                        digest: current.digest.clone(),
                        source_fence: None,
                    }),
                )
                .await?;
            assert!(matches!(
                RuntimeAuthority::connect_output_expectation(
                    &format!("http://{}", relay.address),
                    &relay.state.relay_name,
                    actor,
                    expected
                )
                .await,
                Err(RuntimeError::Denied)
            ));
            sqlx::query("UPDATE runtime_image_checkpoint SET profile=3,agent=NULL,device=NULL WHERE singleton")
                .execute(&f.world.runtime.finance.pool).await?;
            drop(scope);
            // Exercise the public builder without an artificial continuity flag.
            let runtime = Arc::try_unwrap(f.world.runtime).map_err(|_| "exclusive runtime")?;
            f.world.runtime = Arc::new(runtime.qualify_shared_reply_output(target.clone()).await?);
            assert!(f.world.runtime.production_output_scope().is_ok());
            assert!(matches!(
                f.world.runtime.continuity(),
                Err(RuntimeError::RestoreUnreconciled)
            ));
            let stable = checkpoint(&f).await?;
            assert_eq!(
                f.world.runtime.prepare_reply_output("unknown-run").await,
                Err(RuntimeError::RestoreUnreconciled)
            );
            assert_eq!(checkpoint(&f).await?, stable);
            assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
            // A cold reopen does not deserialize or inherit the live route.
            reopen(&mut f).await?;
            assert!(f.world.runtime.production_output_scope().is_err());
        }
        relay.stop().await;
        f.finish().await;
        println!("shared output route configuration={configuration}: actual mandatory relay binding per native device, original checkpoint, no publication/provider/financial authority PASS");
    }
    invalid_sources().await?;
    Ok(())
}
async fn invalid_sources() -> TestResult {
    let f = Fixture::with_shared_runtime().await?;
    configured(&f, true).await?;
    let relay = Relay::start(&f).await?;
    let target = relay.target(&f, relay.address);
    let before = checkpoint(&f).await?;
    for (name, source) in [
        ("wrong-relay", f.world.relay_cell.clone().ok_or("source")?),
        (&relay.state.relay_name, "wrong-source".into()),
    ] {
        let changed = crate::ImageDeliveryTarget::configured(
            format!("http://{}", relay.address),
            name.into(),
            source,
        )?;
        assert!(
            native::testing::qualify_shared_output(&f.world.runtime, changed)
                .await
                .is_err()
        );
        no_effects(&f, &before).await?;
    }
    let key = f.world.runtime.execution.root.pack(&(
        "native-device",
        f.world.agents[0].identity.as_str(),
        f.world.agents[0].device.as_deref().ok_or("device")?,
        "root",
    ));
    let tx = f.world.runtime.execution.transaction().await?;
    let original = tx
        .get(&key, false)
        .await?
        .ok_or("actual capsule root")?
        .to_vec();
    tx.clear(&key);
    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    assert!(
        native::testing::qualify_shared_output(&f.world.runtime, target.clone())
            .await
            .is_err(),
        "matching marker rows do not replace the actual committed capsule"
    );
    let tx = f.world.runtime.execution.transaction().await?;
    assert!(tx.get(&key, false).await?.is_none());
    tx.set(&key, &original);
    tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    let witness: String =
        sqlx::query_scalar("SELECT witness FROM runtime_deployment_binding WHERE singleton")
            .fetch_one(&f.world.runtime.finance.pool)
            .await?;
    let changed = sqlx::query("UPDATE runtime_deployment_binding SET witness=$1 WHERE singleton")
        .bind("f".repeat(64))
        .execute(&f.world.runtime.finance.pool)
        .await;
    assert!(matches!(changed, Err(sqlx::Error::Database(ref error)) if error.code().as_deref() == Some("23514")),
        "the original SQL pairing is immutable; do not disable its trigger to manufacture a test state");
    let retained: String =
        sqlx::query_scalar("SELECT witness FROM runtime_deployment_binding WHERE singleton")
            .fetch_one(&f.world.runtime.finance.pool)
            .await?;
    assert_eq!(retained, witness);
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(f.second.device.as_deref().ok_or("second")?)
        .execute(&f.world.runtime.finance.pool)
        .await?;
    assert!(
        native::testing::qualify_shared_output(&f.world.runtime, target)
            .await
            .is_err(),
        "every configured principal must have a current certified SQL directory entry"
    );
    no_effects(&f, &before).await?;
    relay.stop().await;
    f.finish().await;
    println!("shared output route invalid identity/source/pairing/revoked device or missing actual capsule: refusal without metadata repair/provider/financial mutations PASS");
    Ok(())
}
