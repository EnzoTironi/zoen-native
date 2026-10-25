//! A real separate owner Client queues a signed renewal decision before its echo.
use super::*;

#[test]
fn actual_owner_optimistic_renewed_decision_cannot_execute_before_canonical_echo(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture().unwrap();
    let (old, _) = expired_approved(&f, "read_deckcraft").unwrap();
    let intent = request_again(&f, &old);
    let next = f
        .agent
        .stage_tool_review_successor(&old.id, &intent)
        .unwrap()
        .unwrap();
    confirm_pending(&f)?;
    let (profiles, history) = {
        let e = f.agent.lock();
        (
            e.identities.values().cloned().collect(),
            e.logs[&f.space].events().to_vec(),
        )
    };
    {
        let mut owner = f.owner_client.lock();
        owner.put_profiles(profiles).unwrap();
        for event in history {
            assert_eq!(
                owner.ingest(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(&event)?,
                }),
                crate::Ingest::Applied
            );
        }
    }
    // This is the owner's actual account/device and exported decision command.
    // No identity swap, synced-flag edit or synthetic approval overlay is used.
    let decided = f
        .owner_client
        .decide_tool_request(
            next.id.clone(),
            observed(&next).into(),
            ToolRequestDecision::ApproveOnce,
        )
        .unwrap();
    assert_eq!(decided.request.status, RequestStatus::Approved);
    let pending = {
        let owner = f.owner_client.lock();
        assert_eq!(owner.me_id().unwrap(), f.owner);
        assert!(owner.net.optimistic_approvals.contains(&f.space));
        owner.store.outbox().unwrap().into_iter().find(|row|
            matches!(&row.event.body, EventBody::RequestResolved {request,..} if request==&next.id)
        ).unwrap().event
    };
    event_log::verify_author(&pending).unwrap();
    assert_eq!(pending.author, f.owner);
    assert!(pending.device.is_some());
    assert_eq!(
        f.agent
            .requests()
            .iter()
            .find(|r| r.id == next.id)
            .unwrap()
            .status,
        RequestStatus::Pending
    );
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_err());
    assert!(f.agent.approved_tool_turn(&old.id, WORKER).is_err());
    let event = {
        let agent = f.agent.lock();
        let mut log =
            event_log::SpaceLog::from_events(&f.space, agent.logs[&f.space].events().to_vec())
                .unwrap();
        log.sequence(pending)?.clone()
    };
    let echoed = protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(&event)?,
    };
    assert_eq!(
        f.agent.lock().ingest(echoed.clone()),
        crate::Ingest::Applied
    );
    assert_eq!(
        f.owner_client.lock().ingest(echoed),
        crate::Ingest::Confirmed
    );
    assert!(!f
        .owner_client
        .lock()
        .store
        .outbox()
        .unwrap()
        .iter()
        .any(|row| row.event.client_id == event.client_id));
    assert!(f.agent.approved_tool_turn(&next.id, WORKER).is_ok());
    assert_v1(&f);
    Ok(())
}
