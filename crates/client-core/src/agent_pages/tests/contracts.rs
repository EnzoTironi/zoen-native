use super::*;

#[test]
fn published_read_and_exact_owner_approval_replace_only_one_block() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    assert_eq!(
        f.actor.agent.agent_page_finish_read(&t).unwrap(),
        t.snapshot
    );
    let before = counts(&f);
    // Act is not a Page execution permit.
    assert!(f.actor.agent.agent_page_replace(&t, &c).is_err());
    assert_eq!(counts(&f), before);
    let request = stage(&f, &t, &c);
    assert_eq!(stage(&f, &t, &c).id, request.id);
    let dto = request.tool_approval.as_ref().unwrap();
    assert!(dto.operation.contains("Page block"));
    assert!(request.detail.contains(&c.before.text) && request.detail.contains(&c.after.text));
    assert!(!request.can_always_approve);
    decide(&f, &request, ToolRequestDecision::ApproveOnce).unwrap();
    let approved = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    assert_eq!(approved.proposal().schema, 2);
    let saved = f
        .actor
        .agent
        .agent_page_replace(approved.turn(), &c)
        .unwrap();
    assert_eq!(saved.observed_base_version, 1);
    assert_eq!(saved.delivery, AgentPageDelivery::Confirmed);
    assert!(!saved.replayed);
    let now = page(&f);
    for old in &t.snapshot.blocks {
        let current = now.blocks.iter().find(|b| b.id == old.id).unwrap();
        assert_eq!(current, if old.id == c.before.id { &c.after } else { old });
    }
    assert!(!now.unsaved);
    assert_eq!(now.content_hash, saved.applied_content_sha256);
    let history = f.actor.agent.page_at(f.page.clone(), 1).unwrap();
    assert_eq!(history.blocks, t.snapshot.blocks);
    assert_eq!(
        f.actor
            .agent
            .restore_version(f.page.clone(), 1)
            .unwrap()
            .version,
        3
    );
    assert_eq!(page(&f).blocks, t.snapshot.blocks);
}

#[test]
fn deny_cancel_wrong_arguments_and_later_block_cannot_publish() {
    for decision in [ToolRequestDecision::Deny, ToolRequestDecision::Cancel] {
        let f = fixture().unwrap();
        let t = turn(&f);
        let c = command(&t);
        let req = stage(&f, &t, &c);
        decide(&f, &req, decision).unwrap();
        let before = counts(&f);
        assert!(f.actor.agent.approved_page_tool_turn(&req.id).is_err());
        assert!(f.actor.agent.agent_page_replace(&t, &c).is_err());
        assert_eq!(counts(&f), before);
        assert_eq!(page(&f).version, 1);
    }
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let (_, permit) = approved(&f, &t, &c);
    let mut altered = c.clone();
    altered.after.text.push('!');
    assert!(f
        .actor
        .agent
        .agent_page_replace(permit.turn(), &altered)
        .is_err());
    block(&f).unwrap();
    let before = counts(&f);
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).version, 1);
}

