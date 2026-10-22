use super::*;

fn queue_mode(f: &PageFixture) {
    let mut e = f.actor.agent.lock();
    e.store.set_synced(&f.actor.space).unwrap();
    e.net.synced.insert(f.actor.space.clone());
}
fn ack(f: &PageFixture, cid: &str) {
    let mut e = f.actor.agent.lock();
    let p = e
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|p| p.event.client_id == cid)
        .unwrap();
    let head = e.logs[&f.actor.space].head();
    let seq = head.as_ref().map_or(0, |h| h.seq + 1);
    let prev = head.map_or_else(|| types::GENESIS_PREV.into(), |h| h.hash);
    let env = protocol::Envelope::plain(&p.event);
    let frame = protocol::Sequenced {
        seq,
        hash: event_log::chain_hash(&f.actor.space, seq, &prev, &env.wire_hash()),
        prev,
        env,
    };
    assert_eq!(e.ingest(frame), crate::sync::Ingest::Confirmed);
}

#[test]
fn queued_is_not_confirmed_and_exact_receipt_survives_ack_and_later_head() {
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (request, permit) = approved(&f, &t, &c);
    queue_mode(&f);
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(accepted.delivery, AgentPageDelivery::Queued);
    assert_eq!(f.actor.agent.lock().store.outbox_len().unwrap(), 1);
    let before = counts(&f);
    let replay = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.client_id, accepted.client_id);
    assert_eq!(counts(&f), before);
    assert!(f
        .actor
        .agent
        .agent_page_turn(&f.page, &f.actor.source, &f.actor.owner)
        .is_err());
    ack(&f, &accepted.client_id);
    let current = page(&f);
    let mut other = current.blocks.last().unwrap().clone();
    other.text = "Later human version".into();
    f.actor
        .agent
        .page_apply_from(
            f.page.clone(),
            "later-version".into(),
            current.edit_context,
            current.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![other],
        )
        .unwrap();
    f.actor
        .agent
        .page_commit(f.page.clone(), "Later version".into())
        .unwrap();
    let later = f
        .actor
        .agent
        .lock()
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .event
        .client_id;
    ack(&f, &later);
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    let before = counts(&f);
    let replay = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(replay.client_id, accepted.client_id);
    assert_eq!(replay.observed_base_version, 1);
    assert_eq!(replay.current_version, 3);
    assert_eq!(replay.delivery, AgentPageDelivery::Confirmed);
    assert!(replay.replayed);
    assert_eq!(counts(&f), before);
}

