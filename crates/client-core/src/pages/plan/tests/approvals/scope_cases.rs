//! scope cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn cross_space_ambiguity_closes_local_writes_but_preserves_scoped_confirmed_decisions(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let document = canonical(
        &mut fixture,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let request = open(&mut fixture, &agent, &document).unwrap();
    let other = new_id("other-space").unwrap();
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
            &new_id("collision").unwrap(),
            1,
            fixture.engine.logs.get(&other).and_then(|log| log.head()),
            body,
        )?;
        deliver(&mut fixture.engine, &signed, Ingest::Applied)?;
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
    Ok(())
}

#[test]
pub(super) fn contexts_are_bound_to_the_exact_decision_and_do_not_read_later_payloads() {
    let mut fixture = Fixture::new().unwrap();
    let agent = agent(&mut fixture).unwrap();
    let request = open(&mut fixture, &agent, &original()).unwrap();
    let event = fixture.ordered(decision(&request, true)).unwrap();
    let end = fixture.engine.logs[&fixture.space].len() - 1;
    let (page, _, _, _, _) = missing_blob(
        &fixture,
        LinePatch {
            text: Some("Later missing payload".into()),
            ..Default::default()
        },
    )
    .unwrap();
    fixture
        .ordered(fixture.version(ItemContent::Page(page)))
        .unwrap();
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
pub(super) fn later_foreign_creations_do_not_rewrite_the_scoped_approved_decision(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for reader in [true, false] {
        for at_ms in [1, 1_900_000_000_000] {
            let directory = std::env::temp_dir().join(new_id("late-foreign-plan-origin").unwrap());
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("plan.sqlite");
            let mut fixture = Fixture::at(path.to_str().unwrap()).unwrap();
            let agent = agent(&mut fixture).unwrap();
            let document = canonical(
                &mut fixture,
                LinePatch {
                    text: Some("Exact historically approved row".into()),
                    cost_cents: Some(317),
                    ..Default::default()
                },
            )
            .unwrap();
            let request = open(&mut fixture, &agent, &document).unwrap();
            let accepted = fixture.ordered(decision(&request, true)).unwrap();
            assert_eq!(
                fixture.engine.state.requests[&request.id].status,
                ReqStatus::Approved
            );
            assert_eq!(
                fixture.engine.state.resolved_order,
                vec![request.id.clone()]
            );
            let original_raw = fixture.engine.store.events(&fixture.space).unwrap();
            let other = new_id("unrelated-space").unwrap();
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
                    &new_id("late-foreign-event").unwrap(),
                    at_ms,
                    fixture.engine.logs.get(&other).and_then(|log| log.head()),
                    body,
                )?;
                deliver(&mut fixture.engine, &signed, Ingest::Applied)?;
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
    Ok(())
}

#[test]
pub(super) fn duplicate_request_ids_keep_each_space_history_and_close_unscoped_writes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for at_ms in [1, 1_900_000_000_000] {
        let mut fixture = Fixture::new().unwrap();
        let agent = agent(&mut fixture).unwrap();
        let document = canonical(
            &mut fixture,
            LinePatch {
                text: Some("Own authenticated row".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let request = open(&mut fixture, &agent, &document).unwrap();
        let owner = fixture.engine.author_for(&fixture.owner).unwrap();
        let agent_author = fixture.engine.author_for(&agent).unwrap();
        let other = new_id("duplicate-request-space").unwrap();
        let other_item = new_id("different-plan").unwrap();
        let mut foreign_document = original();
        foreign_document.title = "Different scoped Plan".into();
        foreign_document.sections[0].lines[0].text = "Unrelated row".into();
        foreign_document.sections[0].lines[0].cost_cents = 987;
        let line = &foreign_document.sections[0].lines[0];
        let mut foreign_request = request.clone();
        foreign_request.title = "Unrelated request".into();
        foreign_request.item = Some(other_item.clone());
        foreign_request.content_hash = content_hash(&(&line.text, line.cost_cents))?;
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
                &new_id("separate-request-event").unwrap(),
                at_ms,
                fixture.engine.logs.get(&other).and_then(|log| log.head()),
                body,
            )?;
            deliver(&mut fixture.engine, &event, Ingest::Applied)?;
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

        fixture.ordered(decision(&request, true)).unwrap();
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
    Ok(())
}
