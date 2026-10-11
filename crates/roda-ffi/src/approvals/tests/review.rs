use super::*;

fn frame(e: &Event) -> roda_proto::Sequenced {
    roda_proto::Sequenced {
        seq: e.seq,
        prev: e.prev.clone(),
        hash: e.hash.clone(),
        env: roda_proto::Envelope::plain(e),
    }
}

fn confirm_pending(e: &mut Engine) {
    for pending in e.store.outbox().unwrap() {
        let space = &pending.event.space;
        let mut log =
            roda_log::SpaceLog::from_events(space, e.logs[space].events().to_vec()).unwrap();
        let event = log.sequence(pending.event).clone();
        assert_eq!(e.ingest(frame(&event)), crate::sync::Ingest::Confirmed);
    }
}

#[test]
fn a_newer_standing_decision_stops_an_interrupted_batch_after_restart() {
    for allow in [false, true] {
        let (chat, mut e) = Chat::create();
        for prompt in ["first", "second"] {
            e.agent_create_plan(
                &chat.space,
                &chat.agent,
                prompt,
                plan(),
                "Scripted model",
                37,
            )
            .unwrap();
        }
        let first = e.state.request_order[0].clone();
        let second = e.state.request_order[1].clone();
        e.store.conn().execute_batch(&format!("CREATE TRIGGER fail_covered BEFORE INSERT ON events WHEN NEW.json LIKE '%RequestResolved%' AND NEW.json LIKE '%{second}%' BEGIN SELECT RAISE(ABORT, 'covered fault'); END;")).unwrap();
        assert!(e
            .decide_request(&first, RequestDecision::AlwaysApprove)
            .is_err());
        e.store
            .conn()
            .execute_batch("DROP TRIGGER fail_covered")
            .unwrap();
        e.append(
            &chat.space,
            &chat.owner,
            EventBody::GrantIssued {
                grant: Grant {
                    id: new_id("gr_replacement"),
                    grantor: chat.owner.clone(),
                    grantee: Some(chat.agent.clone()),
                    scope: GrantScope::Space(chat.space.clone()),
                    capability: Capability::Standing {
                        action: roda_grants::standing_key(&ActionClass::Reversible).into(),
                        allow,
                    },
                    expires_at_ms: None,
                },
            },
        )
        .unwrap();
        drop(e);
        let mut e = chat.reopen();
        let count = e.store.event_count().unwrap();
        assert!(
            e.decide_request(&first, RequestDecision::AlwaysApprove)
                .is_err(),
            "the old batch bypassed a replacement standing decision (allow={allow})"
        );
        assert_eq!(e.state.requests[&second].status, ReqStatus::Pending);
        assert_eq!(e.items().len(), 1);
        assert_eq!(e.store.event_count().unwrap(), count);
        assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(74));
    }
}

#[test]
fn an_ordered_denial_wins_over_an_optimistic_approval_before_its_echo() {
    let (chat, mut e) = Chat::create();
    e.agent_create_plan(
        &chat.space,
        &chat.agent,
        "Proposal",
        plan(),
        "Scripted model",
        37,
    )
    .unwrap();
    let r = e.state.requests[&e.state.request_order[0]].clone();
    e.store.set_synced(&chat.space).unwrap();
    e.net.synced.insert(chat.space.clone());
    e.decide_request(&r.req.id, RequestDecision::AlwaysApprove)
        .unwrap();
    let pending = e.store.outbox().unwrap().into_iter().next().unwrap().event;
    let other_device =
        roda_log::Author::device(&e.signers[&chat.owner], roda_log::Signer::generate());
    let mut log =
        roda_log::SpaceLog::from_events(&chat.space, e.logs[&chat.space].events().to_vec())
            .unwrap();
    let signed = other_device.sign_event(
        &chat.space,
        &new_ulid(crate::engine::now_ms()),
        crate::engine::now_ms(),
        log.head(),
        EventBody::RequestResolved {
            request: r.req.id.clone(),
            approved: false,
            content_hash: r.req.content_hash,
            resolution: Some(RequestResolution {
                message: "Declined on another device".into(),
                created_item: None,
                ai_cost_cents: 37,
                standing_grant: None,
            }),
        },
    );
    let denied = log.sequence(signed).clone();
    assert_eq!(
        e.ingest(roda_proto::Sequenced {
            seq: denied.seq,
            prev: denied.prev.clone(),
            hash: denied.hash.clone(),
            env: roda_proto::Envelope::plain(&denied)
        }),
        crate::sync::Ingest::Applied
    );
    assert_eq!(e.state.requests[&r.req.id].status, ReqStatus::Denied);
    assert!(e.items().is_empty());
    assert!(!e
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == format!("gr_standing_{}", r.req.id)));
    let losing = log.sequence(pending).clone();
    assert_eq!(
        e.ingest(roda_proto::Sequenced {
            seq: losing.seq,
            prev: losing.prev.clone(),
            hash: losing.hash.clone(),
            env: roda_proto::Envelope::plain(&losing)
        }),
        crate::sync::Ingest::Confirmed
    );
    assert_eq!(e.outbox_len(), 0);
    assert_eq!(e.state.requests[&r.req.id].status, ReqStatus::Denied);
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.state.requests[&r.req.id].status, ReqStatus::Denied);
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

