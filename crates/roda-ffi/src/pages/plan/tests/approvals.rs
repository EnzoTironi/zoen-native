use super::*;
use crate::engine::ReqStatus;
use crate::pages::{Currentness, PlanOrigin, PlanReplay, Projection};
use roda_log::content_hash;

fn agent(fixture: &mut Fixture) -> String {
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
    });
    agent
}

fn linked(fixture: &Fixture, agent: &str, document: &PlanDoc) -> AgentRequest {
    let line = &document.sections[0].lines[0];
    AgentRequest {
        id: new_id("linked-request"),
        agent: agent.into(),
        title: "Owner decision".into(),
        detail: "Exact retained row".into(),
        audience: "This Space".into(),
        action: ActionClass::Money {
            cents: line.cost_cents,
        },
        content_hash: content_hash(&(&line.text, line.cost_cents)),
        item: Some(fixture.item.clone()),
        line: Some(line.id.clone()),
        proposal: None,
    }
}

fn open(fixture: &mut Fixture, agent: &str, document: &PlanDoc) -> AgentRequest {
    let request = linked(fixture, agent, document);
    fixture.ordered_as(
        agent,
        EventBody::RequestOpened {
            request: request.clone(),
        },
    );
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    request
}

fn decision(request: &AgentRequest, approved: bool) -> EventBody {
    EventBody::RequestResolved {
        request: request.id.clone(),
        approved,
        content_hash: request.content_hash.clone(),
        resolution: Some(RequestResolution {
            message: "Owner decision recorded".into(),
            created_item: None,
            ai_cost_cents: 0,
            standing_grant: None,
        }),
    }
}

fn canonical(fixture: &mut Fixture, patch: LinePatch) -> PlanDoc {
    let fold = fixture.fold();
    let document = edited(fold.document.as_ref().unwrap(), patch);
    let result = document.view().unwrap().to_plan_doc();
    fixture.ordered(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))));
    result
}

fn missing_blob(
    fixture: &Fixture,
    patch: LinePatch,
) -> (PageContent, Vec<u8>, crate::media::Wanted, Vec<u8>, PlanDoc) {
    let fold = fixture.fold();
    let document = edited(fold.document.as_ref().unwrap(), patch);
    let result = document.view().unwrap().to_plan_doc();
    let bytes = document.full_update().to_json().unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let (key, sealed) = crate::media::seal(&bytes);
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
    };
    (content, bytes, wanted, sealed, result)
}

#[test]
fn canonical_linked_approval_matches_live_and_cold_replay_before_later_edits() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let document = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Approved canonical row".into()),
            cost_cents: Some(317),
            ..Default::default()
        },
    );
    let request = open(&mut fixture, &agent, &document);
    assert!(
        fixture.engine.resolve_request(&request.id, true).is_err(),
        "local canonical writes stay closed"
    );
    let accepted = fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    canonical(
        &mut fixture,
        LinePatch {
            text: Some("Later replacement".into()),
            cost_cents: Some(999),
            ..Default::default()
        },
    );
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    fixture.engine.reload().unwrap();
    let replay = &fixture.engine.state.requests[&request.id];
    assert_eq!(replay.status, ReqStatus::Approved);
    assert_eq!(replay.resolved_ms, Some(accepted.at_ms));
    assert_eq!(fixture.engine.state.resolved_order, vec![request.id]);
}

#[test]
fn a_later_matching_edit_does_not_repair_a_refused_historical_approval() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let changed = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Changed before decision".into()),
            ..Default::default()
        },
    );
    let request = open(&mut fixture, &agent, &original());
    let refused = fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    canonical(
        &mut fixture,
        LinePatch {
            text: Some(original().sections[0].lines[0].text.clone()),
            ..Default::default()
        },
    );
    assert_ne!(changed, original());
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture.engine.state.resolved_order.is_empty());
    assert!(fixture.engine.logs[&fixture.space]
        .events()
        .iter()
        .any(|event| event.hash == refused.hash));
}

#[test]
fn removed_duplicate_lines_use_the_first_remaining_exact_external_id() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let request = open(&mut fixture, &agent, &original());
    let fold = fixture.fold();
    let mut document = fold.document.as_ref().unwrap().fork().unwrap();
    let first = document.view().unwrap().sections[0].lines[0]
        .block_id
        .clone();
    document
        .apply(PlanCommand::RemoveLine {
            block: first,
            expected: ExpectedLine::AnyCurrent,
        })
        .unwrap();
    fixture.ordered(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))));
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    let remaining = document.view().unwrap().to_plan_doc();
    let credit = open(&mut fixture, &agent, &remaining);
    fixture.ordered(decision(&credit, true));
    assert_eq!(
        fixture.engine.state.requests[&credit.id].status,
        ReqStatus::Approved
    );
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert_eq!(
        fixture.engine.state.requests[&credit.id].status,
        ReqStatus::Approved
    );
}

