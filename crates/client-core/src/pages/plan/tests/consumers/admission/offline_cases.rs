//! offline cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn signed_offline_creation_keeps_two_legacy_edits_without_an_authenticated_page_seed(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let item = new_id("native-offline-plan").unwrap();
    let creation = fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "Signed offline creation".into(),
    });
    let before = storage(&fixture);
    let created = fixture.engine.plan_page(&item).unwrap();
    assert_eq!(created.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert_eq!(created.version, 0);
    assert!(created.ready);
    assert!(created.pending_sync);
    assert!(!created.unsaved);
    assert!(created.legacy_edit_admitted);
    assert_eq!(created.total_cents, Some(193));
    assert_eq!(created.blocks[0].text, original().title);
    assert_eq!(created.blocks[1].text, original().summary);
    assert_eq!(created.content_hash.len(), 64);
    assert!(created.content_hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(created
        .blocks
        .iter()
        .all(|block| block.id.starts_with("local-legacy:")));
    let mut ids: Vec<_> = created.blocks.iter().map(|block| &block.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), created.blocks.len());
    assert!(fixture.engine.require_legacy_plan(&item).is_ok());
    assert_eq!(storage(&fixture), before);

    let first = {
        let observed_hash = fixture.engine.plan_page(&item).unwrap().content_hash;
        fixture
            .engine
            .add_plan_line(&item, 0, "Offline first", 25, &observed_hash)
    }
    .unwrap();
    assert_eq!(first.item.plan.unwrap().total_cents, 218);
    let first_event = fixture
        .engine
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|pending| pending.event.client_id != creation.client_id)
        .unwrap()
        .event;
    let second = {
        let observed_hash = fixture.engine.plan_page(&item).unwrap().content_hash;
        fixture
            .engine
            .add_plan_line(&item, 0, "Offline second", 30, &observed_hash)
    }
    .unwrap();
    assert_eq!(second.item.plan.unwrap().total_cents, 248);
    let pending = fixture.engine.store.outbox().unwrap();
    assert_eq!(pending.len(), 3);
    let second_event = pending
        .into_iter()
        .find(|pending| {
            pending.event.client_id != creation.client_id
                && pending.event.client_id != first_event.client_id
        })
        .unwrap()
        .event;
    let before = storage(&fixture);
    let fold = fixture.engine.plan_timeline(&item).unwrap();
    assert!(fold.document.is_none());
    assert!(fold.pending_legacy);
    assert!(fold.manifest().is_err());
    let read = fixture.engine.plan_page(&item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert_eq!(read.version, 0);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(248));
    let costs: Vec<_> = read
        .blocks
        .iter()
        .filter_map(|block| match block.content {
            PlanPageContentDto::Line { cost_cents, .. } => Some(cost_cents),
            _ => None,
        })
        .collect();
    assert_eq!(costs, [200, -7, 25, 30]);
    assert_eq!(
        fixture.engine.plan_snapshot(&item).unwrap().total_cents,
        Some(248)
    );
    assert_eq!(storage(&fixture), before);

    for event in [creation, first_event, second_event] {
        deliver(&mut fixture.engine, &event, Ingest::Confirmed)?;
    }
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert_eq!(read.source, PlanPageReadSourceDto::DocumentSession);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(248));
    assert_eq!(storage(&fixture), before);
    Ok(())
}

