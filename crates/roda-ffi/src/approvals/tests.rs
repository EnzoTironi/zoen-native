use crate::dto::*;
use crate::engine::{Engine, ReqStatus, RequestState};
use roda_types::*;

struct Chat {
    path: String,
    owner: String,
    agent: String,
    space: String,
}

impl Chat {
    fn create() -> (Self, Engine) {
        let path = std::env::temp_dir()
            .join(format!("{}.sqlite", new_id("zoen_approval")))
            .to_string_lossy()
            .into_owned();
        let mut e = Engine::open(&path).unwrap();
        let owner = e
            .create_identity(identity(IdentityKind::Person, None), true)
            .unwrap();
        e.me = Some(owner.clone());
        e.store.set_meta("me", &owner).unwrap();
        let agent = e
            .create_identity(identity(IdentityKind::Agent, Some(owner.clone())), true)
            .unwrap();
        let space = new_id("sp");
        e.append(
            &space,
            &owner,
            EventBody::SpaceCreated {
                title: "Approval journey".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .unwrap();
        e.append(
            &space,
            &owner,
            EventBody::MemberAdded {
                identity: agent.clone(),
                role: Role::Member,
            },
        )
        .unwrap();
        e.set_trust(&agent, &space, TrustLevel::Suggest).unwrap();
        e.append(
            &space,
            &owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr"),
                    grantor: owner.clone(),
                    grantee: Some(agent.clone()),
                    scope: GrantScope::Everywhere,
                    capability: Capability::MonthlyBudget { cents: 1000 },
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
        (
            Self {
                path,
                owner,
                agent,
                space,
            },
            e,
        )
    }

    fn reopen(&self) -> Engine {
        Engine::open(&self.path).unwrap()
    }
}

impl Drop for Chat {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path));
        }
    }
}

fn identity(kind: IdentityKind, owner: Option<String>) -> Identity {
    Identity {
        id: String::new(),
        kind,
        name: "Journey principal".into(),
        handle: String::new(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner,
        bio: String::new(),
    }
}

fn plan() -> PlanDto {
    PlanDto {
        title: "Persist this exact plan".into(),
        summary: "Owner review".into(),
        budget_cents: Some(5000),
        total_cents: 0,
        sections: vec![PlanSectionDto {
            title: "Work".into(),
            lines: vec![PlanLineDto {
                id: "line_original".into(),
                text: "A durable proposal".into(),
                cost_cents: 4200,
                done: false,
            }],
        }],
    }
}

fn ingest_signed(e: &mut Engine, space: &str, author: &str, body: EventBody) {
    let mut log = roda_log::SpaceLog::from_events(space, e.logs[space].events().to_vec()).unwrap();
    let signer = e.author_for(author).unwrap();
    let signed = signer.sign_event(
        space,
        &new_ulid(crate::engine::now_ms()),
        crate::engine::now_ms(),
        log.head(),
        body,
    );
    let event = log.sequence(signed).clone();
    let frame = roda_proto::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: roda_proto::Envelope::plain(&event),
    };
    assert_eq!(e.ingest(frame), crate::sync::Ingest::Applied);
}

fn resolution(r: &RequestState) -> EventBody {
    let p = r.req.proposal.as_ref().unwrap();
    EventBody::RequestResolved {
        request: r.req.id.clone(),
        approved: true,
        content_hash: r.req.content_hash.clone(),
        resolution: Some(RequestResolution {
            message: p.message.clone(),
            created_item: Some(p.item.clone()),
            ai_cost_cents: p.ai_cost_cents,
            standing_grant: None,
        }),
    }
}