#[test]
fn lost_outer_response_and_process_cold_reopen_return_same_publication() {
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (request, permit) = approved(&f, &t, &c);
    queue_mode(&f);
    // Discarding a completed API result models loss strictly after outer RELEASE.
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    let dir = std::env::temp_dir().join(types::new_id("page-command-cold"));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("state.sqlite");
    f.actor
        .agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let vault = f.actor.agent_vault.clone();
    drop(permit);
    drop(t);
    drop(f);
    let cold = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(cold.unlock(vault).unwrap());
    let permit = cold.approved_page_tool_turn(&request.id).unwrap();
    let count = cold.lock().store.outbox_len().unwrap();
    let replay = cold.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(replay.client_id, accepted.client_id);
    assert_eq!(replay.observed_base_version, accepted.observed_base_version);
    assert_eq!(
        replay.applied_content_sha256,
        accepted.applied_content_sha256
    );
    assert!(replay.replayed);
    assert_eq!(cold.lock().store.outbox_len().unwrap(), count);
    drop(cold);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn real_sql_faults_after_apply_publication_and_receipt_rollback_entire_command() {
    for fail_at in [1, 2, 3] {
        let f = fixture();
        let t = turn(&f);
        let c = command(&t);
        let (_, permit) = approved(&f, &t, &c);
        queue_mode(&f);
        // Three actual encrypted journal saves: accepted edit, publication marker,
        // and linked command receipt. The trigger rolls back with the savepoint.
        f.actor.agent.lock().store.conn().execute_batch(&format!("CREATE TABLE page_fault(n INTEGER);INSERT INTO page_fault VALUES(0);CREATE TRIGGER page_journal_fault BEFORE INSERT ON meta WHEN NEW.key LIKE 'page.draft.%' BEGIN UPDATE page_fault SET n=n+1;SELECT CASE WHEN (SELECT n FROM page_fault)={fail_at} THEN RAISE(ABORT,'synthetic Page journal fault') END;END;")).unwrap();
        let before = counts(&f);
        let baseline = page(&f);
        assert!(
            f.actor.agent.agent_page_replace(permit.turn(), &c).is_err(),
            "save{fail_at}"
        );
        assert_eq!(counts(&f), before);
        assert_eq!(page(&f).blocks, baseline.blocks);
        assert!(!page(&f).unsaved);
        f.actor
            .agent
            .lock()
            .store
            .conn()
            .execute_batch("DROP TRIGGER page_journal_fault")
            .unwrap();
        let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
        assert_eq!(accepted.observed_base_version, 1);
    }
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (_, permit) = approved(&f, &t, &c);
    queue_mode(&f);
    f.actor.agent.lock().store.conn().execute_batch("CREATE TRIGGER page_outbox_fault BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'synthetic Page outbox fault');END;").unwrap();
    let before = counts(&f);
    assert!(f.actor.agent.agent_page_replace(permit.turn(), &c).is_err());
    assert_eq!(counts(&f), before);
    assert!(!page(&f).unsaved);
}

#[test]
fn permanent_refusal_recovery_reports_the_original_cid_without_retrying_it() {
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (_, permit) = approved(&f, &t, &c);
    queue_mode(&f);
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    f.actor
        .agent
        .lock()
        .store
        .outbox_note(&accepted.client_id, "synthetic permanent refusal", true)
        .unwrap();
    let before = counts(&f);
    let result = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(result.delivery, AgentPageDelivery::Refused);
    assert_eq!(result.client_id, accepted.client_id);
    assert!(result.replayed);
    assert_eq!(counts(&f), before);
    assert!(f
        .actor
        .agent
        .agent_page_turn(&f.page, &f.actor.source, &f.actor.owner)
        .is_err());
}

#[test]
fn valid_member_same_cid_is_not_confirmation_and_does_not_mask_the_later_exact_echo() {
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (_, permit) = approved(&f, &t, &c);
    queue_mode(&f);
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    {
        let mut e = f.actor.agent.lock();
        let mut log = event_log::SpaceLog::from_events(
            &f.actor.space,
            e.logs[&f.actor.space].events().to_vec(),
        )
        .unwrap();
        let author = e.author_for(&f.actor.owner).unwrap();
        let at = crate::engine::now_ms();
        let signed = author.sign_event(
            &f.actor.space,
            &accepted.client_id,
            at,
            log.head(),
            EventBody::MessagePosted {
                message: types::new_id("foreign_alias"),
                text: "Valid foreign same-CID event".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        );
        let event = log.sequence(signed).clone();
        assert_eq!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)
            }),
            crate::sync::Ingest::Applied
        );
        assert_eq!(e.store.outbox_len().unwrap(), 1);
    }
    assert_eq!(
        f.actor
            .agent
            .agent_page_replace(permit.turn(), &c)
            .unwrap()
            .delivery,
        AgentPageDelivery::Queued
    );
    assert!(f.actor.agent.timeline(f.actor.space.clone()).unwrap().iter().any(|m|m.author.id==f.actor.owner && matches!(&m.kind,crate::EntryKind::Message {text,..} if text=="Valid foreign same-CID event")));
    ack(&f, &accepted.client_id);
    let before = counts(&f);
    let confirmed = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(confirmed.delivery, AgentPageDelivery::Confirmed);
    assert_eq!(confirmed.client_id, accepted.client_id);
    assert_eq!(counts(&f), before);
}

