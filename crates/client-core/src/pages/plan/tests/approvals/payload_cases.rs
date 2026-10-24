//! payload cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn missing_and_damaged_blobs_wait_without_stale_status_until_verified_repair() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, sealed, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Arriving row".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    fixture.ordered(decision(&request, true));
    assert!(fixture.engine.state.waiting_spaces.contains(&fixture.space));
    assert_eq!(
        fixture.engine.request_currentness(&request).unwrap(),
        Currentness::Waiting
    );
    assert_eq!(
        fixture
            .engine
            .request_dto(&fixture.engine.state.requests[&request.id])
            .status,
        crate::dto::RequestStatus::Pending
    );
    assert!(!fixture
        .engine
        .import_media(&wanted.plain, b"wrong bytes", "application/json")
        .unwrap());
    fixture
        .engine
        .store
        .conn()
        .execute(
            "INSERT INTO media (sha256,mime,bytes,at_ms) VALUES (?1,'application/json',?2,0)",
            rusqlite::params![wanted.plain, vec![0_u8; bytes.len()]],
        )
        .unwrap();
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    let mut wrong = wanted.clone();
    wrong.key = "11".repeat(32);
    assert!(!fixture.engine.media_arrived(&wrong, &sealed));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture.engine.media_arrived(&wanted, &sealed));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
}

#[test]
pub(super) fn waiting_approval_holds_later_denial_grants_messages_and_usage_in_order() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, _, document) = missing_blob(
        &fixture,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    let first = fixture.ordered(decision(&request, true));
    let denial = fixture.ordered(decision(&request, false));
    let owner = fixture.owner.clone();
    fixture.ordered(EventBody::GrantIssued {
        grant: Grant {
            id: "held-grant".into(),
            grantor: owner,
            grantee: Some(agent.clone()),
            scope: GrantScope::Space(fixture.space.clone()),
            capability: Capability::Trust(TrustLevel::Listen),
            expires_at_ms: None,
        },
    });
    fixture.ordered_as(
        &agent,
        EventBody::UsageRecorded {
            agent: agent.clone(),
            cents: 3,
            what: "held".into(),
        },
    );
    fixture.ordered(EventBody::MessagePosted {
        message: "held-message".into(),
        text: "Held suffix".into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    });
    assert!(fixture.engine.state.grants.is_empty());
    assert!(fixture.engine.state.usage.is_empty());
    assert!(fixture.engine.state.resolved_order.is_empty());
    assert!(!fixture.engine.state.spaces[&fixture.space]
        .entries
        .iter()
        .any(|entry| entry.client_id == denial.client_id));
    assert!(fixture
        .engine
        .import_media(&wanted.plain, &bytes, "application/json")
        .unwrap());
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    assert_eq!(
        fixture.engine.state.requests[&request.id].resolved_ms,
        Some(first.at_ms)
    );
    assert_eq!(fixture.engine.state.resolved_order, vec![request.id]);
    assert_eq!(fixture.engine.state.grants[0].grant.id, "held-grant");
    assert_eq!(fixture.engine.state.usage.len(), 1);
    assert!(!fixture.engine.state.spaces[&fixture.space]
        .entries
        .iter()
        .any(|entry| entry.client_id == denial.client_id));
    assert!(fixture.engine.state.spaces[&fixture.space].entries.iter().any(|entry| matches!(&entry.body, crate::engine::EntryBody::Message { text, .. } if text == "Held suffix")));
}

#[test]
pub(super) fn a_standalone_denial_before_waiting_does_not_read_the_plan_payload() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let (page, _, _, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Missing".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE media RENAME TO temporarily_unavailable_media")
        .unwrap();
    fixture.ordered(decision(&request, false));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Denied
    );
    assert!(fixture.engine.state.waiting_spaces.is_empty());
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE temporarily_unavailable_media RENAME TO media")
        .unwrap();
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Denied
    );
}

#[test]
pub(super) fn malformed_page_descriptors_are_refused_without_waiting_or_approval_effects() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let (mut page, _, _, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Untrusted".into()),
            ..Default::default()
        },
    )
    .unwrap();
    page.blob.as_mut().unwrap().bytes = u64::MAX;
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture.engine.state.waiting_spaces.is_empty());
    fixture.ordered(decision(&request, false));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Denied
    );
}

#[test]
pub(super) fn unauthorized_owner_hash_and_receipt_checks_precede_missing_payload_reads() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let intruder = add_identity(&mut fixture.engine, "Other writer");
    fixture.ordered(EventBody::MemberAdded {
        identity: intruder.clone(),
        role: Role::Member,
    });
    let (page, _, _, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Owner only".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE media RENAME TO temporarily_unavailable_media")
        .unwrap();
    fixture.ordered_as(&intruder, decision(&request, true));
    let mut bad_hash = decision(&request, true);
    if let EventBody::RequestResolved { content_hash, .. } = &mut bad_hash {
        *content_hash = "wrong hash".into();
    }
    fixture.ordered(bad_hash);
    let mut bad_receipt = decision(&request, true);
    if let EventBody::RequestResolved {
        resolution: Some(receipt),
        ..
    } = &mut bad_receipt
    {
        receipt.ai_cost_cents = 1;
    }
    fixture.ordered(bad_receipt);
    fixture.ordered(EventBody::MemberRemoved {
        identity: agent.clone(),
    });
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture.engine.state.waiting_spaces.is_empty());
    assert!(fixture.engine.state.grants.is_empty());
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE temporarily_unavailable_media RENAME TO media")
        .unwrap();
}

#[test]
pub(super) fn storage_errors_preserve_the_stable_projection_and_raw_decision_for_retry() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Infrastructure wait".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document).unwrap();
    let before: Vec<_> = fixture.engine.state.spaces[&fixture.space]
        .entries
        .iter()
        .map(|entry| entry.client_id.clone())
        .collect();
    let signed = fixture.signed_as(&fixture.owner, decision(&request, true));
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE media RENAME TO temporarily_unavailable_media")
        .unwrap();
    let incoming = frame(&fixture.engine, &signed);
    assert!(matches!(
        fixture.engine.ingest(incoming.clone()),
        Ingest::Invalid(_)
    ));
    assert!(fixture.engine.approval_replay_needed);
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert_eq!(
        fixture.engine.state.spaces[&fixture.space]
            .entries
            .iter()
            .map(|entry| entry.client_id.clone())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        fixture.engine.logs[&fixture.space]
            .events()
            .last()
            .unwrap()
            .client_id,
        signed.client_id
    );
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE temporarily_unavailable_media RENAME TO media")
        .unwrap();
    assert_eq!(fixture.engine.ingest(incoming), Ingest::Duplicate);
    assert!(fixture.engine.state.waiting_spaces.contains(&fixture.space));
    assert!(!fixture.engine.approval_replay_needed);
    assert!(fixture
        .engine
        .import_media(&wanted.plain, &bytes, "application/json")
        .unwrap());
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
}
