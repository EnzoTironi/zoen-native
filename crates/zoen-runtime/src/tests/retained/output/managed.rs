//! Managed text activation, using the same native/SQL/FDB/external-anchor
//! recovery as images. Fixture continuity is not production paid admission.
use super::*;

pub(in crate::tests) async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    for fault in [0, 67, 43] {
        let mut fixture = Fixture::with_managed_output().await?;
        fixture.grant("managed-text-owner-trust").await?;
        fixture.message(ORIGINAL).await?;
        let run = fixture.sync(false).await?.runs.remove(0);
        let actor = fixture.world.agents[0].clone();
        let device = actor.device.as_deref().ok_or("fixture device")?;
        fixture.world.runtime.run_model(&run.run).await?;
        assert_one_bill(&fixture).await;
        let generation =
            native::testing::generation(&fixture.world.runtime, &actor.identity, device).await;
        fixture.world.runtime.fault.store(fault, Ordering::SeqCst);
        let prepared = proof::prepare(&fixture.world.runtime, &run.run).await;
        fixture.world.runtime.fault.store(0, Ordering::SeqCst);
        if fault == 0 {
            assert!(prepared.is_ok());
        } else {
            assert_eq!(prepared, Err(RuntimeError::Unavailable));
        }
        if fault == 67 {
            let runtime = &fixture.world.runtime;
            assert_eq!(
                runtime
                    .native
                    .as_ref()
                    .ok_or("native custody")?
                    .prepare_output(runtime, &run.run, &super::authority::RefusedOutput)
                    .await,
                Err(RuntimeError::Denied)
            );
            let pending = fixture.world.runtime.pending_image_activation().await;
            assert_eq!(
                native::testing::generation(&fixture.world.runtime, &actor.identity, device).await,
                generation
            );
            fixture
                .world
                .runtime
                .refuse_unproven_image_activation()
                .await;
            fixture
                .message("Conversation advanced while text activation was offline.")
                .await?;
            cold(&mut fixture).await;
            fixture
                .world
                .runtime
                .assert_image_activation_recovered(&pending)
                .await;
            assert_eq!(
                proof::publication(&fixture.world.runtime, &run.run).await,
                pending.envelope
            );
        } else {
            cold(&mut fixture).await;
        }
        let output = proof::prepare(&fixture.world.runtime, &run.run).await?;
        let envelope = proof::publication(&fixture.world.runtime, &run.run).await;
        let checkpoint = proof::checkpoint(&fixture.world.runtime, &run.run)
            .await?
            .ok_or("managed text checkpoint missing")?;
        assert_eq!((checkpoint.generation, checkpoint.sequence), (1, 1));
        crate::tests::managed_images::assert_publish_checkpoint(
            &fixture.world.runtime,
            fixture
                .managed_directory
                .as_ref()
                .ok_or("managed directory")?,
            &envelope,
            &checkpoint,
        )
        .await;
        assert_eq!(
            proof::prepare(&fixture.world.runtime, &run.run).await?,
            output
        );
        assert_one_bill(&fixture).await;
        let original = proof::legacy_checkpoint_record(&fixture.world.runtime, &run.run).await?;
        assert_eq!(
            proof::prepare(&fixture.world.runtime, &run.run).await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        assert_eq!(
            proof::receive(&fixture.world.runtime, &run.run).await,
            Err(RuntimeError::RestoreUnreconciled)
        );
        proof::restore_publication_record(&fixture.world.runtime, &run.run, &original).await?;
        assert_one_bill(&fixture).await;
        crate::tests::managed_images::matching_checkpoint_rollback(
            &fixture.world.runtime,
            fixture
                .managed_directory
                .as_ref()
                .ok_or("managed directory")?,
            &actor.identity,
            device,
        )
        .await;
        cold(&mut fixture).await;
        assert_eq!(
            proof::prepare(&fixture.world.runtime, &run.run).await?,
            output
        );
        assert_one_bill(&fixture).await;
        assert!(matches!(
            fixture.world.runtime.prepare_reply_output(&run.run).await,
            Err(RuntimeError::CoreAuthorityUnavailable)
        ));
        println!("managed text activation fault {fault}: same native publication/capsule/reference and checkpoint recover after cold reopen, one original bill, production guard closed PASS");
        fixture.world.finish().await;
    }
    Ok(())
}
