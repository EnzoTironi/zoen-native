//! Shared signed expiry history for schema1 and schema2; no wall-clock sleep.
use super::*;

pub(crate) fn signed(
    f: &Fixture,
    author: &str,
    body: EventBody,
    at: i64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut e = f.agent.lock();
    let mut log =
        event_log::SpaceLog::from_events(&f.space, e.logs[&f.space].events().to_vec()).unwrap();
    let signed = e.author_for(author).unwrap().sign_event(
        &f.space,
        &types::new_ulid(at)?,
        at,
        log.head(),
        body,
    )?;
    let event = log.sequence(signed)?.clone();
    assert_eq!(
        e.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(&event)?,
        }),
        crate::sync::Ingest::Applied
    );

    Ok(())
}

pub(crate) fn expired_approved(
    f: &Fixture,
    live: crate::AgentRequestDto,
) -> std::result::Result<crate::AgentRequestDto, Box<dyn std::error::Error>> {
    let mut request = f.agent.lock().state.requests[&live.id].req.clone();
    request.id = types::new_id("expired_review")?;
    let expiry = crate::engine::now_ms() - 1000;
    request.tool_proposal.as_mut().unwrap().expires_at_ms = expiry;
    request.content_hash = request.tool_proposal_hash(&f.space).unwrap();
    let (id, hash) = (request.id.clone(), request.content_hash.clone());
    let agent = request.agent.clone();
    signed(
        f,
        &agent,
        EventBody::RequestOpened { request },
        expiry - 1000,
    )?;
    signed(
        f,
        &f.owner,
        EventBody::RequestResolved {
            request: id.clone(),
            approved: true,
            content_hash: hash,
            resolution: Some(types::RequestResolution {
                message: "Approved once. Waiting for the reviewed action.".into(),
                created_item: None,
                ai_cost_cents: 0,
                standing_grant: None,
                tool_decision: Some(types::ToolDecision::ApproveOnce),
            }),
        },
        expiry - 1,
    )?;
    let dto = f.agent.requests().into_iter().find(|r| r.id == id).unwrap();
    assert_eq!(dto.status, crate::RequestStatus::Approved);
    Ok(dto)
}

/// Sequence this fixture's actual outbox, so renewed permissions cannot rely
/// on a locally optimistic ordering. No network transport or authority bypass.
pub(crate) fn confirm_pending(f: &Fixture) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut e = f.agent.lock();
    let rows = e.store.outbox().unwrap();
    for row in rows
        .into_iter()
        .filter(|row| row.event.space == f.space && !row.failed)
    {
        let mut log =
            event_log::SpaceLog::from_events(&f.space, e.logs[&f.space].events().to_vec()).unwrap();
        let event = log.sequence(row.event)?.clone();
        assert_eq!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)?,
            }),
            crate::sync::Ingest::Confirmed
        );
    }
    Ok(())
}