#[test]
fn optimistic_canonical_pages_cannot_supply_confirmed_approval_content() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let fold = fixture.fold();
    let document = edited(
        fold.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Only pending".into()),
            ..Default::default()
        },
    );
    let request = open(
        &mut fixture,
        &agent,
        &document.view().unwrap().to_plan_doc(),
    );
    let pending = fixture
        .queue(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))));
    let baseline = open(&mut fixture, &agent, &original());
    let queued = fixture.engine.store.outbox().unwrap().len();
    let author = fixture.engine.author_for(&fixture.owner).unwrap();
    assert!(fixture
        .engine
        .append_synced(
            &fixture.space,
            &author,
            1_800_000_000_000,
            decision(&baseline, true)
        )
        .is_err());
    assert_eq!(fixture.engine.store.outbox().unwrap().len(), queued);
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture
        .engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .any(|entry| entry.event.client_id == pending.client_id));
    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    assert!(fixture.engine.state.resolved_order.is_empty());
}

#[test]
fn missing_and_damaged_blobs_wait_without_stale_status_until_verified_repair() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, sealed, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Arriving row".into()),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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
fn waiting_approval_holds_later_denial_grants_messages_and_usage_in_order() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, _, document) = missing_blob(
        &fixture,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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
fn a_standalone_denial_before_waiting_does_not_read_the_plan_payload() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let (page, _, _, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Missing".into()),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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
fn malformed_page_descriptors_are_refused_without_waiting_or_approval_effects() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let (mut page, _, _, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Untrusted".into()),
            ..Default::default()
        },
    );
    page.blob.as_mut().unwrap().bytes = u64::MAX;
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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
fn unauthorized_owner_hash_and_receipt_checks_precede_missing_payload_reads() {
    let mut fixture = Fixture::new();
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
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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
fn storage_errors_preserve_the_stable_projection_and_raw_decision_for_retry() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let (page, bytes, wanted, _, document) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Infrastructure wait".into()),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    let request = open(&mut fixture, &agent, &document);
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

#[test]
fn exact_text_and_cents_select_the_first_duplicate_id_while_done_is_not_hashed() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let mut legacy = original();
    legacy.sections[0].lines[0].cost_cents = -17;
    fixture.ordered(fixture.version(ItemContent::Plan(legacy)));
    let document = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Cafe\u{301}\r\n😀".into()),
            cost_cents: None,
            done: Some(true),
        },
    );
    let request = open(&mut fixture, &agent, &document);
    let mut second = linked(&fixture, &agent, &document);
    let second_line = &document.sections[0].lines[1];
    second.content_hash = content_hash(&(&second_line.text, second_line.cost_cents));
    fixture.ordered_as(
        &agent,
        EventBody::RequestOpened {
            request: second.clone(),
        },
    );
    fixture.ordered(decision(&second, true));
    assert_eq!(
        fixture.engine.state.requests[&second.id].status,
        ReqStatus::Pending
    );
    canonical(
        &mut fixture,
        LinePatch {
            done: Some(false),
            ..Default::default()
        },
    );
    fixture.ordered(decision(&request, true));
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
fn proposal_created_legacy_plans_remain_valid_without_minting_duplicate_origins() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let mut proposal = AgentRequest {
        id: new_id("proposal"),
        agent: agent.clone(),
        title: "Legacy proposal".into(),
        detail: String::new(),
        audience: "This Space".into(),
        action: ActionClass::Reversible,
        content_hash: String::new(),
        item: None,
        line: None,
        proposal: Some(Box::new(ItemProposal {
            item: new_id("proposal-plan"),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "retained proposal".into(),
            message: "Created proposal".into(),
            ai_cost_cents: 0,
        })),
    };
    proposal.content_hash = proposal.proposal_hash(&fixture.space).unwrap();
    fixture.ordered_as(
        &agent,
        EventBody::RequestOpened {
            request: proposal.clone(),
        },
    );
    let item = proposal.proposal.as_ref().unwrap().item.clone();
    fixture.ordered(EventBody::RequestResolved {
        request: proposal.id.clone(),
        approved: true,
        content_hash: proposal.content_hash.clone(),
        resolution: Some(RequestResolution {
            message: "Created proposal".into(),
            created_item: Some(item.clone()),
            ai_cost_cents: 0,
            standing_grant: None,
        }),
    });
    assert_eq!(
        fixture.engine.state.plan_origins[&item],
        PlanOrigin::Proposal
    );
    let explicit_item = fixture.item.clone();
    fixture.item = item.clone();
    let request = open(&mut fixture, &agent, &original());
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    fixture.ordered(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "duplicate cannot mint ancestry".into(),
    });
    assert_eq!(
        fixture.engine.state.plan_origins[&item],
        PlanOrigin::Proposal
    );
    let baseline = fixture.engine.plan_timeline(&explicit_item).unwrap();
    let counterfeit = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Counterfeit origin".into()),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page(
        baseline.manifest().unwrap(),
        &counterfeit,
    ))));
    let invalid = open(
        &mut fixture,
        &agent,
        &counterfeit.view().unwrap().to_plan_doc(),
    );
    fixture.ordered(decision(&invalid, true));
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
}

