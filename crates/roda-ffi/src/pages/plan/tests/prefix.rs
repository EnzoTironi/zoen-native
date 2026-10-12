use super::super::confirmed::Prefix;
use super::*;

fn frontier(fixture: &Fixture) -> usize {
    fixture.engine.logs[&fixture.space].len()
}

fn snapshot_at(fixture: &Fixture, end: usize) -> (PlanFold, u32) {
    Prefix::from_log(
        &fixture.space,
        fixture.engine.logs.get(&fixture.space),
        end,
    )
    .unwrap()
    .fold(&fixture.item, |content| fixture.engine.plan_payload(content))
    .unwrap()
}

#[test]
fn bounded_prefix_excludes_a_later_cutover_and_late_legacy_write() {
    let mut fixture = Fixture::new();
    let before_end = frontier(&fixture);
    let (before, before_number) = snapshot_at(&fixture, before_end);
    let manifest = before.manifest().unwrap();
    let creation = before.slots[0].source.clone();
    let old_version = before.document.as_ref().unwrap().version();
    let document = edited(
        before.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Cafe\u{301} 👩🏽‍💻\r\nexact bytes".into()),
            done: Some(true),
            ..Default::default()
        },
    );
    let canonical = fixture.ordered(fixture.version(ItemContent::Page(page(
        manifest.clone(),
        &document,
    ))));
    let canonical_end = frontier(&fixture);
    let late_document = PlanDoc {
        summary: "Late legacy overwrite".into(),
        ..original()
    };
    let late = fixture.ordered(fixture.version(ItemContent::Plan(late_document.clone())));

    let (historical, number) = snapshot_at(&fixture, before_end);
    assert_eq!(number, before_number);
    assert_eq!(number, 1);
    assert_eq!(view(&historical), original());
    assert_eq!(historical.document.as_ref().unwrap().version(), old_version);
    assert_eq!(historical.manifest().unwrap(), manifest);
    assert_eq!(historical.slots, before.slots);
    assert_eq!(historical.slots[0].source, creation);
    assert!(historical.frozen_at.is_none());

    let canonical_source = source(&fixture.space, &fixture.item, &canonical, true);
    let (cutover, number) = snapshot_at(&fixture, canonical_end);
    assert_eq!(number, 2);
    assert_eq!(cutover.slots.len(), 2);
    assert_eq!(cutover.accepted_current, Some(2));
    assert_eq!(cutover.frozen_at, Some(canonical_source.clone()));
    assert_eq!(cutover.manifest().unwrap(), manifest);
    assert_eq!(view(&cutover), document.view().unwrap().to_plan_doc());
    assert_eq!(cutover.document.as_ref().unwrap().version(), document.version());
    assert!(cutover.acknowledges(&canonical.client_id, &canonical_source.event_id));

    let visible = fixture.fold();
    assert_eq!(visible.frozen_at, cutover.frozen_at);
    assert_eq!(view(&visible), view(&cutover));
    assert_eq!(visible.slots.len(), 3);
    assert_eq!(visible.slots[2].number, Some(3));
    assert_eq!(visible.slots[2].content, Some(ItemContent::Plan(late_document)));
    assert_eq!(
        visible.slots[2].source,
        source(&fixture.space, &fixture.item, &late, true)
    );
    assert!(matches!(visible.slots[2].acceptance, Acceptance::Refused(_)));
    assert!(!visible.acknowledges(&late.client_id, &visible.slots[2].source.event_id));
}

#[test]
fn prefix_roster_checks_use_membership_at_each_ordered_event() {
    let mut fixture = Fixture::new();
    let member = add_identity(&mut fixture.engine, "Historical member");
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Reader,
    });
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let content = page(baseline.manifest().unwrap(), &document);
    let reader = fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    let reader_end = frontier(&fixture);
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    let accepted = fixture.ordered_as(&member, fixture.version(ItemContent::Page(content)));
    let accepted_end = frontier(&fixture);
    fixture.ordered(EventBody::MemberRemoved {
        identity: member.clone(),
    });
    let later_document = edited(
        &document,
        LinePatch {
            text: Some("Removed member's replacement".into()),
            ..Default::default()
        },
    );
    let removed = fixture.ordered_as(
        &member,
        fixture.version(ItemContent::Page(page(
            baseline.manifest().unwrap(),
            &later_document,
        ))),
    );

    let (read_only, number) = snapshot_at(&fixture, reader_end);
    assert_eq!(number, 2);
    assert_eq!(view(&read_only), original());
    assert!(read_only.frozen_at.is_none());
    assert_eq!(read_only.slots[1].source.author, member);
    assert!(matches!(read_only.slots[1].acceptance, Acceptance::Refused(_)));
    let (authorized, number) = snapshot_at(&fixture, accepted_end);
    assert_eq!(number, 3);
    assert_eq!(authorized.accepted_current, Some(3));
    assert_eq!(view(&authorized), document.view().unwrap().to_plan_doc());
    assert_eq!(
        authorized.frozen_at,
        Some(source(&fixture.space, &fixture.item, &accepted, true))
    );
    let (full, number) = snapshot_at(&fixture, frontier(&fixture));
    assert_eq!(number, 4);
    assert_eq!(full.accepted_current, Some(3));
    assert_eq!(full.frozen_at, authorized.frozen_at);
    assert_eq!(view(&full), view(&authorized));
    assert_eq!(full.slots[3].source.author, member);
    assert!(matches!(full.slots[3].acceptance, Acceptance::Refused(_)));
    for refused in [&reader, &removed] {
        assert!(!full.acknowledges(
            &refused.client_id,
            &source(&fixture.space, &fixture.item, refused, true).event_id
        ));
    }
}

