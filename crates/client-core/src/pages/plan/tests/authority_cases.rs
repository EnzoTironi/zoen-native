//! authority cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn transport_confirmation_cannot_ack_a_semantically_refused_page_or_late_legacy_undo() {
    let directory = std::env::temp_dir().join(new_id("plan-refused-outbox-reopen").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("plan.sqlite");
    let fixture = Fixture::at(path.to_str().unwrap()).unwrap();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Canonical saved text".into()),
            ..Default::default()
        },
    );
    let valid = page(baseline.manifest().unwrap(), &document);
    let refused = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(PageContent {
            manifest: None,
            ..valid.clone()
        })),
    );
    assert!(event_log::verify_author(&refused).is_ok());
    let raw = fixture.engine.store.events(&fixture.space).unwrap();
    // A pre-upgrade signed outbox fact must survive recovery without being admitted
    // as a new publication or projected as an authorized Plan version.
    fixture.engine.store.outbox_put(&refused).unwrap();
    let Fixture {
        engine,
        space,
        item,
        owner,
    } = fixture;
    drop(engine);
    let mut fixture = Fixture {
        engine: Engine::open(path.to_str().unwrap()).unwrap(),
        space,
        item,
        owner,
    };
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), raw);
    assert_eq!(
        fixture.engine.store.outbox_get(&refused.client_id).unwrap(),
        Some(refused.clone())
    );
    assert_eq!(
        fixture.engine.net.pending.get(&refused.client_id),
        Some(&fixture.space)
    );
    assert_eq!(fixture.engine.state.items[&fixture.item].versions.len(), 1);
    assert_eq!(view(&fixture.fold()), original());
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Pending
    ));
    let refused_source = source(&fixture.space, &fixture.item, &refused, false);
    deliver(&mut fixture.engine, &refused, Ingest::Confirmed);
    assert!(!fixture
        .fold()
        .acknowledges(&refused.client_id, &refused_source.event_id));
    assert!(fixture.fold().frozen_at.is_none());
    assert!(fixture
        .engine
        .store
        .outbox_get(&refused.client_id)
        .unwrap()
        .is_none());
    assert!(!fixture.engine.net.pending.contains_key(&refused.client_id));
    assert_eq!(fixture.engine.state.items[&fixture.item].versions.len(), 1);
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));

    let valid = fixture.queue(fixture.version(ItemContent::Page(valid)));
    let valid_source = source(&fixture.space, &fixture.item, &valid, false);
    assert!(!fixture
        .fold()
        .acknowledges(&valid.client_id, &valid_source.event_id));
    let confirmed = deliver(&mut fixture.engine, &valid, Ingest::Confirmed);
    let fold = fixture.fold();
    assert_eq!(fold.slots[2].source.event_id, valid_source.event_id);
    assert!(fold.acknowledges(&valid.client_id, &valid_source.event_id));
    assert!(!fold.acknowledges(&valid.client_id, &refused_source.event_id));
    assert!(!fold.acknowledges(&refused.client_id, &valid_source.event_id));
    let frozen = fold.frozen_at;
    let undo = fixture.ordered(fixture.undo(1));
    let old_write = fixture.ordered(fixture.version(ItemContent::Plan(original())));
    let fold = fixture.fold();
    assert_eq!(fold.frozen_at, frozen);
    assert_eq!(fold.accepted_current, Some(3));
    assert_eq!(view(&fold), document.view().unwrap().to_plan_doc());
    assert_eq!(fold.manifest().unwrap().legacy_versions, 1);
    assert_eq!(fold.slots[3].reverted_to, Some(1));
    assert_eq!(fold.slots[3].content, Some(ItemContent::Plan(original())));
    for event in [&undo, &old_write] {
        assert!(!fold.acknowledges(
            &event.client_id,
            &source(&fixture.space, &fixture.item, event, true).event_id
        ));
    }
    assert_eq!(
        fold.slots[2].source.ordered,
        Some((confirmed.seq, confirmed.hash))
    );
    let raw = fixture.engine.store.events(&fixture.space).unwrap();
    let space = fixture.space.clone();
    let item = fixture.item.clone();
    drop(fixture);
    let reopened = Engine::open(path.to_str().unwrap()).unwrap();
    let recovered = reopened.plan_timeline(&item).unwrap();
    assert_eq!(recovered.slots, fold.slots);
    assert_eq!(recovered.frozen_at, fold.frozen_at);
    assert_eq!(recovered.accepted_current, fold.accepted_current);
    assert_eq!(view(&recovered), view(&fold));
    assert_eq!(reopened.store.events(&space).unwrap(), raw);
    assert!(reopened.logs[&space].verify().is_ok());
    assert!(!recovered.acknowledges(&refused.client_id, &refused_source.event_id));
    assert!(recovered.acknowledges(&valid.client_id, &valid_source.event_id));
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
pub(super) fn reader_and_removed_member_frames_never_authorize_canonical_plan_mutations() {
    let mut fixture = Fixture::new().unwrap();
    let member = add_identity(&mut fixture.engine, "Member");
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
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
    assert!(fixture.fold().frozen_at.is_none());
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    let authorized =
        fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    assert_eq!(
        view(&fixture.fold()),
        document.view().unwrap().to_plan_doc()
    );
    fixture.ordered(EventBody::MemberRemoved {
        identity: member.clone(),
    });
    let removed = fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    let fold = fixture.fold();
    assert_eq!(
        fold.frozen_at,
        Some(source(&fixture.space, &fixture.item, &authorized, true))
    );
    for refused in [&reader, &removed] {
        assert!(!fold.acknowledges(
            &refused.client_id,
            &source(&fixture.space, &fixture.item, refused, true).event_id
        ));
    }
    assert!(matches!(
        fold.slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
    fixture.ordered(fixture.version(ItemContent::Page(content)));
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Canonical(_)
    ));
}

