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

#[test]
fn pending_genesis_is_not_a_failed_foreign_corrupt_or_revoked_authority_fallback() -> TestResult<()>
{
    for fault in ["failed", "foreign", "corrupt", "revoked"] {
        let f = Fixture::new(SpaceKind::Group)?;
        let space = f.client.create_community("Unconfirmed birth".into())?;
        {
            let e = f.client.lock();
            let original = e
                .store
                .outbox()?
                .into_iter()
                .find(|row| row.event.space == space)
                .ok_or("genesis missing")?
                .event;
            match fault {
                "failed" => {
                    e.store
                        .outbox_note(&original.client_id, "synthetic rejection", true)?
                }
                "revoked" => e
                    .store
                    .set_meta("devices.revoked", &serde_json::to_string(&vec![&f.device])?)?,
                _ => {
                    let mut changed = if fault == "foreign" {
                        e.author_for(&f.founder)?.sign_event(
                            &space,
                            &original.client_id,
                            original.at_ms,
                            None,
                            original.body,
                        )?
                    } else {
                        original.clone()
                    };
                    if fault == "foreign" {
                        event_log::verify_author(&changed)?;
                    } else {
                        changed.cert = Some("corrupt certificate".into());
                    }
                    e.store.conn().execute(
                        "UPDATE outbox SET json=?1 WHERE client_id=?2",
                        rusqlite::params![serde_json::to_string(&changed)?, original.client_id],
                    )?;
                }
            }
        }
        let before = f.counts()?;
        let original: Vec<_> = f
            .client
            .lock()
            .store
            .outbox()?
            .into_iter()
            .map(|row| row.event)
            .collect();
        assert!(
            f.client.add_member(space, f.member.clone()).is_err(),
            "{fault}"
        );
        assert_eq!(f.counts()?, before);
        assert_eq!(
            f.client
                .lock()
                .store
                .outbox()?
                .into_iter()
                .map(|row| row.event)
                .collect::<Vec<_>>(),
            original
        );
    }
    Ok(())
}
