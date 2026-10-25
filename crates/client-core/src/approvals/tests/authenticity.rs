//! Whole authenticity cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn authenticated_wrong_owner_hash_space_and_duplicate_decisions_have_no_effect_on_ingest_or_replay(
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
    let other_space = new_id("sp")?;
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
    ingest_signed(&mut e, &chat.space, &stranger, resolution(&r))?;
    let mut wrong_hash = resolution(&r);
    if let EventBody::RequestResolved { content_hash, .. } = &mut wrong_hash {
        *content_hash = "0".repeat(64);
    }
    ingest_signed(&mut e, &chat.space, &chat.owner, wrong_hash)?;
    ingest_signed(&mut e, &other_space, &chat.owner, resolution(&r))?;
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    drop(e);
    let mut e = chat.reopen();
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r))?;
    assert_eq!(e.items().len(), 1);
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r))?;
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
    ingest_signed(&mut e, &chat.space, &chat.owner, deny)?;
    assert_eq!(e.state.requests[&id].status, ReqStatus::Approved);
    assert_eq!(e.state.resolved_order.len(), 1);
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.items().len(), 1);
    assert_eq!(e.state.resolved_order.len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    assert!(e.verify_all().iter().all(|r| r.valid));

    Ok(())
}

pub(super) fn changed_proposals_impersonated_requests_and_forged_usage_are_not_projected(
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
    let r = e.state.requests[&e.requests()[0].id].clone();
    let mut changed = r.req.clone();
    changed.id = new_id("rq")?;
    changed.content_hash = changed.proposal_hash(&chat.space).unwrap();
    if let ItemContent::Plan(p) = &mut changed.proposal.as_mut().unwrap().content {
        p.sections[0].lines[0].text = "Unreviewed replacement".into();
    }
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::RequestOpened { request: changed },
    )?;
    let mut impersonated = r.req.clone();
    impersonated.id = new_id("rq")?;
    impersonated.content_hash = impersonated.proposal_hash(&chat.space).unwrap();
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::RequestOpened {
            request: impersonated,
        },
    )?;
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::UsageRecorded {
            agent: chat.agent.clone(),
            cents: 500,
            what: "forged charge".into(),
        },
    )?;
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::UsageRecorded {
            agent: chat.agent.clone(),
            cents: -500,
            what: "negative charge".into(),
        },
    )?;
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));

    Ok(())
}

pub(super) fn claiming_an_owner_without_their_signed_grant_cannot_charge_for_a_proposal(
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
    req.id = new_id("rq")?;
    req.content_hash = req.proposal_hash(&chat.space).unwrap();
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.agent,
        EventBody::RequestOpened { request: req },
    )?;
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.requests().len(), 1);
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));

    Ok(())
}

pub(super) fn an_approval_cannot_redirect_its_output_and_later_events_cannot_overwrite_or_edit_it_from_another_space(
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
    let r = e.state.requests[&e.requests()[0].id].clone();
    let mut redirected = resolution(&r);
    if let EventBody::RequestResolved {
        resolution: Some(receipt),
        ..
    } = &mut redirected
    {
        receipt.created_item = Some(new_id("it")?);
    }
    ingest_signed(&mut e, &chat.space, &chat.owner, redirected)?;
    assert!(e.items().is_empty());
    ingest_signed(&mut e, &chat.space, &chat.owner, resolution(&r))?;
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
    )?;
    let other = new_id("sp")?;
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
    )?;
    assert_eq!(e.item(&p.item).unwrap().title, "Persist this exact plan");
    assert_eq!(e.item(&p.item).unwrap().version, 1);
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.item(&p.item).unwrap().title, "Persist this exact plan");
    assert_eq!(e.item(&p.item).unwrap().version, 1);

    Ok(())
}