#[test]
fn concurrent_peer_publication_does_not_turn_local_result_into_a_canonical_version() {
    use base64::Engine as _;
    let f = fixture();
    let t = turn(&f);
    let c = command(&t);
    let (request, permit) = approved(&f, &t, &c);
    // The peer edits the same published base, before our command is queued.
    // This is an actual independent CRDT peer, not a fabricated merged view.
    let remote = documents::Page::new();
    let original = {
        let e = f.actor.agent.lock();
        let types::ItemContent::Page(original) = &e.state.items[&f.page].current().content else {
            panic!("ordinary Page")
        };
        original.clone()
    };
    assert!(original.blob.is_none());
    remote
        .import(
            &base64::engine::general_purpose::STANDARD
                .decode(&original.loro)
                .unwrap(),
        )
        .unwrap();
    let baseline = remote.version();
    let remote_blocks = remote.blocks();
    let sibling = remote_blocks.last().unwrap();
    remote
        .apply(
            &remote_blocks
                .iter()
                .map(|b| b.id.clone())
                .collect::<Vec<_>>(),
            &[documents::BlockEdit {
                id: sibling.id.clone(),
                kind: sibling.kind.clone(),
                text: "Concurrent owner sibling".into(),
                spans: Vec::new(),
            }],
        )
        .unwrap();
    queue_mode(&f);
    let accepted = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(accepted.delivery, AgentPageDelivery::Queued);
    {
        let mut e = f.actor.agent.lock();
        let mut log = event_log::SpaceLog::from_events(
            &f.actor.space,
            e.logs[&f.actor.space].events().to_vec(),
        )
        .unwrap();
        let signed = e.author_for(&f.actor.owner).unwrap().sign_event(
            &f.actor.space,
            &types::new_id("peer_page"),
            crate::engine::now_ms(),
            log.head(),
            EventBody::ItemVersioned {
                item: f.page.clone(),
                content: types::ItemContent::Page(types::PageContent {
                    loro: base64::engine::general_purpose::STANDARD
                        .encode(remote.updates_since(&baseline)),
                    ..original
                }),
                note: "Concurrent owner publication".into(),
            },
        );
        let event = log.sequence(signed).clone();
        assert_eq!(
            e.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)
            }),
            crate::sync::Ingest::Applied
        );
        assert_eq!(e.store.outbox_len().unwrap(), 1);
    }
    ack(&f, &accepted.client_id);
    let current = page(&f);
    assert_eq!(current.version, 3);
    assert!(current.blocks.iter().any(|b| b.text == c.after.text));
    assert!(current
        .blocks
        .iter()
        .any(|b| b.text == "Concurrent owner sibling"));
    assert_ne!(current.content_hash, accepted.applied_content_sha256);
    let permit = f.actor.agent.approved_page_tool_turn(&request.id).unwrap();
    let before = counts(&f);
    let replay = f.actor.agent.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(replay.delivery, AgentPageDelivery::Confirmed);
    assert_eq!(replay.client_id, accepted.client_id);
    assert_eq!(counts(&f), before);
    // The local accepted result does not claim a relay ordinal or merged hash.
    assert_eq!(replay.observed_base_version, t.snapshot.version);
    assert_eq!(replay.current_version, current.version);
    assert_eq!(
        replay.applied_content_sha256,
        accepted.applied_content_sha256
    );
    let wire = serde_json::to_value(&replay).unwrap();
    assert!(wire.get("version").is_none() && wire.get("contentSha256").is_none());
    let dir = std::env::temp_dir().join(types::new_id("concurrent-page-cold"));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("state.sqlite");
    f.actor
        .agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let vault = f.actor.agent_vault.clone();
    drop(permit);
    drop(t);
    drop(f);
    let cold = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(cold.unlock(vault).unwrap());
    let permit = cold.approved_page_tool_turn(&request.id).unwrap();
    let events = cold.lock().store.event_count().unwrap();
    let recovered = cold.agent_page_replace(permit.turn(), &c).unwrap();
    assert_eq!(recovered, replay);
    assert_eq!(cold.page(c.artifact_id).unwrap().blocks, current.blocks);
    assert_eq!(cold.lock().store.outbox_len().unwrap(), 0);
    assert_eq!(cold.lock().store.event_count().unwrap(), events);
    drop(cold);
    std::fs::remove_dir_all(dir).unwrap();
}