#[test]
fn own_confirmation_rechecks_semantics_and_preserves_profiles_sessions_and_identity() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let request = open(&mut fixture, &agent, &original());
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
    );
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    let confirmed = deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
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
}

#[test]
fn cross_space_ambiguity_closes_local_writes_but_preserves_scoped_confirmed_decisions() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let document = canonical(
        &mut fixture,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let request = open(&mut fixture, &agent, &document);
    let other = new_id("other-space");
    let author = fixture.engine.author_for(&fixture.owner).unwrap();
    for body in [
        EventBody::SpaceCreated {
            title: "Other Space".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Public,
        },
        EventBody::ItemCreated {
            item: fixture.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "Collision".into(),
        },
    ] {
        let signed = author.sign_event(
            &other,
            &new_id("collision"),
            1,
            fixture.engine.logs.get(&other).and_then(|log| log.head()),
            body,
        );
        deliver(&mut fixture.engine, &signed, Ingest::Applied);
    }
    let raw = fixture.engine.store.events(&fixture.space).unwrap();
    let outbox = fixture.engine.store.outbox().unwrap();
    assert!(!fixture.engine.state.items.contains_key(&fixture.item));
    assert!(fixture.engine.plan_timeline(&fixture.item).is_err());
    assert!(fixture.engine.card(&fixture.item).is_none());
    assert!(fixture.engine.resolve_request(&request.id, true).is_err());
    assert!(fixture
        .engine
        .append_synced(
            &fixture.space,
            &author,
            1_800_000_000_000,
            decision(&request, true),
        )
        .is_err());
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), raw);
    assert_eq!(fixture.engine.store.outbox().unwrap().len(), outbox.len());
    for pending in outbox {
        assert_eq!(
            fixture
                .engine
                .store
                .outbox_get(&pending.event.client_id)
                .unwrap(),
            Some(pending.event)
        );
    }
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );

    // An ordered decision has only this Space's authenticated prefix. The wire
    // does not attest whether the unrelated Space was observed before or after it.
    fixture.ordered(decision(&request, true));
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Approved
    );
    assert_eq!(
        fixture.engine.state.resolved_order,
        vec![request.id.clone()]
    );
    assert_eq!(
        fixture
            .engine
            .plan_snapshot_in_space(&fixture.space, &fixture.item)
            .unwrap()
            .document,
        document
    );
    assert!(fixture.engine.timeline(&fixture.space).unwrap().iter().any(|entry| {
        matches!(&entry.kind, crate::dto::EntryKind::Request { request_id, approved: Some(true), .. } if request_id == &request.id)
    }));
}