#[test]
fn ordered_agent_removal_invalidates_an_optimistic_approval_before_its_echo() {
    let (chat, mut e) = Chat::create();
    e.agent_create_plan(
        &chat.space,
        &chat.agent,
        "Proposal",
        plan(),
        "Scripted model",
        37,
    )
    .unwrap();
    let id = e.state.request_order[0].clone();
    e.store.set_synced(&chat.space).unwrap();
    e.net.synced.insert(chat.space.clone());
    e.decide_request(&id, RequestDecision::AlwaysApprove)
        .unwrap();
    let pending = e.store.outbox().unwrap().into_iter().next().unwrap().event;
    ingest_signed(
        &mut e,
        &chat.space,
        &chat.owner,
        EventBody::MemberRemoved {
            identity: chat.agent.clone(),
        },
    );
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    assert!(!e
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == format!("gr_standing_{id}")));
    let mut log =
        roda_log::SpaceLog::from_events(&chat.space, e.logs[&chat.space].events().to_vec())
            .unwrap();
    let losing = log.sequence(pending).clone();
    assert_eq!(
        e.ingest(roda_proto::Sequenced {
            seq: losing.seq,
            prev: losing.prev.clone(),
            hash: losing.hash.clone(),
            env: roda_proto::Envelope::plain(&losing)
        }),
        crate::sync::Ingest::Confirmed
    );
    assert_eq!(e.outbox_len(), 0);
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    drop(e);
    let e = chat.reopen();
    assert_eq!(e.state.requests[&id].status, ReqStatus::Pending);
    assert!(e.items().is_empty());
    assert_eq!(e.agent_profile(&chat.agent).budget_spent_cents, Some(37));
}

#[test]
fn another_member_cannot_preempt_or_revoke_an_approval_generated_grant() {
    for standing in [false, true] {
        let (chat, mut e) = Chat::create();
        let stranger = e
            .create_identity(identity(IdentityKind::Person, None), true)
            .unwrap();
        e.append(
            &chat.space,
            &chat.owner,
            EventBody::MemberAdded {
                identity: stranger.clone(),
                role: Role::Member,
            },
        )
        .unwrap();
        e.agent_create_app(
            &chat.space,
            &chat.agent,
            "adopt_pet",
            r#"{"name":"Durable Donkey"}"#,
            "Scripted model",
            "Create this pet",
        )
        .unwrap();
        let id = e.state.request_order[0].clone();
        let collision = format!("gr_{}_{id}", if standing { "standing" } else { "app" });
        ingest_signed(
            &mut e,
            &chat.space,
            &stranger,
            EventBody::GrantIssued {
                grant: Grant {
                    id: collision.clone(),
                    grantor: stranger.clone(),
                    grantee: None,
                    scope: GrantScope::Space(chat.space.clone()),
                    capability: Capability::Standing {
                        action: "reversible".into(),
                        allow: true,
                    },
                    expires_at_ms: None,
                },
            },
        );
        e.decide_request(
            &id,
            if standing {
                RequestDecision::AlwaysApprove
            } else {
                RequestDecision::Approve
            },
        )
        .unwrap();
        confirm_pending(&mut e);
        ingest_signed(
            &mut e,
            &chat.space,
            &stranger,
            EventBody::GrantRevoked {
                grant: collision.clone(),
            },
        );
        let item = e.items()[0].id.clone();
        assert_eq!(
            e.item(&item).unwrap().app.unwrap().trust,
            TrustLevelDto::Act
        );
        let synthetic: Vec<_> = e
            .state
            .grants
            .iter()
            .filter(|g| g.grant.id == collision)
            .collect();
        assert_eq!(synthetic.len(), 1);
        assert_eq!(synthetic[0].grant.grantor, chat.owner);
        assert!(!synthetic[0].revoked);
        drop(e);
        let mut e = chat.reopen();
        assert_eq!(
            e.item(&item).unwrap().app.unwrap().trust,
            TrustLevelDto::Act
        );
        ingest_signed(
            &mut e,
            &chat.space,
            &chat.owner,
            EventBody::GrantRevoked {
                grant: format!("gr_app_{id}"),
            },
        );
        assert_eq!(
            e.item(&item).unwrap().app.unwrap().trust,
            TrustLevelDto::Listen
        );
    }
}

#[test]
fn another_members_device_grant_cannot_authorize_this_owners_phone() {
    let (chat, mut e) = Chat::create();
    e.agent_create_app(
        &chat.space,
        &chat.agent,
        "adopt_pet",
        r#"{"name":"Permission test"}"#,
        "Scripted model",
        "Create this pet",
    )
    .unwrap();
    let id = e.state.request_order[0].clone();
    e.resolve_request(&id, true).unwrap();
    let item = e.items()[0].id.clone();
    let stranger = e
        .create_identity(identity(IdentityKind::Person, None), true)
        .unwrap();
    e.append(
        &chat.space,
        &chat.owner,
        EventBody::MemberAdded {
            identity: stranger.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    ingest_signed(
        &mut e,
        &chat.space,
        &stranger,
        EventBody::GrantIssued {
            grant: Grant {
                id: new_id("gr"),
                grantor: stranger.clone(),
                grantee: Some(format!("app:{item}")),
                scope: GrantScope::Item(item.clone()),
                capability: Capability::Device {
                    capability: "camera".into(),
                    purpose: "Another member's consent".into(),
                },
                expires_at_ms: None,
            },
        },
    );
    assert!(!e.app_device_allowed(&item, "camera"));
    drop(e);
    let mut e = chat.reopen();
    assert!(!e.app_device_allowed(&item, "camera"));
    e.grant_app_device(&item, "camera", "This owner's consent", true)
        .unwrap();
    assert!(e.app_device_allowed(&item, "camera"));
}