#[test]
fn private_draft_is_not_read_or_published_and_stale_before_is_not_reviewed() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let initial = page(&f);
    let mut private = c.before.clone();
    private.text = "PRIVATE INPUT".into();
    f.actor
        .agent
        .page_apply_from(
            f.page.clone(),
            "human-private".into(),
            initial.edit_context,
            initial.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![private.clone()],
        )
        .unwrap();
    assert!(page(&f).unsaved);
    assert_eq!(
        f.actor.agent.agent_page_finish_read(&t).unwrap(),
        t.snapshot
    );
    let before = counts(&f);
    assert!(f
        .actor
        .agent
        .stage_page_tool_approval(
            &t,
            input(
                &f,
                &t,
                "replace_page_block",
                serde_json::to_value(&c).unwrap()
            )
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(
        page(&f).blocks.iter().find(|b| b.id == private.id).unwrap(),
        &private
    );
    let f = fixture().unwrap();
    let t = turn(&f);
    let mut c = command(&t);
    c.before.text = "Invented before".into();
    assert!(f
        .actor
        .agent
        .stage_page_tool_approval(
            &t,
            input(
                &f,
                &t,
                "replace_page_block",
                serde_json::to_value(&c).unwrap()
            )
        )
        .is_err());
    let mut args = serde_json::to_value(command(&t)).unwrap();
    args["after"]["injectedField"] = true.into();
    assert!(tools::parse("replace_page_block", &args).is_err());
    let mut c = command(&t);
    c.after.kind = "quote".into();
    assert!(tools::parse("replace_page_block", &serde_json::to_value(c).unwrap()).is_err());
    let mut c = command(&t);
    c.after.text = "x".into();
    assert!(
        f.actor
            .agent
            .stage_page_tool_approval(
                &t,
                input(
                    &f,
                    &t,
                    "replace_page_block",
                    serde_json::to_value(c).unwrap()
                )
            )
            .is_err(),
        "the preserved bold span is outside the shortened text"
    );
}

#[test]
fn source_device_membership_and_real_new_head_are_rechecked_after_review() {
    for boundary in 0..4 {
        let f = fixture().unwrap();
        let t = turn(&f);
        let c = command(&t);
        let (_, permit) = approved(&f, &t, &c);
        {
            let mut e = f.actor.agent.lock();
            match boundary {
                0 => {
                    e.append(
                        &f.actor.space,
                        &f.actor.owner,
                        EventBody::MessageAnnotated {
                            target: f.actor.source.clone(),
                            note: types::Annotation::Edit {
                                text: "Changed intent".into(),
                            },
                        },
                    )
                    .unwrap();
                }
                1 => {
                    let device = e.account().unwrap().device.clone();
                    e.store
                        .set_meta("devices.revoked", &json!([device]).to_string())
                        .unwrap();
                }
                2 => {
                    let me = e.me_id().unwrap();
                    e.append(
                        &f.actor.space,
                        &me,
                        EventBody::MemberAdded {
                            identity: f.actor.owner.clone(),
                            role: types::Role::Reader,
                        },
                    )
                    .unwrap();
                }
                3 => {
                    let dto = e.page(&f.page).unwrap();
                    let mut block = c.before.clone();
                    block.text = "Concurrent human edit".into();
                    e.page_apply_from(
                        &f.page,
                        "concurrent",
                        &dto.edit_context,
                        &dto.blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
                        &[block],
                    )
                    .unwrap();
                    e.page_commit(&f.page, "new head").unwrap();
                }
                _ => unreachable!(),
            }
        }
        let before = counts(&f);
        assert!(
            f.actor.agent.agent_page_replace(permit.turn(), &c).is_err(),
            "boundary{boundary}"
        );
        assert_eq!(counts(&f), before);
    }
}

#[test]
fn active_empty_input_binding_blocks_agent_write_without_discard_or_capture() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let (_, permit) = approved(&f, &t, &c);
    let account = f.actor.agent.account().unwrap();
    let dto = page(&f);
    let scope = crate::FileEditDraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: f.actor.space.clone(),
        item_id: f.page.clone(),
    };
    let binding = f
        .actor
        .agent
        .page_input_open(
            scope.clone(),
            0,
            "human-editor".into(),
            crate::PageInputTarget::Block {
                block_id: c.before.id.clone(),
            },
            crate::PageInputMode::RawRich,
            dto.edit_context,
        )
        .unwrap();
    assert!(binding.binding.is_some() && binding.capture.is_none());
    let before = counts(&f);
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(f.actor.agent.page_input_load(scope).unwrap(), binding);
    assert_eq!(
        f.actor.agent.agent_page_finish_read(&t).unwrap(),
        t.snapshot
    );
}

