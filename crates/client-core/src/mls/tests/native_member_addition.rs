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
    claim_group(&mut owner, space, &mut []);
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
    assert_eq!(owner.ingest(frame), Ingest::Confirmed);
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
    owner
        .logs
        .get(space)
        .ok_or("missing signed log")?
        .verify()?;
    Ok(())
}
