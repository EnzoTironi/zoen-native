//! Preserve public offline creation; pending membership alone grants no authority.
use super::*;

#[test]
fn public_creator_can_queue_an_initial_addition_before_genesis_confirmation() -> TestResult<()> {
    for community in [false, true] {
        let f = Fixture::new(SpaceKind::Group)?;
        let space = if community {
            f.client.create_community("Offline community".into())?
        } else {
            f.client
                .create_group_with("Offline group".into(), vec![], PrivacyDto::Closed)?
        };
        f.client.add_member(space.clone(), f.member.clone())?;
        let engine = f.client.lock();
        assert!(engine
            .logs
            .get(&space)
            .is_none_or(event_log::SpaceLog::is_empty));
        let events: Vec<_> = engine
            .store
            .outbox()?
            .into_iter()
            .filter(|row| row.event.space == space)
            .map(|row| row.event)
            .collect();
        assert_eq!(
            events.len(),
            2,
            "genesis then addition, without awaiting the relay"
        );
        for event in &events {
            event_log::verify_author(event)?;
        }
        assert!(matches!(events[0].body, EventBody::SpaceCreated { .. }));
        assert!(
            matches!(&events[1].body, EventBody::MemberAdded { identity, role: Role::Member } if identity == &f.member)
        );
    }
    Ok(())
}
