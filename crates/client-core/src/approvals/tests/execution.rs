//! Whole execution cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn always_approve_commits_its_grant_with_the_item_and_resumes_an_interrupted_batch() {
    let (chat, mut e) = Chat::create();
    for prompt in ["first", "second"] {
        e.agent_create_plan(
            &chat.space,
            &chat.agent,
            prompt,
            plan(),
            "Scripted model",
            37,
        )
        .unwrap();
    }
    let first = e.state.request_order[0].clone();
    let second = e.state.request_order[1].clone();
    let grants = e.state.grants.len();
    e.store.conn().execute_batch("CREATE TRIGGER fail_approval BEFORE INSERT ON events WHEN NEW.json LIKE '%RequestResolved%' BEGIN SELECT RAISE(ABORT, 'approval fault'); END;").unwrap();
    assert!(e
        .decide_request(&first, RequestDecision::AlwaysApprove)
        .is_err());
    assert!(e.items().is_empty());
    assert_eq!(e.state.grants.len(), grants);
    assert_eq!(e.state.requests[&first].status, ReqStatus::Pending);
    e.store
        .conn()
        .execute_batch("DROP TRIGGER fail_approval")
        .unwrap();
    let sql = format!("CREATE TRIGGER fail_covered BEFORE INSERT ON events WHEN NEW.json LIKE '%RequestResolved%' AND NEW.json LIKE '%{second}%' BEGIN SELECT RAISE(ABORT, 'covered fault'); END;");
    e.store.conn().execute_batch(&sql).unwrap();
    assert!(e
        .decide_request(&first, RequestDecision::AlwaysApprove)
        .is_err());
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.state.grants.len(), grants + 1);
    assert_eq!(e.state.requests[&second].status, ReqStatus::Pending);
    let standing = e.state.requests[&first]
        .resolution
        .as_ref()
        .unwrap()
        .standing_grant
        .as_ref()
        .unwrap()
        .id
        .clone();
    drop(e);
    let mut e = chat.reopen();
    e.store
        .conn()
        .execute_batch("DROP TRIGGER fail_covered")
        .unwrap();
    let resumed = e
        .decide_request(&first, RequestDecision::AlwaysApprove)
        .unwrap();
    assert_eq!(
        resumed.standing_grant_id.as_deref(),
        Some(standing.as_str())
    );
    assert_eq!(resumed.also_resolved, 1);
    assert_eq!(e.items().len(), 2);
    assert_eq!(e.state.grants.len(), grants + 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(74));
    let count = e.store.event_count().unwrap();
    e.decide_request(&first, RequestDecision::AlwaysApprove)
        .unwrap();
    assert_eq!(e.store.event_count().unwrap(), count);
}

pub(super) fn agent_removal_prevents_execution_and_denial_keeps_the_model_charge_once() {
    let (chat, mut e) = Chat::create();
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
    let r = e.state.requests[&id].clone();
    e.append(
        &chat.space,
        &chat.owner,
        EventBody::MemberRemoved {
            identity: chat.agent.clone(),
        },
    )
    .unwrap();
    assert_eq!(e.requests()[0].status, RequestStatus::Stale);
    assert!(e.resolve_request(&id, true).is_err());
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r));
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    e.resolve_request(&id, false).unwrap();
    e.resolve_request(&id, false).unwrap();
    assert_eq!(e.requests()[0].status, RequestStatus::Denied);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

pub(super) fn direct_agent_outputs_and_usage_roll_back_together_when_a_later_write_fails() {
    let (chat, mut e) = Chat::create();
    e.set_trust(&chat.agent, &chat.space, TrustLevel::Act)
        .unwrap();
    let before = e.store.event_count().unwrap();
    let entries = e.timeline(&chat.space).unwrap().len();
    e.store.conn().execute_batch("CREATE TRIGGER fail_usage BEFORE INSERT ON events WHEN NEW.json LIKE '%UsageRecorded%' BEGIN SELECT RAISE(ABORT, 'usage fault'); END;").unwrap();
    assert!(e
        .agent_create_plan(
            &chat.space,
            &chat.agent,
            "Keep the payload",
            plan(),
            "Scripted model",
            37
        )
        .is_err());
    assert!(e
        .agent_say(&chat.space, &chat.agent, "A real reply", 37)
        .is_err());
    assert_eq!(e.store.event_count().unwrap(), before);
    assert_eq!(e.timeline(&chat.space).unwrap().len(), entries);
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(0));
    drop(e);
    let mut e = chat.reopen();
    assert_eq!(e.store.event_count().unwrap(), before);
    e.store
        .conn()
        .execute_batch("DROP TRIGGER fail_usage")
        .unwrap();
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
    assert!(out.item.is_some());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

pub(super) fn revoked_standing_grants_cannot_resume_an_interrupted_batch() {
    let (chat, mut e) = Chat::create();
    for prompt in ["first", "second"] {
        e.agent_create_plan(
            &chat.space,
            &chat.agent,
            prompt,
            plan(),
            "Scripted model",
            37,
        )
        .unwrap();
    }
    let first = e.state.request_order[0].clone();
    let second = e.state.request_order[1].clone();
    let sql = format!("CREATE TRIGGER fail_covered BEFORE INSERT ON events WHEN NEW.json LIKE '%RequestResolved%' AND NEW.json LIKE '%{second}%' BEGIN SELECT RAISE(ABORT, 'covered fault'); END;");
    e.store.conn().execute_batch(&sql).unwrap();
    assert!(e
        .decide_request(&first, RequestDecision::AlwaysApprove)
        .is_err());
    let grant = e.state.requests[&first]
        .resolution
        .as_ref()
        .unwrap()
        .standing_grant
        .as_ref()
        .unwrap()
        .id
        .clone();
    e.store
        .conn()
        .execute_batch("DROP TRIGGER fail_covered")
        .unwrap();
    e.revoke_standing(&grant).unwrap();
    drop(e);
    let mut e = chat.reopen();
    assert!(e
        .decide_request(&first, RequestDecision::AlwaysApprove)
        .is_err());
    assert_eq!(e.state.requests[&second].status, ReqStatus::Pending);
    assert_eq!(e.items().len(), 1);
}
