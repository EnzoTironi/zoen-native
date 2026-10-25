use super::*;
use crate::engine::ReqStatus;
use crate::pages::{Currentness, PlanOrigin, PlanReplay, Projection};
use event_log::content_hash;

fn agent(fixture: &mut Fixture) -> std::result::Result<String, Box<dyn std::error::Error>> {
    let agent = fixture
        .engine
        .create_identity(
            Identity {
                id: String::new(),
                kind: IdentityKind::Agent,
                name: "Plan reviewer".into(),
                handle: "reviewer".into(),
                tint_hex: "#778899".into(),
                glyph: None,
                owner: Some(fixture.owner.clone()),
                bio: String::new(),
                owner_proof: None,
            },
            true,
        )
        .unwrap();
    fixture.ordered(EventBody::MemberAdded {
        identity: agent.clone(),
        role: Role::Member,
    })?;
    Ok(agent)
}

fn linked(
    fixture: &Fixture,
    agent: &str,
    document: &PlanDoc,
) -> std::result::Result<AgentRequest, Box<dyn std::error::Error>> {
    let line = &document.sections[0].lines[0];
    Ok(AgentRequest {
        tool_proposal: None,
        id: new_id("linked-request")?,
        agent: agent.into(),
        title: "Owner decision".into(),
        detail: "Exact retained row".into(),
        audience: "This Space".into(),
        action: ActionClass::Money {
            cents: line.cost_cents,
        },
        content_hash: content_hash(&(&line.text, line.cost_cents))?,
        item: Some(fixture.item.clone()),
        line: Some(line.id.clone()),
        proposal: None,
    })
}

fn open(
    fixture: &mut Fixture,
    agent: &str,
    document: &PlanDoc,
) -> std::result::Result<AgentRequest, Box<dyn std::error::Error>> {
    let request = linked(fixture, agent, document)?;
    fixture.ordered_as(
        agent,
        EventBody::RequestOpened {
            request: request.clone(),
        },
    )?;
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    Ok(request)
}

fn decision(request: &AgentRequest, approved: bool) -> EventBody {
    EventBody::RequestResolved {
        request: request.id.clone(),
        approved,
        content_hash: request.content_hash.clone(),
        resolution: Some(RequestResolution {
            tool_decision: None,
            message: "Owner decision recorded".into(),
            created_item: None,
            ai_cost_cents: 0,
            standing_grant: None,
        }),
    }
}

fn canonical(
    fixture: &mut Fixture,
    patch: LinePatch,
) -> std::result::Result<PlanDoc, Box<dyn std::error::Error>> {
    let fold = fixture.fold();
    let document = edited(fold.document.as_ref().unwrap(), patch);
    let result = document.view().unwrap().to_plan_doc();
    fixture
        .ordered(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))))?;
    Ok(result)
}

type MissingBlob = (PageContent, Vec<u8>, crate::media::Wanted, Vec<u8>, PlanDoc);

fn missing_blob(
    fixture: &Fixture,
    patch: LinePatch,
) -> std::result::Result<MissingBlob, Box<dyn std::error::Error>> {
    let fold = fixture.fold();
    let document = edited(fold.document.as_ref().unwrap(), patch);
    let result = document.view().unwrap().to_plan_doc();
    let bytes = document.full_update().to_json().unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let (key, sealed) = crate::media::seal(&bytes)?;
    let blob = hex::encode(Sha256::digest(&sealed));
    let key = hex::encode(key);
    let content = PageContent {
        title: result.title.clone(),
        path: String::new(),
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: sha.clone(),
            bytes: bytes.len() as u64,
            blob: blob.clone(),
            key: key.clone(),
        }),
        manifest: Some(PageManifest::PlanV1(fold.manifest().unwrap())),
    };
    let wanted = crate::media::Wanted {
        plain: sha,
        blob,
        key,
        mime: "application/json".into(),
        space: fixture.space.clone(),
        profile: None,
        maximum_sealed_bytes: None,
    };
    Ok((content, bytes, wanted, sealed, result))
}