#[test]
fn authenticated_wrong_owner_hash_space_and_duplicate_decisions_have_no_effect_on_ingest_or_replay()
{
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
    let stranger = e
        .create_identity(identity(IdentityKind::Person, None), true)
        .unwrap();
    e.append(
        &chat.space,
        &chat.owner,
        EventBody::MemberAdded {
            identity: stranger.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    let other_space = new_id("sp");
    e.append(
        &other_space,
        &chat.owner,
        EventBody::SpaceCreated {
            title: "Other Space".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
    .unwrap();
    e.append(
        &other_space,
        &chat.owner,
        EventBody::MemberAdded {
            identity: chat.agent.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    ingest_signed(&mut e, &chat.space, &stranger, resolution(&r));
    let mut wrong_hash = resolution(&r);
    if let EventBody::RequestResolved { content_hash, .. } = &mut wrong_hash {
        *content_hash = "0".repeat(64);
    }
    ingest_signed(&mut e, &chat.space, &chat.owner, wrong_hash);
    ingest_signed(&mut e, &other_space, &chat.owner, resolution(&r));
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    drop(e);
    let mut e = chat.reopen();
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r));
    assert_eq!(e.items().len(), 1);
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r));
    let mut deny = resolution(&r);
    if let EventBody::RequestResolved {
        approved,
        resolution,
        ..
    } = &mut deny
    {
        *approved = false;
        *resolution = None;
    }
    ingest_signed(&mut e, &chat.space, &chat.owner, deny);
    assert_eq!(e.state.requests[&id].status, ReqStatus::Approved);
    assert_eq!(e.state.resolved_order.len(), 1);
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.state.resolved_order.len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    assert!(e.verify_all().iter().all(|r| r.valid));
}

#[test]
fn changed_proposals_impersonated_requests_and_forged_usage_are_not_projected() {
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
    let r = e.state.requests[&e.requests()[0].id].clone();
    let mut changed = r.req.clone();
    changed.id = new_id("rq");
    changed.content_hash = changed.proposal_hash(&chat.space).unwrap();
    if let ItemContent::Plan(p) = &mut changed.proposal.as_mut().unwrap().content {
        p.sections[0].lines[0].text = "Unreviewed replacement".into();
    }
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::RequestOpened { request: changed },
    );
    let mut impersonated = r.req.clone();
    impersonated.id = new_id("rq");
    impersonated.content_hash = impersonated.proposal_hash(&chat.space).unwrap();
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::RequestOpened {
            request: impersonated,
        },
    );
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::UsageRecorded {
            agent: chat.agent.clone(),
            cents: 500,
            what: "forged charge".into(),
        },
    );
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::UsageRecorded {
            agent: chat.agent.clone(),
            cents: -500,
            what: "negative charge".into(),
        },
    );
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

#[test]
fn always_approve_commits_its_grant_with_the_item_and_resumes_an_interrupted_batch() {
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

#[test]
fn agent_removal_prevents_execution_and_denial_keeps_the_model_charge_once() {
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

#[test]
fn direct_agent_outputs_and_usage_roll_back_together_when_a_later_write_fails() {
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

#[test]
fn a_second_device_projects_the_same_output_after_its_agent_profile_arrives() {
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
    let mut observer = Engine::open(":memory:").unwrap();
    observer
        .put_profiles(vec![e.identities[&chat.owner].clone()])
        .unwrap();
    let frame = |event: &Event| roda_proto::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: roda_proto::Envelope::plain(event),
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
}

#[test]
fn revoked_standing_grants_cannot_resume_an_interrupted_batch() {
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

#[test]
fn claiming_an_owner_without_their_signed_grant_cannot_charge_for_a_proposal() {
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
    let mut req = e.state.requests[&e.requests()[0].id].req.clone();
    let trust = e
        .state
        .grants
        .iter()
        .find(|g| matches!(g.grant.capability, Capability::Trust(_)))
        .unwrap()
        .grant
        .id
        .clone();
    e.append(
        &chat.space,
        &chat.owner,
        EventBody::GrantRevoked { grant: trust },
    )
    .unwrap();
    req.id = new_id("rq");
    req.content_hash = req.proposal_hash(&chat.space).unwrap();
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::RequestOpened { request: req },
    );
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

#[test]
fn an_approval_cannot_redirect_its_output_and_later_events_cannot_overwrite_or_edit_it_from_another_space(
) {
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
    let r = e.state.requests[&e.requests()[0].id].clone();
    let mut redirected = resolution(&r);
    if let EventBody::RequestResolved {
        resolution: Some(receipt),
        ..
    } = &mut redirected
    {
        receipt.created_item = Some(new_id("it"));
    }
    ingest_signed(&mut e, &chat.space, &chat.owner, redirected);
    assert!(e.items().is_empty());
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r));
    let p = r.req.proposal.as_ref().unwrap();
    let mut replacement = p.content.clone();
    if let ItemContent::Plan(doc) = &mut replacement {
        doc.title = "Unreviewed replacement".into();
    }
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::ItemCreated {
            item: p.item.clone(),
            kind: p.kind,
            content: replacement.clone(),
            origin: "overwrite".into(),
        },
    );
    let other = new_id("sp");
    e.append(
        &other,
        &chat.owner,
        EventBody::SpaceCreated {
            title: "Other Space".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
    .unwrap();
    ingest_signed(
        &mut e,
        &other,
        &chat.owner,
        EventBody::ItemVersioned {
            item: p.item.clone(),
            content: replacement,
            note: "wrong Space".into(),
        },
    );
    assert_eq!(e.item(&p.item).unwrap().title, "Persist this exact plan");
    assert_eq!(e.item(&p.item).unwrap().version, 1);
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.item(&p.item).unwrap().title, "Persist this exact plan");
    assert_eq!(e.item(&p.item).unwrap().version, 1);
}

#[test]
fn plan_approval_after_restart_creates_the_exact_item_and_debits_usage_once() {
    let (chat, mut e) = Chat::create();
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
}

#[test]
fn app_approval_after_restart_creates_an_usable_item_with_its_scoped_grant() {
    let (chat, mut e) = Chat::create();
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
}

#[test]
fn failed_approval_write_leaves_the_proposal_pending_and_identical_retry_succeeds() {
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
}

mod encrypted;
mod review;
