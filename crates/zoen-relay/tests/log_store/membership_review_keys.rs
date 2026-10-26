//! Malformed membership must be refused per envelope before any oversized FDB key read.
use super::*;

pub(super) async fn malformed_membership_target_does_not_poison_a_shared_batch(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Reuse the ordinary ordering contract: the worker cannot run before join_all
    // has queued both appends on this single-threaded runtime.
    tokio::task::spawn_blocking(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(shared_membership_batch())
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap()?;
    Ok(())
}

async fn shared_membership_batch() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let log = store()?;
    let owner = Author::device(&Signer::generate()?, Signer::generate()?);
    let (space, genesis) = create(&log, &owner).await?;
    let target = Signer::generate()?.id();
    let malformed = "a".repeat(16 * 1024);
    let envelopes = [
        sign(
            &owner,
            &space,
            Some(genesis.clone()),
            EventBody::MemberAdded {
                identity: target.clone(),
                role: Role::Member,
            },
        )?,
        sign(
            &owner,
            &space,
            Some(genesis),
            EventBody::MemberAdded {
                identity: malformed.clone(),
                role: Role::Member,
            },
        )?,
    ];
    assert!(
        malformed.len() > 10_000,
        "the target exceeds FDB's key limit"
    );
    for envelope in &envelopes {
        assert!(
            envelope.verify().is_ok(),
            "both envelopes are genuinely certified"
        );
        assert!(
            envelope.stored_len() <= zoen_relay::session::MAX_ENVELOPE,
            "this is a bounded public envelope, not an oversized frame"
        );
    }
    assert_eq!(log.role(&space, &owner.identity).await?, Some(Role::Owner));
    // Both targets are declared known at the LogStore boundary. Directory lookup
    // cannot explain the malformed target's refusal or mask the oversized key.
    let results =
        futures_util::future::join_all(envelopes.iter().map(|envelope| log.append(envelope, true)))
            .await;
    let events = read_all(&log, &space, 20).await;
    let target_role = log.role(&space, &target).await;
    // Clean our own cell before expected RED assertions, including a batch-wide
    // retry error from the former unbounded removal-key read.
    log.drop_cell().await?;

    assert!(
        matches!(&results[0], Ok(Sequencing::New { ev, .. })
            if ev.seq == 1 && ev.env.client_id() == envelopes[0].client_id()),
        "the valid member must commit in the same batch: {:?}",
        results[0]
    );
    assert!(
        matches!(&results[1], Err(rejection)
            if rejection.permanent && rejection.reason == "not an identity key"),
        "the malformed target needs its ordinary permanent refusal: {:?}",
        results[1]
    );
    assert_eq!(target_role?, Some(Role::Member));
    assert_eq!(
        events.len(),
        2,
        "only genesis and the valid inclusion persist"
    );
    assert!(events
        .iter()
        .all(|event| event.env.client_id() != envelopes[1].client_id()));
    assert_chain(&space, &events)?;
    Ok(())
}
