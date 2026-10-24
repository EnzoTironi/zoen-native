//! Whole recovery cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn a_second_device_projects_the_same_output_after_its_agent_profile_arrives(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (chat, mut e) = Chat::create()?;
    e.agent_create_plan(
        &chat.space,
        &chat.agent,
        "Keep the payload",
        plan(),
        "Scripted model",
        37,
    )
    .unwrap();
    let id = e.requests()[0].id.clone();
    let mut observer = Engine::open(":memory:").unwrap();
    observer
        .put_profiles(vec![e.identities[&chat.owner].clone()])
        .unwrap();
    let frame = |event: &Event| protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: protocol::Envelope::plain(event),
    };
    for event in e.logs[&chat.space].events() {
        assert_eq!(observer.ingest(frame(event)), crate::sync::Ingest::Applied);
    }
    assert!(
        observer.requests().is_empty(),
        "unknown ownership cannot authorize an agent request"
    );
    observer
        .put_profiles(vec![e.identities[&chat.agent].clone()])
        .unwrap();
    assert_eq!(
        observer.requests().len(),
        1,
        "verified history is reprojected when ownership becomes known"
    );
    assert_eq!(
        observer.agent_profile(&chat.agent).budget_spent_cents,
        Some(37)
    );
    e.resolve_request(&id, true).unwrap();
    let approved = frame(e.logs[&chat.space].events().last().unwrap());
    assert_eq!(
        observer.ingest(approved.clone()),
        crate::sync::Ingest::Applied
    );
    assert_eq!(observer.items().len(), 1);
    assert_eq!(observer.items()[0].id, e.items()[0].id);
    assert_eq!(observer.items()[0].plan, e.items()[0].plan);
    assert_eq!(observer.ingest(approved), crate::sync::Ingest::Duplicate);
    observer.reload().unwrap();
    assert_eq!(observer.items().len(), 1);
    assert_eq!(
        observer.agent_profile(&chat.agent).budget_spent_cents,
        Some(37)
    );

    Ok(())
}

pub(super) fn plan_approval_after_restart_creates_the_exact_item_and_debits_usage_once(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (chat, mut e) = Chat::create()?;
    let out = e
        .agent_create_plan(
            &chat.space,
            &chat.agent,
            "Keep the payload",
            plan(),
            "Scripted model",
            37,
        )
        .unwrap();
    assert_eq!(out.kind, DecisionKind::Request);
    assert!(e.items().is_empty());
    let id = e.requests()[0].id.clone();
    drop(e);
    let mut e = chat.reopen();
    e.resolve_request(&id, true).unwrap();
    let items = e.items();
    assert_eq!(
        items.len(),
        1,
        "approval must create the persisted proposal, even after restart"
    );
    let item = e.item(&items[0].id).unwrap();
    assert_eq!(
        item.plan.as_ref().unwrap().sections[0].lines[0].text,
        "A durable proposal"
    );
    assert_eq!(item.created_by.id, chat.agent);
    assert!(item.origin.contains("Scripted model"));
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    let count = e.store.event_count().unwrap();
    let repeated = e.resolve_request(&id, true).unwrap();
    assert_eq!(repeated.request.status, RequestStatus::Approved);
    assert_eq!(
        e.store.event_count().unwrap(),
        count,
        "repeat decisions must return the existing receipt"
    );
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    assert!(e.verify_all().iter().all(|r| r.valid));

    Ok(())
}

pub(super) fn app_approval_after_restart_creates_an_usable_item_with_its_scoped_grant(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (chat, mut e) = Chat::create()?;
    let out = e
        .agent_create_app(
            &chat.space,
            &chat.agent,
            "adopt_pet",
            r#"{"name":"Durable Donkey"}"#,
            "Scripted model",
            "Create this pet",
        )
        .unwrap();
    assert_eq!(out.kind, DecisionKind::Request);
    let id = e.requests()[0].id.clone();
    drop(e);
    let mut e = chat.reopen();
    let out = e.resolve_request(&id, true).unwrap();
    assert_eq!(out.request.status, RequestStatus::Approved);
    let items = e.items();
    assert_eq!(items.len(), 1, "an approved app must exist");
    let item = e.item(&items[0].id).unwrap();
    assert_eq!(item.app.unwrap().trust, TrustLevelDto::Act);
    let fed = e.app_call_tool(&item.id, "pet_feed", "{}", false).unwrap();
    assert_eq!(fed.status, AppCallStatus::Done);
    assert_eq!(fed.item.unwrap().version, 2);

    Ok(())
}

pub(super) fn failed_approval_write_leaves_the_proposal_pending_and_identical_retry_succeeds(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (chat, mut e) = Chat::create()?;
    e.agent_create_plan(
        &chat.space,
        &chat.agent,
        "Keep the payload",
        plan(),
        "Scripted model",
        37,
    )
    .unwrap();
    let id = e.requests()[0].id.clone();
    let count = e.store.event_count().unwrap();
    e.store.conn().execute_batch("CREATE TRIGGER fail_approval BEFORE INSERT ON events WHEN NEW.json LIKE '%RequestResolved%' BEGIN SELECT RAISE(ABORT, 'approval fault'); END;").unwrap();
    assert!(e.resolve_request(&id, true).is_err());
    assert_eq!(e.store.event_count().unwrap(), count);
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    assert_eq!(
        e.agent_profile(&chat.agent).budget_spent_cents,
        Some(37),
        "preparing a proposal costs once even when approval storage fails"
    );
    drop(e);
    let mut e = chat.reopen();
    e.store
        .conn()
        .execute_batch("DROP TRIGGER fail_approval")
        .unwrap();
    e.resolve_request(&id, true).unwrap();
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));

    Ok(())
}