#[test]
fn semantically_reverted_private_history_is_not_included_in_agent_publication() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    for (id, text) in [
        ("private-first", "Private unsaved secret"),
        ("private-revert", c.before.text.as_str()),
    ] {
        let dto = page(&f);
        let mut edit = c.before.clone();
        edit.text = text.into();
        f.actor
            .agent
            .page_apply_from(
                f.page.clone(),
                id.into(),
                dto.edit_context,
                dto.blocks.iter().map(|b| b.id.clone()).collect(),
                vec![edit],
            )
            .unwrap();
    }
    let current = page(&f);
    assert!(!current.unsaved);
    assert_eq!(current.blocks, t.snapshot.blocks);
    // Equal visible bytes do not prove an editor's CRDT history was published.
    let before = counts(&f);
    assert!(f
        .actor
        .agent
        .stage_page_tool_approval(
            &t,
            input(
                &f,
                &t,
                "replace_page_block",
                serde_json::to_value(c).unwrap()
            )
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(page(&f).version, 1);
}

#[test]
fn expired_signed_page_ask_is_retained_but_never_approves_or_executes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let req = stage(&f, &t, &c);
    let mut expired = f.actor.agent.lock().state.requests[&req.id].req.clone();
    expired.id = types::new_id("expired_page_ask").unwrap();
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = crate::engine::now_ms() - 1000;
    expired.content_hash = expired.tool_proposal_hash(&f.actor.space).unwrap();
    let id = expired.id.clone();
    {
        let mut e = f.actor.agent.lock();
        let at = expired.tool_proposal.as_ref().unwrap().expires_at_ms - 1000;
        let mut log = event_log::SpaceLog::from_events(
            &f.actor.space,
            e.logs[&f.actor.space].events().to_vec(),
        )
        .unwrap();
        let author = e.author_for(&expired.agent).unwrap();
        let signed = author.sign_event(
            &f.actor.space,
            &types::new_ulid(at).unwrap(),
            at,
            log.head(),
            EventBody::RequestOpened { request: expired },
        )?;
        let event = log.sequence(signed)?.clone();
        assert_eq!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)
            }),
            crate::sync::Ingest::Applied
        );
    }
    let req = f
        .actor
        .agent
        .requests()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    let before = counts(&f);
    assert!(decide(&f, &req, ToolRequestDecision::ApproveOnce).is_err());
    assert!(f.actor.agent.approved_page_tool_turn(&id).is_err());
    assert_eq!(counts(&f), before);
    decide(&f, &req, ToolRequestDecision::Cancel).unwrap();
    assert_eq!(page(&f).version, 1);
    Ok(())
}

#[test]
fn page_read_approval_and_another_space_cannot_authorize_a_write() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let c = command(&t);
    let req = f
        .actor
        .agent
        .stage_page_tool_approval(&t, input(&f, &t, "read_page", json!({"artifactId":f.page})))
        .unwrap();
    decide(&f, &req, ToolRequestDecision::ApproveOnce).unwrap();
    let permit = f.actor.agent.approved_page_tool_turn(&req.id).unwrap();
    assert_eq!(
        f.actor.agent.agent_page_finish_read(permit.turn()).unwrap(),
        t.snapshot
    );
    let before = counts(&f);
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
    assert_eq!(counts(&f), before);
    let space = types::new_id("other_page_space").unwrap();
    {
        let mut e = f.actor.agent.lock();
        let me = e.me_id().unwrap();
        e.append(
            &space,
            &me,
            EventBody::SpaceCreated {
                title: "Other Page scope".into(),
                kind: types::SpaceKind::Group,
                privacy: types::Privacy::Closed,
            },
        )
        .unwrap();
        e.append(
            &space,
            &me,
            EventBody::MemberAdded {
                identity: f.actor.owner.clone(),
                role: types::Role::Member,
            },
        )
        .unwrap();
    }
    let other = f
        .actor
        .agent
        .page_create(space, "Other authorized Page".into())
        .unwrap();
    assert!(f
        .actor
        .agent
        .agent_page_turn(&other.id, &f.actor.source, &f.actor.owner)
        .is_err());
    let mut c = c;
    c.artifact_id = other.id;
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
}