#[test]
fn contexts_are_bound_to_the_exact_decision_and_do_not_read_later_payloads() {
    let mut fixture = Fixture::new();
    let agent = agent(&mut fixture);
    let request = open(&mut fixture, &agent, &original());
    let event = fixture.ordered(decision(&request, true));
    let end = fixture.engine.logs[&fixture.space].len() - 1;
    let (page, _, _, _, _) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Later missing payload".into()),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page)));
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE media RENAME TO temporarily_unavailable_media")
        .unwrap();
    let mut state = crate::engine::State::default();
    let mut plans = PlanReplay::new(&fixture.engine.store, &fixture.engine.logs);
    let log = &fixture.engine.logs[&fixture.space];
    for (index, prior) in log.events()[..end].iter().enumerate() {
        let context = plans
            .before(&state, &fixture.engine.identities, prior, index)
            .unwrap();
        assert_ne!(
            state.apply_checked(prior, &fixture.engine.identities, &context),
            Projection::Waiting
        );
    }
    let context = plans
        .before(&state, &fixture.engine.identities, &event, end)
        .unwrap();
    let mut substituted = event.clone();
    substituted.client_id = "substituted".into();
    assert_eq!(
        state.apply_checked(&substituted, &fixture.engine.identities, &context),
        Projection::Refused
    );
    for substituted in [
        Event {
            seq: event.seq + 1,
            ..event.clone()
        },
        Event {
            prev: "substituted".into(),
            ..event.clone()
        },
        Event {
            hash: "substituted".into(),
            ..event.clone()
        },
        Event {
            at_ms: event.at_ms + 1,
            ..event.clone()
        },
    ] {
        assert!(!context.matches(&substituted));
        assert!(plans
            .before(&state, &fixture.engine.identities, &substituted, end)
            .is_err());
        assert_eq!(
            state.apply_checked(&substituted, &fixture.engine.identities, &context),
            Projection::Refused
        );
        assert_eq!(state.requests[&request.id].status, ReqStatus::Pending);
        assert!(state.resolved_order.is_empty());
    }
    assert!(plans
        .before(&state, &fixture.engine.identities, &event, end + 1)
        .is_err());
    assert_eq!(
        state.apply_checked(&event, &fixture.engine.identities, &context),
        Projection::Applied
    );
    assert_eq!(state.requests[&request.id].status, ReqStatus::Approved);
    fixture
        .engine
        .store
        .conn()
        .execute_batch("ALTER TABLE temporarily_unavailable_media RENAME TO media")
        .unwrap();
}

