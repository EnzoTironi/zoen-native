use super::*;
use roda_docs::plan::PlanBlockContent;

#[test]
fn legacy_edit_flag_admits_the_owner_and_active_member_without_writing_on_read() {
    let mut fixture = Fixture::new();
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(read.legacy_edit_admitted);
    assert_eq!(storage(&fixture), before);
    let added = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Exact native cost", 1_025)
        .unwrap();
    assert_eq!(added.item.plan.unwrap().total_cents, 1_218);
    let pending_read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(pending_read.legacy_edit_admitted);
    assert_eq!(
        pending_read.source,
        PlanPageReadSourceDto::LocalPendingLegacy
    );
    assert!(pending_read.pending_sync);
    assert_eq!(pending_read.total_cents, Some(1_218));
    let pending = fixture.engine.store.outbox().unwrap()[0].event.clone();
    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let before = storage(&fixture);
    assert!(
        fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert_eq!(storage(&fixture), before);

    let member = add_identity(&mut fixture.engine, "Active member");
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    fixture.engine.me = Some(member);
    let before = storage(&fixture);
    assert!(
        fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert_eq!(storage(&fixture), before);
    let added = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Exact zero", 0)
        .unwrap();
    assert_eq!(
        added.item.plan.unwrap().sections[0]
            .lines
            .last()
            .unwrap()
            .cost_cents,
        0
    );
}

#[test]
fn legacy_edit_flag_and_actual_mutations_refuse_reader_removed_and_unsigned_actors() {
    let mut fixture = Fixture::new();
    let member = add_identity(&mut fixture.engine, "Reader");
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Reader,
    });
    fixture.engine.me = Some(member.clone());
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.legacy_edit_admitted);
    assert_eq!(storage(&fixture), before);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Refused reader", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);

    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    assert!(
        fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    fixture.ordered(EventBody::MemberRemoved {
        identity: member.clone(),
    });
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Refused removal", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);

    fixture.engine.me = None;
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Refused identity", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn legacy_edit_flag_refuses_pending_and_ordered_canonical_cutover() {
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    let baseline = fixture.fold();
    let canonical = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let pending = fixture.queue(fixture.version(ItemContent::Page(page(
        baseline.manifest().unwrap(),
        &canonical,
    ))));
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.pending_sync);
    assert!(!read.legacy_edit_admitted);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Refused pending", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert!(!read.legacy_edit_admitted);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Refused cutover", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn private_plan_draft_disables_legacy_controls_without_changing_the_existing_mutation_gate() {
    let mut fixture = Fixture::new();
    fixture
        .engine
        .with_document(&fixture.item, ItemKind::Plan, |_, session| {
            let read = session.read(&fixture.engine)?;
            let DocumentView::Plan { blocks } = &read.view else {
                panic!("Plan required")
            };
            let line = blocks
                .iter()
                .find(|block| matches!(block.content, PlanBlockContent::Line { .. }))
                .unwrap();
            let edit = PlanBlockEdit {
                id: line.id.clone(),
                content: line.content.clone(),
                text: "Private draft CR\rLF\n😀 bytes".into(),
                spans: vec![],
            };
            session.apply_from(
                &fixture.engine,
                "native-legacy-admission-draft",
                &read.edit_context,
                DocumentEdit::Plan {
                    order: &blocks
                        .iter()
                        .map(|block| block.id.clone())
                        .collect::<Vec<_>>(),
                    changed: &[edit],
                },
            )?;
            Ok(())
        })
        .unwrap();
    let before = storage(&fixture);
    let private = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(private.ready);
    assert!(private.unsaved);
    assert!(!private.legacy_edit_admitted);
    assert_eq!(storage(&fixture), before);
    assert_eq!(
        fixture
            .engine
            .plan_snapshot(&fixture.item)
            .unwrap()
            .document,
        original()
    );
    assert!(fixture.engine.require_legacy_plan(&fixture.item).is_ok());
    let outcome = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Existing backend policy", 0)
        .unwrap();
    assert_eq!(
        outcome.item.plan.unwrap().sections[0]
            .lines
            .last()
            .unwrap()
            .cost_cents,
        0
    );
}

