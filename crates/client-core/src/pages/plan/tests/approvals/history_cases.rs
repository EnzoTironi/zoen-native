//! history cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn device_history_recovery_releases_waiting_membership_and_its_deferred_receipt() {
    use crate::link::{DeviceMsg, Peer};
    use event_log::content::{Sealed, SealedKind};
    use event_log::content_hash_of;
    use std::collections::HashSet;

    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let newcomer = add_identity(&mut fixture.engine, "Recovered member");
    let (page, _, _, _, document) = missing_blob(&fixture, LinePatch::default()).unwrap();
    fixture
        .ordered(fixture.version(ItemContent::Page(page)))
        .unwrap();
    let request = linked(&fixture, &agent, &document).unwrap();

    // Retain a real authenticated opaque application, as a linked device does
    // before its sibling sends opened history. Membership itself stays clear.
    let mut earlier = request.clone();
    earlier.content_hash = content_hash(&"earlier request binding");
    let author = fixture.engine.author_for(&agent).unwrap();
    let client = new_id("opaque-request").unwrap();
    let at = 1_800_000_000_000;
    let seen = fixture.engine.logs[&fixture.space].head();
    let mut opened = author.sign_event(
        &fixture.space,
        &client,
        at,
        seen.clone(),
        EventBody::RequestOpened {
            request: earlier.clone(),
        },
    );
    let (content, sig) = author.sign_sealed(
        &fixture.space,
        &client,
        at,
        seen.as_ref(),
        Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
    );
    let outer = event_log::event_from_content(
        content,
        sig,
        author.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let retained = fixture
        .engine
        .logs
        .get_mut(&fixture.space)
        .unwrap()
        .sequence(outer)
        .clone();
    fixture.engine.store.append_event(&retained).unwrap();
    fixture.engine.reload().unwrap();
    opened.seq = retained.seq;
    opened.sealed_wire = Some(content_hash_of(&retained));

    fixture
        .ordered_as(
            &agent,
            EventBody::RequestOpened {
                request: request.clone(),
            },
        )
        .unwrap();
    fixture.ordered(decision(&request, true)).unwrap();
    assert!(fixture.engine.state.waiting_spaces.contains(&fixture.space));
    fixture
        .ordered(EventBody::MemberAdded {
            identity: newcomer.clone(),
            role: Role::Member,
        })
        .unwrap();
    assert!(!fixture
        .engine
        .receipt_scope(&fixture.space, &newcomer)
        .unwrap());
    let mut receipts = crate::net::DeferredReceipts::default();
    let mut dirty = HashSet::new();
    assert!(!receipts
        .receive(
            &mut fixture.engine,
            &fixture.space,
            &newcomer,
            0,
            retained.seq
        )
        .unwrap());
    receipts.flush(&mut fixture.engine, &mut dirty).unwrap();
    assert!(dirty.is_empty());
    assert!(fixture.engine.store.peer_receipts().unwrap().is_empty());

    let receiver = event_log::Signer::generate().unwrap();
    let sender = event_log::Signer::generate().unwrap();
    let owner = fixture.engine.signers[&fixture.owner].clone();
    let device_author = event_log::Author::device(&owner, receiver.clone());
    fixture
        .engine
        .save_linked_account(crate::sync::AccountMeta {
            identity: fixture.owner.clone(),
            device: receiver.id(),
            cert: device_author.cert.unwrap(),
            relay_url: "https://synthetic.invalid".into(),
            registered: true,
        })
        .unwrap();
    let (receiver_secret, receiver_public) = crate::link::key_pair();
    let (sender_secret, sender_public) = crate::link::key_pair();
    let secret = hex::encode(crate::link::random::<32>());
    fixture.engine.add_peer(Peer {
        device: sender.id(),
        their_hpke: hex::encode(sender_public),
        my_hpke_secret: hex::encode(receiver_secret),
        secret: secret.clone(),
    });
    let sender_peer = Peer {
        device: receiver.id(),
        their_hpke: hex::encode(receiver_public),
        my_hpke_secret: hex::encode(sender_secret),
        secret,
    };
    let page = sender_peer
        .seal(&DeviceMsg::Page {
            req: new_id("history-page").unwrap(),
            space: fixture.space.clone(),
            events: vec![opened],
            more: false,
        })
        .unwrap();
    receipts
        .device_message(
            &mut fixture.engine,
            &sender.id(),
            &receiver.id(),
            &page,
            &mut dirty,
        )
        .unwrap();
    assert!(!fixture.engine.state.waiting_spaces.contains(&fixture.space));
    assert_eq!(
        fixture.engine.state.requests[&request.id].req.content_hash,
        earlier.content_hash
    );
    assert!(fixture
        .engine
        .receipt_scope(&fixture.space, &newcomer)
        .unwrap());
    assert_eq!(dirty, HashSet::from([fixture.space.clone()]));
    assert_eq!(
        fixture.engine.net.receipts[&fixture.space][&newcomer],
        (retained.seq, retained.seq)
    );
    assert_eq!(fixture.engine.store.peer_receipts().unwrap().len(), 1);
    assert!(fixture.engine.logs[&fixture.space].verify().is_ok());
}

