use super::*;
use crate::mls_recovery::recovery_probe;
use std::time::Duration;

#[test]
fn settled_groups_do_not_repeat_recovery_download_group_reads(
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut owner, _, _) = member(":memory:");
    let (mut peer, _, _) = member(":memory:");
    owner.net.profiles.supported = true;
    let clock = protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or("missing fixture clock")?;
    for index in 0..32 {
        let space = format!("recovery-download-{index}");
        for frame in claim_group(&mut owner, &space, std::slice::from_mut(&mut peer)) {
            assert_eq!(peer.ingest(frame), Ingest::Applied);
        }
        owner.send_message(&space, "Exact established source")?;
        let (_, targets, operation) = owner
            .mls_to_claim(Some(clock))?
            .ok_or("missing fixture claim")?;
        let records = claimed_records(std::slice::from_mut(&mut peer), &targets);
        owner.mls_claimed(&space, &operation, Ok(records));
        relay_ready_to_peer(&mut owner, &mut peer);
        assert!(peer.device()?.has_group(&space));
        assert!(peer.store.meta(&format!("recovery:{space}"))?.is_some());
        assert!(peer.timeline(&space)?.iter().any(|entry| matches!(
            &entry.kind, crate::dto::EntryKind::Message { text, .. } if text == "Exact established source"
        )));
    }
    peer.profiles_session_start(true);
    recovery_probe::reset(Duration::from_millis(2));
    let initial = Instant::now();
    assert!(peer.wanted_recovery_contexts().is_empty());
    let first_download_ms = initial.elapsed().as_millis();
    let recovery = Instant::now();
    assert!(!peer.mls_recovery_pass()?);
    let recovery_ms = recovery.elapsed().as_millis();
    let repeated = Instant::now();
    assert!(peer.wanted_recovery_contexts().is_empty());
    let repeated_download_ms = repeated.elapsed().as_millis();
    // A cold profile refresh can return a peer's agreement key after initial
    // recovery classification. These genuine ready groups already retain their
    // contexts; that unrelated response must not load every group again.
    let identity = owner.me.clone().ok_or("missing fixture owner")?;
    let (public, signed) = owner
        .agreement_to_publish()
        .ok_or("missing fixture agreement")?;
    assert!(peer.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        std::slice::from_ref(&identity),
    ));
    let after_agreement = Instant::now();
    assert!(!peer.mls_recovery_pass()?);
    let after_agreement_ms = after_agreement.elapsed().as_millis();
    let (downloads, groups) = recovery_probe::reads();
    recovery_probe::reset(Duration::ZERO);
    eprintln!("recovery_read_work groups=32 actual_download_reads={downloads} actual_recovery_reads={groups} first_download_ms={first_download_ms} recovery_ms={recovery_ms} repeated_download_ms={repeated_download_ms} after_agreement_ms={after_agreement_ms} injected_per_read_ms=2");
    assert_eq!(
        groups, 32,
        "one real classification is required; profile arrival must not repeat it"
    );
    assert_eq!(
        downloads, 0,
        "ready groups need no HTTP candidate revalidation read"
    );
    Ok(())
}