#[test]
fn ui_overlay_keeps_version_addresses_after_invalid_undo_and_duplicate_creation() {
    let mut fixture = Fixture::new();
    fixture.ordered(fixture.undo(99));
    let reader = add_identity(&mut fixture.engine, "Reader");
    fixture.ordered(EventBody::MemberAdded {
        identity: reader.clone(),
        role: Role::Reader,
    });
    fixture.ordered_as(&reader, fixture.version(ItemContent::Plan(original())));
    fixture.ordered(EventBody::ItemCreated {
        item: fixture.item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "Duplicate creation".into(),
    });
    let end = frontier(&fixture);
    let stored = fixture.engine.store.events(&fixture.space).unwrap();
    let (confirmed, number) = snapshot_at(&fixture, end);
    assert_eq!(number, 2);
    assert_eq!(confirmed.accepted_current, Some(1));
    assert_eq!(
        confirmed.slots.iter().map(|slot| slot.number).collect::<Vec<_>>(),
        vec![Some(1), None, Some(2), None]
    );
    assert!(confirmed.slots[1..]
        .iter()
        .all(|slot| matches!(slot.acceptance, Acceptance::Refused(_))));
    assert_eq!(view(&confirmed), original());
    let manifest = confirmed.manifest().unwrap();
    let intent = PlanDoc {
        summary: "Private pending CR\rLF\n😀 bytes".into(),
        ..original()
    };
    let pending = fixture.queue(fixture.version(ItemContent::Plan(intent.clone())));
    let pending_source = source(&fixture.space, &fixture.item, &pending, false);
    let visible = fixture.fold();
    assert_eq!(&visible.slots[..4], confirmed.slots.as_slice());
    assert_eq!(visible.slots[4].number, Some(3));
    assert_eq!(visible.slots[4].source, pending_source);
    assert_eq!(visible.slots[4].content, Some(ItemContent::Plan(intent.clone())));
    assert_eq!(visible.slots[4].acceptance, Acceptance::Pending);
    assert!(visible.pending_legacy);
    assert_eq!(view(&visible), original());
    assert!(visible.manifest().is_err());

    assert!(fixture.engine.reject(&pending.client_id, "permission", true, None));
    let failed = fixture.fold();
    assert_eq!(&failed.slots[..4], confirmed.slots.as_slice());
    assert_eq!(failed.slots[4].number, Some(3));
    assert_eq!(failed.slots[4].source, pending_source);
    assert_eq!(failed.slots[4].content, Some(ItemContent::Plan(intent)));
    assert_eq!(failed.slots[4].acceptance, Acceptance::TransportRefused("permission".into()));
    assert!(!failed.pending_legacy);
    assert_eq!(failed.manifest().unwrap(), manifest);
    assert_eq!(view(&failed), original());
    let (unchanged, number) = snapshot_at(&fixture, end);
    assert_eq!(number, 2);
    assert_eq!(unchanged.slots, confirmed.slots);
    assert_eq!(unchanged.manifest().unwrap(), manifest);
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), stored);
    assert_eq!(fixture.engine.store.outbox().unwrap()[0].event.content, pending.content);
}