#[test]
pub(super) fn canonical_linked_approval_matches_live_and_cold_replay_before_later_edits() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let document = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Approved canonical row".into()),
            cost_cents: Some(317),
            ..Default::default()
        },
    )
    .unwrap();
    let request = open(&mut fixture, &agent, &document).unwrap();
    assert!(
        fixture.engine.resolve_request(&request.id, true).is_err(),
        "local canonical writes stay closed"
    );
    let accepted = fixture.ordered(decision(&request, true)).unwrap();
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
    )
    .unwrap();
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
pub(super) fn a_later_matching_edit_does_not_repair_a_refused_historical_approval() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let changed = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Changed before decision".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let request = open(&mut fixture, &agent, &original()).unwrap();
    let refused = fixture.ordered(decision(&request, true)).unwrap();
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
    )
    .unwrap();
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
pub(super) fn removed_duplicate_lines_use_the_first_remaining_exact_external_id() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let request = open(&mut fixture, &agent, &original()).unwrap();
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
    fixture
        .ordered(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))))
        .unwrap();
    fixture.ordered(decision(&request, true)).unwrap();
    assert_eq!(
        fixture.engine.state.requests[&request.id].status,
        ReqStatus::Pending
    );
    let remaining = document.view().unwrap().to_plan_doc();
    let credit = open(&mut fixture, &agent, &remaining).unwrap();
    fixture.ordered(decision(&credit, true)).unwrap();
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
pub(super) fn optimistic_canonical_pages_cannot_supply_confirmed_approval_content() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
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
    )
    .unwrap();
    let pending = fixture
        .queue(fixture.version(ItemContent::Page(page(fold.manifest().unwrap(), &document))));
    let baseline = open(&mut fixture, &agent, &original()).unwrap();
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
    fixture.ordered(decision(&request, true)).unwrap();
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
pub(super) fn exact_text_and_cents_select_the_first_duplicate_id_while_done_is_not_hashed() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let mut legacy = original();
    legacy.sections[0].lines[0].cost_cents = -17;
    fixture
        .ordered(fixture.version(ItemContent::Plan(legacy)))
        .unwrap();
    let document = canonical(
        &mut fixture,
        LinePatch {
            text: Some("Cafe\u{301}\r\n😀".into()),
            cost_cents: None,
            done: Some(true),
        },
    )
    .unwrap();
    let request = open(&mut fixture, &agent, &document).unwrap();
    let mut second = linked(&fixture, &agent, &document).unwrap();
    let second_line = &document.sections[0].lines[1];
    second.content_hash = content_hash(&(&second_line.text, second_line.cost_cents));
    fixture
        .ordered_as(
            &agent,
            EventBody::RequestOpened {
                request: second.clone(),
            },
        )
        .unwrap();
    fixture.ordered(decision(&second, true)).unwrap();
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
    )
    .unwrap();
    fixture.ordered(decision(&request, true)).unwrap();
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
