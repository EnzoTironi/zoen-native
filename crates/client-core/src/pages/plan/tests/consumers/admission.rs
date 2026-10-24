use super::*;
use documents::plan::PlanBlockContent;

#[test]
fn legacy_edit_flag_admits_the_owner_and_active_member_without_writing_on_read() {
    let mut fixture = Fixture::new().unwrap();
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(read.legacy_edit_admitted);
    assert_eq!(storage(&fixture), before);
    let added = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Exact native cost", 1_025, &observed_hash)
    }
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
    let added = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Exact zero", 0, &observed_hash)
    }
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
    let mut fixture = Fixture::new().unwrap();
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Refused reader", 1, &observed_hash)
    }
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Refused removal", 1, &observed_hash)
    }
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Refused identity", 1, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn legacy_edit_flag_refuses_pending_and_ordered_canonical_cutover() {
    let mut fixture = Fixture::new().unwrap();
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Refused pending", 1, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let before = storage(&fixture);
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.pending_sync);
    assert!(!read.legacy_edit_admitted);
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Refused cutover", 1, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn private_plan_draft_disables_legacy_controls_without_changing_the_existing_mutation_gate() {
    let mut fixture = Fixture::new().unwrap();
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
    let outcome = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture.engine.add_plan_line(
            &fixture.item,
            0,
            "Existing backend policy",
            0,
            &observed_hash,
        )
    }
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
    let mut fixture = Fixture::new().unwrap();
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Missing payload", 1, &observed_hash)
    }
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Damaged payload", 1, &observed_hash)
    }
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
    assert!({
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Recovered cutover", 1, &observed_hash)
    }
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
    let mut overflow = Fixture::new().unwrap();
    let mut plan = original();
    plan.sections[0].lines[0].cost_cents = i64::MAX;
    plan.sections[0].lines[1].cost_cents = i64::MAX;
    overflow.ordered(overflow.version(ItemContent::Plan(plan)));
    let before = storage(&overflow);
    let read = overflow.engine.plan_page(&overflow.item).unwrap();
    assert!(read.ready);
    assert_eq!(read.total_cents, None);
    assert!(!read.legacy_edit_admitted);
    assert!({
        let observed_hash = overflow
            .engine
            .plan_page(&overflow.item)
            .unwrap()
            .content_hash;
        overflow
            .engine
            .add_plan_line(&overflow.item, 0, "Unrepresentable", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&overflow), before);

    let mut malformed = Fixture::new().unwrap();
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
    assert!({
        let observed_hash = malformed
            .engine
            .plan_page(&malformed.item)
            .unwrap()
            .content_hash;
        malformed
            .engine
            .add_plan_line(&malformed.item, 0, "Nonlegacy current", 0, &observed_hash)
    }
    .is_err());
    assert_eq!(storage(&malformed), before);
}

mod offline_cases;
