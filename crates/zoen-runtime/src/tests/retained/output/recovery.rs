//! Original capture association, lost acknowledgement and cold native replay.
use super::*;

pub(super) async fn accepted_across_prefix_revocation_and_cold_replay(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut fixture, run) = original().await?;
    let runtime = &fixture.world.runtime;
    assert_eq!(
        runtime.prepare_reply_output(&run.run).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    assert_eq!(
        runtime.reconcile_reply_receipt(&run.run).await.unwrap_err(),
        RuntimeError::CoreAuthorityUnavailable
    );
    runtime.run_model(&run.run).await.unwrap();
    let original = proof::fingerprints(runtime, &run.run).await;
    runtime.fault.store(43, Ordering::SeqCst);
    assert_eq!(
        proof::prepare(runtime, &run.run).await.unwrap_err(),
        RuntimeError::Unavailable
    );
    runtime.fault.store(0, Ordering::SeqCst);
    let first = proof::prepare(runtime, &run.run).await.unwrap();
    let env = proof::publication(runtime, &run.run).await;
    assert_eq!(first.client_id, env.client_id());
    proof::metadata_bounds(runtime, &run.run).await;
    let fingerprints = proof::fingerprints(runtime, &run.run).await;
    assert_eq!(
        (&fingerprints.0, &fingerprints.1, &fingerprints.2),
        (&original.0, &original.1, &original.2)
    );
    assert_eq!(
        proof::receive(runtime, &run.run).await.unwrap(),
        ReplyReceiptProgress::Waiting
    );
    cold(&mut fixture).await;
    assert_eq!(
        proof::prepare(&fixture.world.runtime, &run.run)
            .await
            .unwrap(),
        first
    );
    for _ in 0..65 {
        fixture
            .clear(EventBody::DeviceJoining {
                device: fixture.world.owner.device.clone().unwrap(),
            })
            .await?;
    }
    let accepted = fixture.append(env).await;
    fixture
        .encrypted(EventBody::GrantRevoked {
            grant: "output-owner-trust".into(),
        })
        .await?;
    revoke_device(&fixture).await;
    // Observe the real FDB timeout without changing the execution deadline or
    // the paired receipt leases, then exercise native work beyond that deadline.
    let bounded = fixture.world.runtime.execution.transaction().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(2250)).await;
    let timeout = bounded
        .get(
            &fixture
                .world
                .runtime
                .execution
                .root
                .pack(&("receipt-read-deadline-probe",)),
            false,
        )
        .await
        .err()
        .expect("bounded transaction must expire before the probe");
    assert_eq!(timeout.code(), 1031);
    drop(bounded);
    eprintln!("native receipt real bounded transaction timeout: FDB code1031 PASS");
    fixture.world.runtime.fault.store(55, Ordering::SeqCst);
    let ReplyReceiptProgress::CatchingUp {
        through,
        generation,
    } = proof::receive(&fixture.world.runtime, &run.run)
        .await
        .unwrap()
    else {
        panic!("target is beyond first64 actual positions");
    };
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    assert!(through.seq < accepted.seq && generation > first.generation);
    assert_eq!(
        proof::fingerprints(&fixture.world.runtime, &run.run).await,
        fingerprints
    );
    cold(&mut fixture).await;
    fixture.world.runtime.fault.store(49, Ordering::SeqCst);
    assert_eq!(
        proof::receive(&fixture.world.runtime, &run.run)
            .await
            .unwrap_err(),
        RuntimeError::Unavailable
    );
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    let retained = proof::receive(&fixture.world.runtime, &run.run)
        .await
        .unwrap();
    let ReplyReceiptProgress::Accepted {
        accepted: seen,
        generation,
    } = &retained
    else {
        panic!("actual target must produce AcceptedReply");
    };
    assert_eq!(
        seen,
        &Seen {
            seq: accepted.seq,
            hash: accepted.hash.clone()
        }
    );
    cold(&mut fixture).await;
    assert_eq!(
        proof::receive(&fixture.world.runtime, &run.run)
            .await
            .unwrap(),
        retained
    );
    assert_eq!(
        native::testing::generation(
            &fixture.world.runtime,
            &fixture.agent.id,
            fixture.world.agents[0].device.as_deref().unwrap()
        )
        .await,
        *generation
    );
    assert_eq!(
        proof::fingerprints(&fixture.world.runtime, &run.run).await,
        fingerprints
    );
    proof::assert_pinned(&fixture.world.runtime, &run.run).await;
    assert_one_bill(&fixture).await;
    assert_eq!(
        fixture.world.runtime.pending_reply_runs(64).await.unwrap(),
        vec![run]
    );
    fixture.finish().await;
    println!("native output journey: actual paid capture, uncertain preparation ACK, original CID/Publish, cold64-position prefix, grant/device revocation, uncertain receipt ACK and inert cold replay; one provider request/bill, unchanged originals and retained wake/refs PASS");
    Ok(())
}

pub(super) async fn association_failure_does_not_block_incurred_evidence(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (fixture, run) = original().await?;
    fixture.world.runtime.fault.store(40, Ordering::SeqCst);
    fixture.world.runtime.run_model(&run.run).await.unwrap();
    fixture.world.runtime.fault.store(0, Ordering::SeqCst);
    assert!(proof::association(&fixture.world.runtime, &run.attempt)
        .await
        .is_none());
    assert_eq!(
        fixture
            .world
            .count("SELECT count(*) FROM runtime_evidence")
            .await,
        1
    );
    assert_eq!(
        fixture
            .world
            .runtime
            .execution
            .pending_evidence(64)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        proof::prepare(&fixture.world.runtime, &run.run)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert!(fixture.world.runtime.run_model(&run.run).await.is_err());
    assert_one_bill(&fixture).await;
    fixture.finish().await;
    println!("native output association cut: producer refusal occurs first, independent SQL and FDB incurred evidence remain; unindexed capture cannot be adopted or trigger a second provider request PASS");
    Ok(())
}
