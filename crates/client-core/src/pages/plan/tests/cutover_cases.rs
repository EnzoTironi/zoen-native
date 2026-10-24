//! cutover cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn pending_legacy_snapshots_and_undos_never_enlarge_the_authenticated_seed() {
    let mut fixture = Fixture::new().unwrap();
    let baseline = fixture.fold();
    let manifest = baseline.manifest().unwrap();
    let version = baseline.document.as_ref().unwrap().version();
    let mut changed = original();
    changed.summary = "Only an unconfirmed intent".into();
    let pending = fixture.queue(fixture.version(ItemContent::Plan(changed.clone())));
    let undo = fixture.queue(fixture.undo(1));
    let fold = fixture.fold();
    assert!(fold.pending_legacy);
    assert!(!fold.writable());
    assert!(fold.manifest().is_err());
    assert_eq!(view(&fold), original());
    assert_eq!(fold.document.as_ref().unwrap().version(), version);
    assert_eq!(fold.manifest.as_ref(), Some(&manifest));
    assert_eq!(fold.slots.len(), 3);
    assert!(fold.slots[1..].iter().all(
        |slot| matches!(slot.acceptance, Acceptance::Pending) && slot.source.ordered.is_none()
    ));
    assert!(!fold.acknowledges(
        &pending.client_id,
        &source(&fixture.space, &fixture.item, &pending, false).event_id
    ));

    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    assert!(fixture.fold().pending_legacy); // The still-pending undo retains its intent.
    deliver(&mut fixture.engine, &undo, Ingest::Confirmed);
    let confirmed = fixture.fold();
    assert!(confirmed.writable());
    assert_eq!(view(&confirmed), original());
    assert_eq!(confirmed.manifest().unwrap().legacy_versions, 3);
    assert_ne!(confirmed.document.as_ref().unwrap().version(), version);
    assert!(
        confirmed
            .slots
            .iter()
            .all(|slot| matches!(slot.acceptance, Acceptance::Legacy)
                && slot.source.ordered.is_some())
    );
    assert!(fixture.engine.store.outbox().unwrap().is_empty());
}

#[test]
pub(super) fn refused_legacy_intent_stays_readable_without_blocking_a_new_promotion() {
    let mut fixture = Fixture::new().unwrap();
    let pending = fixture.queue(fixture.version(ItemContent::Plan(PlanDoc {
        summary: "Refused text remains visible".into(),
        ..original()
    })));
    assert!(fixture.fold().pending_legacy);
    assert!(fixture
        .engine
        .reject(&pending.client_id, "permission", true, None)
        .user_failed());
    let fold = fixture.fold();
    assert!(fold.writable());
    assert_eq!(view(&fold), original());
    assert_eq!(fold.manifest().unwrap().legacy_versions, 1);
    assert_eq!(
        fold.slots[1].content,
        Some(ItemContent::Plan(PlanDoc {
            summary: "Refused text remains visible".into(),
            ..original()
        }))
    );
    assert_eq!(
        fold.slots[1].acceptance,
        Acceptance::TransportRefused("permission".into())
    );
    assert!(fold.slots[1].source.ordered.is_none());
    assert_eq!(
        fixture.engine.store.outbox().unwrap()[0].event.content,
        pending.content
    );
}

#[test]
pub(super) fn a_pending_creation_has_no_verified_seed_or_promotion_manifest() {
    let mut fixture = Fixture::new().unwrap();
    let item = new_id("pending-plan").unwrap();
    fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "offline".into(),
    });
    let fold = fixture.engine.plan_timeline(&item).unwrap();
    assert!(fold.document.is_none());
    assert!(fold.frozen_at.is_none());
    assert!(fold.pending_legacy);
    assert!(fold.manifest().is_err());
    assert_eq!(fold.slots.len(), 1);
    assert_eq!(fold.slots[0].acceptance, Acceptance::Pending);
}