#[test]
fn proposal_created_legacy_plans_remain_valid_without_minting_duplicate_origins(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let mut proposal = AgentRequest {
        tool_proposal: None,
        id: new_id("proposal").unwrap(),
        agent: agent.clone(),
        title: "Legacy proposal".into(),
        detail: String::new(),
        audience: "This Space".into(),
        action: ActionClass::Reversible,
        content_hash: String::new(),
        item: None,
        line: None,
        proposal: Some(Box::new(ItemProposal {
            item: new_id("proposal-plan").unwrap(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "retained proposal".into(),
            message: "Created proposal".into(),
            ai_cost_cents: 0,
        })),
    };
    proposal.content_hash = proposal.proposal_hash(&fixture.space).unwrap();
    fixture
        .ordered(EventBody::MemberAdded {
            identity: agent.clone(),
            role: Role::Reader,
        })
        .unwrap();
    fixture
        .ordered_as(
            &agent,
            EventBody::ItemCreated {
                item: proposal.proposal.as_ref().unwrap().item.clone(),
                kind: ItemKind::Plan,
                content: ItemContent::Plan(original()),
                origin: "refused reader creation".into(),
            },
        )
        .unwrap();
    fixture
        .ordered(EventBody::MemberAdded {
            identity: agent.clone(),
            role: Role::Member,
        })
        .unwrap();
    // Identical payload in an owner-signed opening is refused. Its bytes must
    // never stand in for the real certified Agent opening in a shorter prefix.
    fixture
        .ordered(EventBody::RequestOpened {
            request: proposal.clone(),
        })
        .unwrap();
    let refused_opening_end = fixture.engine.logs[&fixture.space].len();
    fixture
        .ordered_as(
            &agent,
            EventBody::RequestOpened {
                request: proposal.clone(),
            },
        )
        .unwrap();
    let item = proposal.proposal.as_ref().unwrap().item.clone();
    let before_resolution = fixture.engine.logs[&fixture.space].len();
    let pending_approval = fixture.queue(EventBody::RequestResolved {
        request: proposal.id.clone(),
        approved: true,
        content_hash: proposal.content_hash.clone(),
        resolution: Some(RequestResolution {
            tool_decision: None,
            message: "Created proposal".into(),
            created_item: Some(item.clone()),
            ai_cost_cents: 0,
            standing_grant: None,
        }),
    });
    assert!(fixture
        .engine
        .plan_timeline(&item)
        .unwrap()
        .origin_tag()
        .is_none());
    assert!(
        fixture.engine.plan_page(&item).is_err(),
        "unconfirmed approval cannot authenticate a Plan session"
    );
    deliver(&mut fixture.engine, &pending_approval, Ingest::Confirmed)?;
    assert_eq!(
        fixture.engine.state.plan_origins[&item],
        PlanOrigin::Proposal
    );
    let read = fixture.engine.plan_page(&item).unwrap();
    assert!(read.ready && read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(193));
    assert_eq!(
        fixture.engine.state.items[&item].versions.initial().author,
        agent
    );
    let scoped = &fixture.engine.space_projections[&fixture.space];
    for end in [refused_opening_end, before_resolution] {
        let (fold, _) = confirmed::Prefix::from_log(
            &fixture.space,
            fixture.engine.logs.get(&fixture.space),
            end,
        )
        .unwrap()
        .with_requests(&scoped.requests)
        .fold(&item, |_| unreachable!())
        .unwrap();
        assert!(
            fold.origin_tag().is_none(),
            "final approval cannot authenticate an earlier prefix"
        );
    }
    fixture.engine.reload().unwrap();
    assert!(
        fixture
            .engine
            .plan_page(&item)
            .unwrap()
            .legacy_edit_admitted
    );
    let explicit_item = fixture.item.clone();
    fixture.item = item.clone();
    let request = open(&mut fixture, &agent, &original()).unwrap();
    fixture.ordered(decision(&request, true)).unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    fixture
        .ordered(EventBody::ItemCreated {
            item: item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "duplicate cannot mint ancestry".into(),
        })
        .unwrap();
    assert_eq!(
        fixture.engine.state.plan_origins[&item],
        PlanOrigin::Proposal
    );
    let observed = fixture.engine.plan_page(&item).unwrap().content_hash;
    fixture
        .engine
        .add_plan_line(&item, 0, "Owner edits approved proposal", 25, &observed)
        .unwrap();
    let pending_edit = fixture.engine.store.outbox().unwrap().into_iter().find(|pending|
        matches!(&pending.event.body, EventBody::ItemVersioned { item: edited, .. } if edited == &item)).unwrap().event;
    deliver(&mut fixture.engine, &pending_edit, Ingest::Confirmed)?;
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.plan_page(&item).unwrap().total_cents,
        Some(218)
    );
    let baseline = fixture.engine.plan_timeline(&explicit_item).unwrap();
    let counterfeit = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Counterfeit origin".into()),
            ..Default::default()
        },
    );
    fixture
        .ordered(fixture.version(ItemContent::Page(page(
            baseline.manifest().unwrap(),
            &counterfeit,
        ))))
        .unwrap();
    let invalid = open(
        &mut fixture,
        &agent,
        &counterfeit.view().unwrap().to_plan_doc(),
    )
    .unwrap();
    fixture.ordered(decision(&invalid, true)).unwrap();
    assert_eq!(
        fixture.engine.state.requests[&invalid.id].status,
        ReqStatus::Pending
    );
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.plan_origins[&item],
        PlanOrigin::Proposal
    );
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    // The genuine two-source ancestry supports a canonical cutover, including
    // cold replay, without an ItemCreated event or changing the Agent creator.
    canonical(
        &mut fixture,
        LinePatch {
            text: Some("Owner-approved canonical plan".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.engine.reload().unwrap();
    let read = fixture.engine.plan_page(&item).unwrap();
    assert!(read.ready);
    assert!(!read.legacy_edit_admitted);
    assert_eq!(
        fixture.engine.state.items[&item].versions.initial().author,
        agent
    );
    assert_eq!(
        fixture
            .engine
            .plan_snapshot_in_space(&fixture.space, &item)
            .unwrap()
            .document
            .sections[0]
            .lines[0]
            .text,
        "Owner-approved canonical plan"
    );
    Ok(())
}

#[test]
fn own_confirmation_rechecks_semantics_and_preserves_profiles_sessions_and_identity(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let request = open(&mut fixture, &agent, &original()).unwrap();
    let pending = fixture.queue(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    let profiles = fixture.engine.identity_order.clone();
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    let epoch = fixture.engine.pages.epoch.clone();
    canonical(
        &mut fixture,
        LinePatch {
            text: Some("Changed before own confirmation".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    let confirmed = deliver(&mut fixture.engine, &pending, Ingest::Confirmed)?;
    assert_eq!(confirmed.client_id, pending.client_id);
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert_eq!(fixture.engine.identity_order, profiles);
    assert!(std::sync::Arc::ptr_eq(&fixture.engine.pages.epoch, &epoch));
    assert_eq!(
        fixture.engine.plan_page(&fixture.item).unwrap().item_id,
        read.item_id
    );
    assert!(!fixture.engine.net.pending.contains_key(&pending.client_id));
    assert!(fixture.engine.store.outbox().unwrap().is_empty());
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    Ok(())
}

mod history_cases;

mod payload_cases;

mod scope_cases;
