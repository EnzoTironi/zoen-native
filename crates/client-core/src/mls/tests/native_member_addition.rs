//! Reviewed native inclusion reaches the normal MLS claim gate, without bypassing it.
use super::*;

#[test]
fn reviewed_native_addition_waits_for_new_members_key_package(
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut owner, _, _) = member(":memory:");
    let (mut peer, _, _) = member(":memory:");
    let target = peer.me.clone().ok_or("missing peer")?;
    owner.put_profiles(vec![peer
        .identities
        .get(&target)
        .ok_or("missing profile")?
        .clone()])?;
    owner.net.profiles.supported = true;
    peer.net.profiles.supported = true;
    let (public, signed) = peer.agreement_to_publish().ok_or("missing agreement key")?;
    owner.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: target.clone(),
            public,
            signed,
        }],
        std::slice::from_ref(&target),
    );
    let space = "native-reviewed-add-mls";
    for frame in claim_group(&mut owner, space, &mut []) {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    owner.store.set_synced(space)?;
    owner.net.synced.insert(space.into());
    let old_epoch = owner.device()?.epoch(space)?;
    let account = owner.net.account.clone().ok_or("missing account")?;
    let review = owner.space_member_context(space, &target, &account.identity, &account.device)?;
    assert!(review.can_add);
    owner.add_member_reviewed(
        space,
        &target,
        &account.identity,
        &account.device,
        &review.revision,
    )?;
    let pending = owner
        .store
        .outbox()?
        .into_iter()
        .find(|row| {
            matches!(&row.event.body,
        EventBody::MemberAdded { identity, role: Role::Member } if identity == &target)
        })
        .ok_or("missing signed MemberAdded")?;
    let frame = ordered(&owner, Envelope::plain(&pending.event));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame), Ingest::Applied);
    let waiting = owner.space_member_context(space, &target, &account.identity, &account.device)?;
    assert_eq!(waiting.state, crate::MemberActionState::Active);
    assert!(!waiting.delivery_ready);
    assert!(!waiting.can_add);
    let existing =
        owner.space_member_context(space, &account.identity, &account.identity, &account.device)?;
    assert!(
        existing.delivery_ready,
        "the existing owner remains a reader"
    );
    let clock = protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or("invalid claim clock")?;
    let (_, targets, operation) = owner
        .mls_to_claim(Some(clock))?
        .ok_or("missing MLS claim")?;
    assert!(targets.contains(&target));
    owner.mls_claimed(space, &operation, Ok(vec![]));
    assert!(owner.net.mls.stuck.contains(space));
    assert_eq!(
        owner.device()?.epoch(space)?,
        old_epoch,
        "membership alone does not advance the MLS epoch"
    );
    assert!(
        !peer.device()?.has_group(space),
        "the newcomer still has no MLS group"
    );
    let counts = (owner.store.event_count()?, owner.store.outbox()?.len());
    assert!(owner
        .add_member_reviewed(
            space,
            &target,
            &account.identity,
            &account.device,
            &review.revision
        )
        .is_err());
    assert_eq!(
        (owner.store.event_count()?, owner.store.outbox()?.len()),
        counts
    );
    // Advance only the existing retry scheduler; a genuine KeyPackage and the
    // normal commit/Welcome publication establish the next actual group roster.
    owner.net.mls.retry_at.insert(space.into(), Instant::now());
    let (_, targets, operation) = owner.mls_to_claim(Some(clock))?.ok_or("missing retry")?;
    assert_eq!(targets, vec![target.clone()]);
    let records = claimed_records(std::slice::from_mut(&mut peer), &targets);
    assert_eq!(records.len(), 1);
    owner.mls_claimed(space, &operation, Ok(records));
    relay_ready_to_peer(&mut owner, &mut peer);
    assert!(peer.device()?.has_group(space));
    let ready = owner.space_member_context(space, &target, &account.identity, &account.device)?;
    assert_eq!(ready.state, crate::MemberActionState::Active);
    assert!(ready.delivery_ready);
    assert!(!ready.can_add);
    assert_ne!(
        ready.revision, waiting.revision,
        "readiness is part of the observed fingerprint"
    );
    let log = owner.logs.get(space).ok_or("missing signed log")?;
    assert_eq!(
        log.events()
            .iter()
            .filter(|event| matches!(&event.body,
        EventBody::MemberAdded { identity, role: Role::Member } if identity == &target))
            .count(),
        1
    );
    log.verify()?;
    Ok(())
}

#[test]
fn confirmed_non_e2e_members_are_ready_without_an_mls_group(
) -> Result<(), Box<dyn std::error::Error>> {
    for privacy in [Privacy::Closed, Privacy::Public] {
        let (mut owner, author, _) = member(":memory:");
        let (peer, _, _) = member(":memory:");
        let target = peer.me_id()?;
        owner.put_profiles(vec![peer
            .identities
            .get(&target)
            .ok_or("missing profile")?
            .clone()])?;
        let space = "non-e2e-member-readiness";
        ingest_signed(
            &mut owner,
            &author,
            space,
            EventBody::SpaceCreated {
                title: "Non-E2E".into(),
                kind: SpaceKind::Group,
                privacy,
            },
        );
        owner.store.set_synced(space)?;
        owner.net.synced.insert(space.into());
        let account = owner.net.account.clone().ok_or("missing account")?;
        let review =
            owner.space_member_context(space, &target, &account.identity, &account.device)?;
        assert!(!review.delivery_ready);
        owner.add_member_reviewed(
            space,
            &target,
            &account.identity,
            &account.device,
            &review.revision,
        )?;
        let pending = owner
            .store
            .outbox()?
            .into_iter()
            .find(|row| {
                matches!(&row.event.body,
            EventBody::MemberAdded { identity, role: Role::Member } if identity == &target)
            })
            .ok_or("missing member intent")?;
        let frame = ordered(&owner, Envelope::plain(&pending.event));
        assert_eq!(owner.ingest(frame), Ingest::Confirmed);
        let confirmed =
            owner.space_member_context(space, &target, &account.identity, &account.device)?;
        assert_eq!(confirmed.state, crate::MemberActionState::Active);
        assert!(confirmed.delivery_ready);
        assert!(!confirmed.can_add);
        assert!(owner.mls_status(space).is_none());
    }
    Ok(())
}