#[test]
pub(super) fn invalid_signatures_and_oversized_payloads_cannot_establish_cutover() {
    let mut fixture = Fixture::new().unwrap();
    let baseline = fixture.fold();
    let content = page(
        baseline.manifest().unwrap(),
        baseline.document.as_ref().unwrap(),
    );
    let event = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(content.clone())),
    );
    let mut forged = frame(&fixture.engine, &event);
    forged.env.sig = "00".repeat(64);
    assert!(matches!(fixture.engine.ingest(forged), Ingest::Invalid(_)));
    assert_eq!(fixture.fold().slots.len(), 1);
    assert!(fixture.fold().frozen_at.is_none());
    let huge = PageContent {
        loro: "a".repeat(MAX_PLAN_PACKET_BYTES.div_ceil(3) * 4 + 1),
        ..content.clone()
    };
    fixture.ordered(fixture.version(ItemContent::Page(huge)));
    let declared = PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: "ab".repeat(32),
            bytes: MAX_PLAN_PACKET_BYTES as u64 + 1,
            blob: "cd".repeat(32),
            key: "ef".repeat(32),
        }),
        ..content
    };
    fixture.ordered(fixture.version(ItemContent::Page(declared)));
    let fold = fixture.fold();
    assert!(fold.slots[1..]
        .iter()
        .all(|slot| matches!(slot.acceptance, Acceptance::Refused(_))));
    assert!(fold.frozen_at.is_none());
    assert!(!fold.waiting);
    assert_eq!(view(&fold), original());
}

#[test]
pub(super) fn tagged_plan_versions_preserve_author_space_and_creation_checks() {
    let mut fixture = Fixture::new().unwrap();
    let reader = add_identity(&mut fixture.engine, "Reader");
    let removed = add_identity(&mut fixture.engine, "Removed");
    fixture.ordered(EventBody::MemberAdded {
        identity: reader.clone(),
        role: Role::Reader,
    });
    fixture.ordered(EventBody::MemberAdded {
        identity: removed.clone(),
        role: Role::Member,
    });
    fixture.ordered(EventBody::MemberRemoved {
        identity: removed.clone(),
    });
    let other_space = new_id("other-plan-space").unwrap();
    fixture
        .engine
        .append(
            &other_space,
            &fixture.owner,
            EventBody::SpaceCreated {
                title: "Other conversation".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Public,
            },
        )
        .unwrap();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let content = page(baseline.manifest().unwrap(), &document);
    let mut ordinary = content.clone();
    ordinary.manifest = None;
    let mut unsupported = content.clone();
    unsupported.manifest = Some(PageManifest::Unsupported(serde_json::json!({
        "schema": "future"
    })));
    let version = fixture.version(ItemContent::Page(content.clone()));
    let refusals = [
        (
            fixture.space.clone(),
            fixture.owner.clone(),
            fixture.version(ItemContent::Page(ordinary)),
        ),
        (
            fixture.space.clone(),
            fixture.owner.clone(),
            fixture.version(ItemContent::Page(unsupported)),
        ),
        (other_space.clone(), fixture.owner.clone(), version.clone()),
        (fixture.space.clone(), reader, version.clone()),
        (fixture.space.clone(), removed, version.clone()),
        (
            fixture.space.clone(),
            fixture.owner.clone(),
            EventBody::ItemCreated {
                item: new_id("plan-page-creation").unwrap(),
                kind: ItemKind::Plan,
                content: ItemContent::Page(content.clone()),
                origin: "unsupported creation".into(),
            },
        ),
        (
            fixture.space.clone(),
            fixture.owner.clone(),
            EventBody::ItemVersioned {
                item: new_id("missing-plan").unwrap(),
                content: ItemContent::Page(content),
                note: "missing target".into(),
            },
        ),
    ];
    let saved = fixture.engine.store.events(&fixture.space).unwrap();
    let other_saved = fixture.engine.store.events(&other_space).unwrap();
    let versions = fixture
        .engine
        .item_state(&fixture.item)
        .unwrap()
        .versions
        .len();
    for (space, actor, body) in refusals {
        let author = fixture.engine.author_for(&actor).unwrap();
        assert!(fixture
            .engine
            .append_synced(&space, &author, 1_800_000_000_000, body)
            .is_err());
    }
    assert_eq!(fixture.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), saved);
    assert_eq!(
        fixture.engine.store.events(&other_space).unwrap(),
        other_saved
    );
    assert_eq!(
        fixture
            .engine
            .item_state(&fixture.item)
            .unwrap()
            .versions
            .len(),
        versions
    );
    let pending = fixture.queue(version);
    assert_eq!(fixture.engine.store.outbox_len().unwrap(), 1);
    let confirmed = deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let fold = fixture.fold();
    assert_eq!(view(&fold), document.view().unwrap().to_plan_doc());
    assert!(fold.acknowledges(
        &confirmed.client_id,
        &source(&fixture.space, &fixture.item, &confirmed, true).event_id
    ));
}