#[test]
fn later_foreign_creations_do_not_rewrite_the_scoped_approved_decision() {
    for reader in [true, false] {
        for at_ms in [1, 1_900_000_000_000] {
            let directory = std::env::temp_dir().join(new_id("late-foreign-plan-origin"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("plan.sqlite");
            let mut fixture = Fixture::at(path.to_str().unwrap());
            let agent = agent(&mut fixture);
            let document = canonical(
                &mut fixture,
                LinePatch {
                    text: Some("Exact historically approved row".into()),
                    cost_cents: Some(317),
                    ..Default::default()
                },
            );
            let request = open(&mut fixture, &agent, &document);
            let accepted = fixture.ordered(decision(&request, true));
            assert_eq!(
                fixture.engine.state.requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture.engine.state.resolved_order,
                vec![request.id.clone()]
            );
            let original_raw = fixture.engine.store.events(&fixture.space).unwrap();
            let other = new_id("unrelated-space");
            let author = fixture.engine.author_for(&fixture.owner).unwrap();
            let mut bodies = vec![EventBody::SpaceCreated {
                title: "Unrelated later history".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Public,
            }];
            if reader {
                bodies.push(EventBody::MemberAdded {
                    identity: fixture.owner.clone(),
                    role: Role::Reader,
                });
            }
            bodies.push(EventBody::ItemCreated {
                item: fixture.item.clone(),
                kind: ItemKind::Plan,
                content: ItemContent::Plan(original()),
                origin: "Refused duplicate".into(),
            });
            for body in bodies {
                let signed = author.sign_event(
                    &other,
                    &new_id("late-foreign-event"),
                    at_ms,
                    fixture.engine.logs.get(&other).and_then(|log| log.head()),
                    body,
                );
                deliver(&mut fixture.engine, &signed, Ingest::Applied);
            }
            assert_eq!(
                fixture.engine.store.events(&fixture.space).unwrap(),
                original_raw
            );
            assert_eq!(
                fixture.engine.state.requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture
                    .engine
                    .plan_snapshot_in_space(&fixture.space, &fixture.item)
                    .unwrap()
                    .document,
                document
            );
            assert_eq!(
                fixture.engine.space_projections[&fixture.space].requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture.engine.state.requests[&request.id].resolved_ms,
                Some(accepted.at_ms)
            );
            assert_eq!(
                fixture.engine.state.resolved_order,
                vec![request.id.clone()]
            );
            fixture.engine.reload().unwrap();
            assert_eq!(
                fixture.engine.state.requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture
                    .engine
                    .plan_snapshot_in_space(&fixture.space, &fixture.item)
                    .unwrap()
                    .document,
                document
            );
            assert_eq!(
                fixture.engine.space_projections[&fixture.space].requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture.engine.state.requests[&request.id].resolved_ms,
                Some(accepted.at_ms)
            );
            assert_eq!(
                fixture.engine.state.resolved_order,
                vec![request.id.clone()]
            );
            let own_space = fixture.space.clone();
            drop(fixture);
            let reopened = Engine::open(path.to_str().unwrap()).unwrap();
            assert_eq!(reopened.store.events(&own_space).unwrap(), original_raw);
            assert_eq!(
                reopened.state.requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                reopened
                    .plan_snapshot_in_space(&own_space, request.item.as_deref().unwrap())
                    .unwrap()
                    .document,
                document
            );
            assert_eq!(
                reopened.state.requests[&request.id].resolved_ms,
                Some(accepted.at_ms)
            );
            assert_eq!(reopened.state.resolved_order, vec![request.id.clone()]);
            drop(reopened);
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}

#[test]
fn duplicate_request_ids_keep_each_space_history_and_close_unscoped_writes() {
    for at_ms in [1, 1_900_000_000_000] {
        let mut fixture = Fixture::new();
        let agent = agent(&mut fixture);
        let document = canonical(
            &mut fixture,
            LinePatch {
                text: Some("Own authenticated row".into()),
                ..Default::default()
            },
        );
        let request = open(&mut fixture, &agent, &document);
        let owner = fixture.engine.author_for(&fixture.owner).unwrap();
        let agent_author = fixture.engine.author_for(&agent).unwrap();
        let other = new_id("duplicate-request-space");
        let other_item = new_id("different-plan");
        let mut foreign_document = original();
        foreign_document.title = "Different scoped Plan".into();
        foreign_document.sections[0].lines[0].text = "Unrelated row".into();
        foreign_document.sections[0].lines[0].cost_cents = 987;
        let line = &foreign_document.sections[0].lines[0];
        let mut foreign_request = request.clone();
        foreign_request.title = "Unrelated request".into();
        foreign_request.item = Some(other_item.clone());
        foreign_request.content_hash = content_hash(&(&line.text, line.cost_cents));
        for body in [
            EventBody::SpaceCreated {
                title: "Separate request history".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Public,
            },
            EventBody::MemberAdded {
                identity: agent.clone(),
                role: Role::Member,
            },
            EventBody::ItemCreated {
                item: other_item,
                kind: ItemKind::Plan,
                content: ItemContent::Plan(foreign_document),
                origin: "Separate authenticated origin".into(),
            },
            EventBody::RequestOpened {
                request: foreign_request.clone(),
            },
            decision(&foreign_request, false),
        ] {
            let author = if matches!(&body, EventBody::RequestOpened { .. }) {
                &agent_author
            } else {
                &owner
            };
            let event = author.sign_event(
                &other,
                &new_id("separate-request-event"),
                at_ms,
                fixture.engine.logs.get(&other).and_then(|log| log.head()),
                body,
            );
            deliver(&mut fixture.engine, &event, Ingest::Applied);
        }
        assert!(!fixture.engine.state.requests.contains_key(&request.id));
        let raw = fixture.engine.store.events(&fixture.space).unwrap();
        let outbox = fixture.engine.store.outbox().unwrap().len();
        assert!(fixture.engine.resolve_request(&request.id, true).is_err());
        assert!(fixture
            .engine
            .append_synced(
                &fixture.space,
                &owner,
                1_800_000_000_000,
                decision(&request, true),
            )
            .is_err());
        assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), raw);
        assert_eq!(fixture.engine.store.outbox().unwrap().len(), outbox);

        fixture.ordered(decision(&request, true));
        fixture.engine.reload().unwrap();
        assert!(!fixture.engine.state.requests.contains_key(&request.id));
        let own = &fixture.engine.space_projections[&fixture.space].requests[&request.id];
        let foreign = &fixture.engine.space_projections[&other].requests[&request.id];
        assert_eq!(own.req, request);
        assert_eq!(own.status, ReqStatus::Approved);
        assert_eq!(foreign.req, foreign_request);
        assert_eq!(foreign.status, ReqStatus::Denied);
        assert_eq!(
            fixture.engine.space_projections[&fixture.space].resolved_order,
            vec![request.id.clone()]
        );
        assert_eq!(
            fixture.engine.space_projections[&other].resolved_order,
            vec![request.id.clone()]
        );
        for (space, title, approved) in [
            (&fixture.space, request.title.as_str(), true),
            (&other, foreign_request.title.as_str(), false),
        ] {
            assert!(fixture.engine.timeline(space).unwrap().iter().any(|entry| {
                matches!(&entry.kind, crate::dto::EntryKind::Request { request_id, title: visible_title, approved: Some(value) }
                    if request_id == &request.id && visible_title == title && *value == approved)
            }));
        }
    }
}