#[test]
fn bounded_fold_never_reads_payloads_beyond_its_frontier() {
    let mut fixture = Fixture::new();
    let member = add_identity(&mut fixture.engine, "Removed writer");
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let valid = page(baseline.manifest().unwrap(), &document);
    let before_end = frontier(&fixture);
    fixture.ordered(fixture.version(ItemContent::Page(valid.clone())));
    let valid_end = frontier(&fixture);
    let mut wrong_manifest = baseline.manifest().unwrap();
    wrong_manifest.space = "wrong-space".into();
    fixture.ordered(fixture.version(ItemContent::Page(PageContent {
        manifest: Some(PageManifest::PlanV1(wrong_manifest)),
        ..valid.clone()
    })));
    fixture.ordered(EventBody::MemberRemoved { identity: member.clone() });
    fixture.ordered_as(&member, fixture.version(ItemContent::Page(valid.clone())));
    let refused_end = frontier(&fixture);
    let malformed = PageContent {
        loro: "invalid base64!!!".into(),
        ..valid.clone()
    };
    fixture.ordered(fixture.version(ItemContent::Page(malformed.clone())));
    let malformed_end = frontier(&fixture);
    let bytes = document.full_update().to_json().unwrap();
    let missing = PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: hex::encode(Sha256::digest(&bytes)),
            bytes: bytes.len() as u64,
            blob: "ab".repeat(32),
            key: "cd".repeat(32),
        }),
        ..valid.clone()
    };
    fixture.ordered(fixture.version(ItemContent::Page(missing.clone())));
    let missing_end = frontier(&fixture);
    let stored = fixture.engine.store.events(&fixture.space).unwrap();
    for (end, expected) in [
        (before_end, vec![]),
        (valid_end, vec![valid.clone()]),
        (refused_end, vec![valid.clone()]),
        (malformed_end, vec![valid.clone(), malformed.clone()]),
        (missing_end, vec![valid.clone(), malformed.clone(), missing]),
    ] {
        let mut reads = Vec::new();
        let (fold, _) = Prefix::from_log(
            &fixture.space,
            fixture.engine.logs.get(&fixture.space),
            end,
        )
        .unwrap()
        .fold(&fixture.item, |content| {
            reads.push(content.clone());
            fixture.engine.plan_payload(content)
        })
        .unwrap();
        assert_eq!(reads, expected);
        assert_eq!(fold.waiting, end == missing_end);
        if end == before_end {
            assert_eq!(view(&fold), original());
            assert!(fold.frozen_at.is_none());
        } else {
            assert_eq!(view(&fold), document.view().unwrap().to_plan_doc());
            assert_eq!(fold.accepted_current, Some(2));
        }
        if end >= refused_end {
            assert!(matches!(fold.slots[2].acceptance, Acceptance::Refused(_)));
            assert!(matches!(fold.slots[3].acceptance, Acceptance::Refused(_)));
        }
        if end >= malformed_end {
            assert!(matches!(fold.slots[4].acceptance, Acceptance::Refused(_)));
        }
        if end == missing_end {
            assert!(matches!(fold.slots[5].acceptance, Acceptance::Waiting(_)));
        }
    }
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), stored);
}

#[test]
fn wrong_space_or_unavailable_prefix_cannot_borrow_future_history() {
    let mut fixture = Fixture::new();
    let other = Fixture::new();
    let end = frontier(&fixture);
    let stored = fixture.engine.store.events(&fixture.space).unwrap();
    let other_stored = other.engine.store.events(&other.space).unwrap();
    let pending_item = new_id("pending-origin");
    fixture.queue(EventBody::ItemCreated {
        item: pending_item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "Private draft origin".into(),
    });
    assert!(matches!(
        Prefix::from_log(&fixture.space, other.engine.logs.get(&other.space), 0),
        Err(CoreError::Invalid { .. })
    ));
    assert!(matches!(
        Prefix::from_log(&fixture.space, fixture.engine.logs.get(&fixture.space), end + 1),
        Err(CoreError::Invalid { .. })
    ));
    assert!(matches!(
        Prefix::from_log(&fixture.space, None, 1),
        Err(CoreError::Invalid { .. })
    ));
    for empty in [
        Prefix::from_log(&fixture.space, None, 0).unwrap(),
        Prefix::from_log(&fixture.space, fixture.engine.logs.get(&fixture.space), 0).unwrap(),
        Prefix::from_log(&fixture.space, fixture.engine.logs.get(&fixture.space), 1).unwrap(),
    ] {
        let (fold, number) = empty
            .fold(&fixture.item, |_| panic!("empty prefix read a payload"))
            .unwrap();
        assert_eq!(number, 0);
        assert!(fold.slots.is_empty());
        assert!(fold.document.is_none());
        assert!(fold.origin_tag().is_none());
        assert!(fold.frozen_at.is_none());
        assert!(fold.manifest().is_err());
    }
    let (no_origin, number) = Prefix::from_log(
        &fixture.space,
        fixture.engine.logs.get(&fixture.space),
        end,
    )
    .unwrap()
    .fold(&pending_item, |_| panic!("pending creation read a payload"))
    .unwrap();
    assert_eq!(number, 0);
    assert!(no_origin.slots.is_empty());
    assert!(no_origin.document.is_none());
    assert!(no_origin.origin_tag().is_none());
    let private = fixture.engine.plan_timeline(&pending_item).unwrap();
    assert!(private.pending_legacy);
    assert_eq!(private.slots[0].acceptance, Acceptance::Pending);
    assert_eq!(frontier(&fixture), end);
    assert_eq!(view(&snapshot_at(&fixture, end).0), original());
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), stored);
    assert_eq!(other.engine.store.events(&other.space).unwrap(), other_stored);
}