#[test]
fn missing_or_damaged_payload_never_admits_legacy_editing_before_or_after_recovery() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let canonical = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let bytes = canonical.full_update().to_json().unwrap();
    let hash = hex::encode(Sha256::digest(&bytes));
    fixture.ordered(fixture.version(ItemContent::Page(PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: hash.clone(),
            bytes: bytes.len() as u64,
            blob: "ab".repeat(32),
            key: "cd".repeat(32),
        }),
        ..page(baseline.manifest().unwrap(), &canonical)
    })));
    let before = storage(&fixture);
    let missing = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(!missing.ready);
    assert!(!missing.legacy_edit_admitted);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Missing payload", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
    fixture
        .engine
        .store
        .put_media(&hash, "application/json", &vec![b'x'; bytes.len()], 0)
        .unwrap();
    let before = storage(&fixture);
    let damaged = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(!damaged.ready);
    assert!(!damaged.legacy_edit_admitted);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Damaged payload", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
    assert!(!fixture
        .engine
        .import_media(&hash, b"wrong imported payload", "application/json")
        .unwrap());
    assert_eq!(storage(&fixture), before);
    assert!(fixture
        .engine
        .import_media(&hash, &bytes, "application/json")
        .unwrap());
    let before = storage(&fixture);
    let recovered = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(recovered.ready);
    assert_eq!(fixture.engine.media(&hash).unwrap(), Some(bytes.clone()));
    assert!(!recovered.legacy_edit_admitted);
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Recovered cutover", 1)
        .is_err());
    assert_eq!(storage(&fixture), before);
    assert!(fixture
        .engine
        .import_media(&hash, &bytes, "application/vnd.zoen.page")
        .unwrap());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn legacy_edit_flag_refuses_unrepresentable_total_and_nonlegacy_current_content() {
    let mut overflow = Fixture::new();
    let mut plan = original();
    plan.sections[0].lines[0].cost_cents = i64::MAX;
    plan.sections[0].lines[1].cost_cents = i64::MAX;
    overflow.ordered(overflow.version(ItemContent::Plan(plan)));
    let before = storage(&overflow);
    let read = overflow.engine.plan_page(&overflow.item).unwrap();
    assert!(read.ready);
    assert_eq!(read.total_cents, None);
    assert!(!read.legacy_edit_admitted);
    assert!(overflow
        .engine
        .add_plan_line(&overflow.item, 0, "Unrepresentable", 0)
        .is_err());
    assert_eq!(storage(&overflow), before);

    let mut malformed = Fixture::new();
    let baseline = malformed.fold();
    malformed.ordered(malformed.version(ItemContent::Page(PageContent {
        loro: "not base64!!!".into(),
        ..page(
            baseline.manifest().unwrap(),
            baseline.document.as_ref().unwrap(),
        )
    })));
    let before = storage(&malformed);
    let read = malformed.engine.plan_page(&malformed.item).unwrap();
    assert!(read.ready);
    assert!(!read.legacy_edit_admitted);
    assert!(malformed.fold().frozen_at.is_none());
    assert!(malformed
        .engine
        .require_legacy_plan(&malformed.item)
        .is_ok());
    assert!(malformed
        .engine
        .add_plan_line(&malformed.item, 0, "Nonlegacy current", 0)
        .is_err());
    assert_eq!(storage(&malformed), before);
}

#[test]
fn signed_offline_creation_keeps_two_legacy_edits_without_an_authenticated_page_seed() {
    let mut fixture = Fixture::new();
    let item = new_id("native-offline-plan");
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
    assert!(created.content_hash.starts_with("local-legacy:"));
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

    let first = fixture
        .engine
        .add_plan_line(&item, 0, "Offline first", 25)
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
    let second = fixture
        .engine
        .add_plan_line(&item, 0, "Offline second", 30)
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
        deliver(&mut fixture.engine, &event, Ingest::Confirmed);
    }
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert_eq!(read.source, PlanPageReadSourceDto::DocumentSession);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(248));
    assert_eq!(storage(&fixture), before);
}

#[test]
fn typed_legacy_controls_preserve_two_offline_edits_despite_the_crdt_session_gate() {
    let mut fixture = Fixture::new();
    let first = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Offline first", 5)
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

    let second = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Offline second", 5)
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
        deliver(&mut fixture.engine, &event, Ingest::Confirmed);
    }
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert_eq!(read.source, PlanPageReadSourceDto::DocumentSession);
    assert!(read.legacy_edit_admitted);
    assert_eq!(read.total_cents, Some(203));
    assert_eq!(storage(&fixture), before);
}

#[test]
fn legacy_admission_and_actual_writes_preserve_missing_key_and_relay_account_refusal() {
    let mut fixture = Fixture::new();
    let signer = fixture.engine.signers.remove(&fixture.owner).unwrap();
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!(fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Missing key", 0)
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
        space: new_id("authenticated-plan-space"),
        item: new_id("authenticated-plan"),
        owner: account,
    };
    relay.ordered(EventBody::SpaceCreated {
        title: "Authenticated Plan conversation".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Public,
    });
    relay.ordered(EventBody::ItemCreated {
        item: relay.item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "account user".into(),
    });
    let other_writer = add_identity(&mut relay.engine, "Other writer");
    relay.ordered(EventBody::MemberAdded {
        identity: other_writer.clone(),
        role: Role::Member,
    });
    let before = storage(&relay);
    assert!(
        relay
            .engine
            .plan_page(&relay.item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert_eq!(storage(&relay), before);
    relay
        .engine
        .add_plan_line(&relay.item, 0, "Matching relay signer", 0)
        .unwrap();
    relay.engine.me = Some(other_writer.clone());
    assert!(relay.engine.state.writer(&relay.space, &other_writer));
    assert!(relay.engine.author_for(&other_writer).is_ok());
    assert!(relay.engine.net.synced.contains(&relay.space));
    let before = storage(&relay);
    let read = relay.engine.plan_page(&relay.item).unwrap();
    assert_eq!(read.source, PlanPageReadSourceDto::LocalPendingLegacy);
    assert!(!read.legacy_edit_admitted);
    assert!(relay
        .engine
        .add_plan_line(&relay.item, 0, "Wrong relay signer", 0)
        .is_err());
    assert_eq!(storage(&relay), before);
}

#[test]
fn a_typed_local_pending_creation_never_admits_reader_or_removed_member_writes() {
    let mut fixture = Fixture::new();
    let reader = add_identity(&mut fixture.engine, "Offline reader");
    fixture.ordered(EventBody::MemberAdded {
        identity: reader.clone(),
        role: Role::Reader,
    });
    let item = new_id("native-offline-reader-plan");
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
    assert!(fixture
        .engine
        .add_plan_line(&item, 0, "Offline reader", 0)
        .is_err());
    assert_eq!(storage(&fixture), before);
    fixture.ordered(EventBody::MemberRemoved { identity: reader });
    let before = storage(&fixture);
    assert!(
        !fixture
            .engine
            .plan_page(&item)
            .unwrap()
            .legacy_edit_admitted
    );
    assert!(fixture
        .engine
        .add_plan_line(&item, 0, "Offline removed", 0)
        .is_err());
    assert_eq!(storage(&fixture), before);
}