#[test]
pub(super) fn typed_legacy_controls_preserve_two_offline_edits_despite_the_crdt_session_gate(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let first = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Offline first", 5, &observed_hash)
    }
    .unwrap();
    assert_eq!(first.item.plan.unwrap().total_cents, 198);
    let first_event = fixture.engine.store.outbox().unwrap()[0].event.clone();
    let before = storage(&fixture);
    fixture
        .engine
        .with_document(&fixture.item, ItemKind::Plan, |_, session| {
            let read = session.read(&fixture.engine)?;
            assert!(read.ready);
            assert!(read.pending_sync);
            assert!(!read.can_edit);
            assert!(!read.unsaved);
            assert!(read.save_error.is_some());
            assert!(!session.legacy_read_blocked());
            Ok(())
        })
        .unwrap();
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert_eq!(read.version, 1);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(198));
    assert!(fixture.engine.require_legacy_plan(&fixture.item).is_ok());
    assert_eq!(storage(&fixture), before);

    let second = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Offline second", 5, &observed_hash)
    }
    .unwrap();
    assert_eq!(second.item.plan.unwrap().total_cents, 203);
    let pending = fixture.engine.store.outbox().unwrap();
    assert_eq!(pending.len(), 2);
    let second_event = pending
        .into_iter()
        .find(|pending| pending.event.client_id != first_event.client_id)
        .unwrap()
        .event;
    let before = storage(&fixture);
    assert!(fixture.fold().pending_legacy);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert_eq!(read.version, 1);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(203));
    assert_eq!(
        fixture
            .engine
            .plan_snapshot(&fixture.item)
            .unwrap()
            .total_cents,
        Some(203)
    );
    assert_eq!(storage(&fixture), before);

    for event in [first_event, second_event] {
        deliver(&mut fixture.engine, &event, Ingest::Confirmed)?;
    }
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert_eq!(read.source, PlanPageReadSourceDto::DocumentSession);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(203));
    assert_eq!(storage(&fixture), before);
    Ok(())
}

#[test]
pub(super) fn legacy_admission_and_actual_writes_preserve_missing_key_and_relay_account_refusal() {
    let mut fixture = Fixture::new().unwrap();
    let signer = fixture.engine.signers.remove(&fixture.owner).unwrap();
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Missing key", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
    fixture.engine.signers.insert(fixture.owner.clone(), signer);

    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Relay account", "relay_account", "http://127.0.0.1:9")
        .unwrap();
    let account = engine.me_id().unwrap();
    let mut relay = Fixture {
        engine,
        space: new_id("authenticated-plan-space").unwrap(),
        item: new_id("authenticated-plan").unwrap(),
        owner: account,
    };
    relay
        .ordered(EventBody::SpaceCreated {
            title: "Authenticated Plan conversation".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Public,
        })
        .unwrap();
    relay
        .ordered(EventBody::ItemCreated {
            item: relay.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "account user".into(),
        })
        .unwrap();
    let other_writer = add_identity(&mut relay.engine, "Other writer");
    relay
        .ordered(EventBody::MemberAdded {
            identity: other_writer.clone(),
            role: Role::Member,
        })
        .unwrap();
    let before = storage(&relay);
    assert!(
        relay
            .engine
            .plan_page(&relay.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert_eq!(storage(&relay), before);
    {
        let observed_hash = relay.engine.plan_page(&relay.item).unwrap().content_hash;
        relay
            .engine
            .add_plan_line(&relay.item, 0, "Matching relay signer", 0, &observed_hash)
    }
    .unwrap();
    relay.engine.me = Some(other_writer.clone());
    assert!(relay.engine.state.writer(&relay.space, &other_writer));
    assert!(relay.engine.author_for(&other_writer).is_ok());
    assert!(relay.engine.net.synced.contains(&relay.space));
    let before = storage(&relay);
    let read = relay.engine.plan_page(&relay.item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert!(!read.legacy_edit_admitted);
    assert!({
        let observed_hash = relay.engine.plan_page(&relay.item).unwrap().content_hash;
        relay
            .engine
            .add_plan_line(&relay.item, 0, "Wrong relay signer", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&relay), before);
}

#[test]
pub(super) fn a_typed_local_pending_creation_never_admits_reader_or_removed_member_writes() {
    let mut fixture = Fixture::new().unwrap();
    let reader = add_identity(&mut fixture.engine, "Offline reader");
    fixture
        .ordered(EventBody::MemberAdded {
            identity: reader.clone(),
            role: Role::Reader,
        })
        .unwrap();
    let item = new_id("native-offline-reader-plan").unwrap();
    fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "Signed offline creation".into(),
    });
    fixture.engine.me = Some(reader.clone());
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert!(read.ready);
    assert!(!read.legacy_edit_admitted);
    assert!({
        let observed_hash = fixture.engine.plan_page(&item).unwrap().content_hash;
        fixture
            .engine
            .add_plan_line(&item, 0, "Offline reader", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
    fixture
        .ordered(EventBody::MemberRemoved { identity: reader })
        .unwrap();
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!({
        let observed_hash = fixture.engine.plan_page(&item).unwrap().content_hash;
        fixture
            .engine
            .add_plan_line(&item, 0, "Offline removed", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
}