#[test]
pub(super) fn an_ordered_remote_cutover_preserves_but_never_imports_local_legacy_intent() {
    let mut fixture = Fixture::new().unwrap();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let remote = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(page(
            baseline.manifest().unwrap(),
            &document,
        ))),
    );
    let intent = PlanDoc {
        summary: "Unconfirmed whole-document intent".into(),
        ..original()
    };
    let pending = fixture.queue(fixture.version(ItemContent::Plan(intent.clone())));
    assert!(!fixture.fold().writable());
    let ordered = deliver(&mut fixture.engine, &remote, Ingest::Applied);
    let cutover = fixture.fold();
    assert_eq!(
        cutover.frozen_at,
        Some(source(&fixture.space, &fixture.item, &ordered, true))
    );
    assert_eq!(view(&cutover), document.view().unwrap().to_plan_doc());
    assert!(cutover.pending_legacy);
    assert!(!cutover.writable());
    assert_eq!(
        cutover.slots.last().unwrap().content,
        Some(ItemContent::Plan(intent.clone()))
    );
    assert_eq!(
        cutover.slots.last().unwrap().acceptance,
        Acceptance::Pending
    );

    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let resolved = fixture.fold();
    assert!(resolved.writable());
    assert_eq!(view(&resolved), document.view().unwrap().to_plan_doc());
    assert_eq!(resolved.manifest().unwrap().legacy_versions, 1);
    assert_eq!(
        resolved.slots.last().unwrap().content,
        Some(ItemContent::Plan(intent))
    );
    assert!(matches!(
        resolved.slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
}

#[test]
pub(super) fn shorter_exact_ancestry_and_two_first_writers_merge_in_both_relay_orders() {
    let mut fixture = Fixture::new().unwrap();
    let first = fixture.fold();
    let first_editor = edited(
        first.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Text from the earlier prefix".into()),
            ..Default::default()
        },
    );
    let first_page = page(first.manifest().unwrap(), &first_editor);
    let queued = fixture.queue(fixture.version(ItemContent::Page(first_page)));
    let pending_fold = fixture.fold();
    assert!(pending_fold.frozen_at.is_none());
    assert_eq!(view(&pending_fold), original());

    let mut legacy_v2 = original();
    legacy_v2.title = "Title from authenticated legacy v2".into();
    legacy_v2.sections[0].lines[0].cost_cents = 700;
    fixture.ordered(fixture.version(ItemContent::Plan(legacy_v2.clone())));
    let second = fixture.fold();
    let second_editor = edited(
        second.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let second_page = page(second.manifest().unwrap(), &second_editor);
    let second_event = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(second_page)),
    );
    let history = fixture.engine.store.events(&fixture.space).unwrap();
    let mut reverse = Engine::open(":memory:").unwrap();
    for event in &history {
        deliver(&mut reverse, event, Ingest::Applied);
    }

    deliver(&mut fixture.engine, &queued, Ingest::Confirmed);
    deliver(&mut fixture.engine, &second_event, Ingest::Applied);
    deliver(&mut reverse, &second_event, Ingest::Applied);
    deliver(&mut reverse, &queued, Ingest::Applied);
    let forward = fixture.fold();
    let backward = reverse.plan_timeline(&fixture.item).unwrap();
    assert_eq!(view(&forward), view(&backward));
    assert_eq!(
        forward.document.as_ref().unwrap().version(),
        backward.document.as_ref().unwrap().version()
    );
    let result = view(&forward);
    assert_eq!(result.title, legacy_v2.title);
    assert_eq!(result.summary, original().summary);
    assert_eq!(
        result.sections[0].lines[0].text,
        "Text from the earlier prefix"
    );
    assert_eq!(result.sections[0].lines[0].cost_cents, 700);
    assert!(result.sections[0].lines[0].done);
    assert_eq!(result.sections[0].lines[1], original().sections[0].lines[1]);
    for fold in [&forward, &backward] {
        assert!(fold.writable());
        assert_eq!(fold.manifest().unwrap().legacy_versions, 2);
        assert!(fold.slots[2..]
            .iter()
            .all(|slot| matches!(slot.acceptance, Acceptance::Canonical(_))));
        assert!(fold.acknowledges(
            &queued.client_id,
            &source(&fixture.space, &fixture.item, &queued, false).event_id
        ));
        assert!(fold.acknowledges(
            &second_event.client_id,
            &source(&fixture.space, &fixture.item, &second_event, false).event_id
        ));
    }
}

#[test]
pub(super) fn invalid_manifests_and_inline_payloads_do_not_freeze_the_first_valid_page() {
    let mut fixture = Fixture::new().unwrap();
    let baseline = fixture.fold();
    let document = baseline.document.as_ref().unwrap();
    let valid = page(baseline.manifest().unwrap(), document);
    let mut variants = Vec::new();
    for field in 0..8 {
        let mut manifest = baseline.manifest().unwrap();
        match field {
            0 => manifest.schema = "future_version".into(),
            1 => manifest.space = "other-space".into(),
            2 => manifest.item = "other-item".into(),
            3 => manifest.creation_event = "other-origin".into(),
            4 => manifest.legacy_versions = 0,
            5 => manifest.legacy_versions = 2,
            6 => manifest.legacy_head = "other-head".into(),
            _ => manifest.prefix_sha256 = "other-prefix".into(),
        }
        variants.push(PageContent {
            manifest: Some(PageManifest::PlanV1(manifest)),
            ..valid.clone()
        });
    }
    variants.push(PageContent {
        manifest: None,
        ..valid.clone()
    });
    variants.push(PageContent {
        manifest: Some(PageManifest::Unsupported(
            serde_json::json!({"schema":"plan_v1","extra":true}),
        )),
        ..valid.clone()
    });
    variants.push(PageContent {
        loro: "not base64!".into(),
        ..valid.clone()
    });
    variants.push(PageContent {
        loro: base64::engine::general_purpose::STANDARD.encode(b"invalid JSON codec"),
        ..valid.clone()
    });
    for variant in variants {
        let event = fixture.ordered(fixture.version(ItemContent::Page(variant)));
        let fold = fixture.fold();
        assert!(matches!(
            fold.slots.last().unwrap().acceptance,
            Acceptance::Refused(_)
        ));
        assert!(fold.frozen_at.is_none());
        assert!(!fold.waiting);
        assert_eq!(fold.manifest().unwrap(), baseline.manifest().unwrap());
        assert_eq!(
            fold.document.as_ref().unwrap().version(),
            document.version()
        );
        assert!(!fold.acknowledges(
            &event.client_id,
            &source(&fixture.space, &fixture.item, &event, true).event_id
        ));
    }
    let accepted = fixture.ordered(fixture.version(ItemContent::Page(valid)));
    let fold = fixture.fold();
    assert_eq!(
        fold.frozen_at,
        Some(source(&fixture.space, &fixture.item, &accepted, true))
    );
    assert!(matches!(
        fold.slots.last().unwrap().acceptance,
        Acceptance::Canonical(_)
    ));
}
