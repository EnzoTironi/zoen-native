//! Real signed source + real document CAS; no model, service or worker simulation.
use super::tests::{counts, fixture, turn, Fixture, CHANGED, ORIGINAL};
use super::*;

pub(crate) fn scope(
    f: &mut Fixture,
    alter: impl FnOnce(&mut types::ReplyRef),
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let text = "Edit only this artifact";
    let root = f
        .agent
        .lock()
        .item_discussion(&f.space, &f.base.item_id)
        .unwrap()
        .root
        .unwrap()
        .id;
    let mut reference = types::ReplyRef::thread(root);
    reference.artifact = Some(types::ArtifactRequest {
        item_id: f.base.item_id.clone(),
        version: f.base.version,
        sha256: f.base.sha256.clone(),
        request_id: types::new_id("app_request")?,
        text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
        agent_id: None,
        intent_sha256: None,
    });
    alter(&mut reference);
    let mut e = f.agent.lock();
    let event = e
        .append(
            &f.space,
            &f.owner,
            types::EventBody::AppEditRequested {
                message: types::new_id("m")?,
                text: text.into(),
                reply: reference,
            },
        )
        .unwrap();
    f.source = event.client_id;

    Ok(())
}

#[test]
fn signed_scope_fences_other_artifacts_creation_sites_pages_images_and_stale_results(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    let other = f
        .agent
        .app_binary_create_for(
            "deckcraft".into(),
            f.space.clone(),
            "Other".into(),
            STANDARD.encode(ORIGINAL),
            "other_create".into(),
        )
        .unwrap();
    let page = f
        .agent
        .page_import_markdown(
            f.space.clone(),
            "outside.md".into(),
            "# Private sibling\n".into(),
        )
        .unwrap();
    scope(&mut f, |_| {})?;
    let t = turn(&f);
    let before = counts(&f);
    assert_eq!(
        f.agent
            .agent_artifact_request(&f.space, &f.source, &f.owner)
            .unwrap()
            .unwrap()
            .item_id,
        f.base.item_id
    );
    assert!(f
        .agent
        .agent_binary_turn(&other.item_id, &f.source, &f.owner, None, None)
        .is_err());
    assert!(f
        .agent
        .agent_page_turn(&page.id, &f.source, &f.owner)
        .is_err());
    assert!(!f.agent.can_create_agent_app(&f.space, &f.source, &f.owner));
    assert!(f
        .agent
        .lock()
        .agent_source_authority(&f.space, &f.source, &f.owner, true)
        .is_err());
    assert!(f
        .agent
        .lock()
        .image_task_actor(&f.space, &f.source, &f.owner)
        .is_err());
    assert_eq!(counts(&f), before);
    // A real mutation outside this request, even by the same actor, is lateral.
    f.agent
        .app_binary_save(
            f.base.item_id.clone(),
            f.base.version,
            f.base.sha256.clone(),
            STANDARD.encode(CHANGED),
            "lateral".into(),
            "11".repeat(32),
        )
        .unwrap();
    let before = counts(&f);
    assert!(f
        .agent
        .agent_artifact_request(&f.space, &f.source, &f.owner)
        .is_err());
    assert!(f.agent.agent_binary_finish_read(&t).is_err());
    assert!(f
        .agent
        .agent_binary_save(&t, "late", &"22".repeat(32), &STANDARD.encode(ORIGINAL))
        .is_err());
    assert_eq!(counts(&f), before);

    Ok(())
}

#[test]
fn own_accepted_chain_allows_multiple_steps_and_lost_ack_but_not_another_source(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    let first = turn(&f);
    let result = f
        .agent
        .agent_binary_save(&first, "first", &"11".repeat(32), &STANDARD.encode(CHANGED))
        .unwrap();
    assert_eq!(result.version, 2);
    let second = f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .unwrap();
    assert_eq!(second.document.version, 2);
    assert_eq!(
        f.agent
            .agent_binary_save(&first, "first", &"11".repeat(32), &STANDARD.encode(CHANGED))
            .unwrap()
            .version,
        2
    );
    let result = f
        .agent
        .agent_binary_save(
            &second,
            "second",
            &"22".repeat(32),
            &STANDARD.encode(ORIGINAL),
        )
        .unwrap();
    assert_eq!(result.version, 3);
    assert!(f.agent.agent_binary_finish_read(&second).is_ok());
    assert_eq!(
        f.agent
            .agent_binary_result(&first, "first", &"11".repeat(32))
            .unwrap()
            .unwrap()
            .version,
        2
    );
    // New source cannot inherit earlier effects by reusing its initial head.
    scope(&mut f, |_| {})?;
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_err());

    Ok(())
}

#[test]
fn malformed_forged_and_changed_signed_scope_fail_closed_without_effects(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for alter in [
        |r: &mut types::ReplyRef| r.artifact = None,
        |r: &mut types::ReplyRef| r.thread = false,
        |r: &mut types::ReplyRef| r.to = "unrelated".into(),
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().version = 0,
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().sha256 = "00".repeat(32),
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().text_sha256 = "00".repeat(32),
    ] {
        let mut f = fixture()?;
        scope(&mut f, alter)?;
        let before = counts(&f);
        assert!(f
            .agent
            .agent_artifact_request(&f.space, &f.source, &f.owner)
            .is_err());
        assert!(f
            .agent
            .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
            .is_err());
        assert!(!f.agent.can_create_agent_app(&f.space, &f.source, &f.owner));
        assert_eq!(counts(&f), before);
    }
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    let t = turn(&f);
    f.agent
        .lock()
        .append(
            &f.space,
            &f.owner,
            types::EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: types::Annotation::Edit {
                    text: "Changed request".into(),
                },
            },
        )
        .unwrap();
    assert!(f.agent.agent_binary_finish_read(&t).is_err());

    Ok(())
}

#[test]
fn scoped_receipt_survives_cold_unlock_and_requires_the_actual_signed_version_event(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    let t = turn(&f);
    let accepted = f
        .agent
        .agent_binary_save(&t, "accepted", &"11".repeat(32), &STANDARD.encode(CHANGED))
        .unwrap();
    let directory = std::env::temp_dir().join(types::new_id("artifact_scope_cold")?);
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("state.sqlite");
    f.agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let cold = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(cold.unlock(f.agent_vault.clone()).unwrap());
    assert_eq!(
        cold.agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
            .unwrap()
            .document
            .version,
        accepted.version
    );
    let account = cold.account().unwrap();
    let key = format!(
        "app_document_receipts:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&(&account.identity_id, &account.device_id, &f.base.item_id))
                .unwrap()
        ))
    );
    let mut receipts: serde_json::Value =
        serde_json::from_str(&cold.lock().store.meta(&key).unwrap().unwrap()).unwrap();
    let receipt = receipts
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "accepted")
        .unwrap();
    assert!(receipt["source"]["event"]
        .as_str()
        .is_some_and(|id| id != f.source));
    // Same version/hash/actor/source/base, but a genuine message's client_id is
    // not the accepted ItemVersioned. This damaged hint must never authorize IO.
    receipt["source"]["event"] = serde_json::json!(f.source);
    cold.lock()
        .store
        .set_meta(&key, &receipts.to_string())
        .unwrap();
    assert!(cold
        .agent_artifact_request(&f.space, &f.source, &f.owner)
        .is_err());
    assert!(cold
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_err());
    drop(cold);
    drop(f);
    std::fs::remove_dir_all(directory).unwrap();

    Ok(())
}

#[test]
fn a_scoped_prompt_under_the_legacy_discriminator_is_not_projected_or_dispatched(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    let mut e = f.agent.lock();
    let EntryBody::Message { text, reply, .. } = e
        .space_state(&f.space)
        .unwrap()
        .entries
        .iter()
        .find(|e| e.id() == f.source)
        .unwrap()
        .body
        .clone()
    else {
        panic!("source")
    };
    let event = e
        .append(
            &f.space,
            &f.owner,
            types::EventBody::MessagePosted {
                message: types::new_id("m")?,
                text,
                attaches: None,
                reply,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    assert!(!e
        .space_state(&f.space)
        .unwrap()
        .entries
        .iter()
        .any(|e| e.id() == event.client_id));
    drop(e);
    assert!(f
        .agent
        .agent_artifact_request(&f.space, &event.client_id, &f.owner)
        .is_err());

    Ok(())
}

#[test]
fn scoped_answer_is_atomic_source_linked_idempotent_and_fenced_after_context_changes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |_| {})?;
    let answer = f
        .agent
        .agent_artifact_reply(&f.space, &f.source, &f.owner, "Scoped answer")
        .unwrap();
    assert_eq!(answer.reply_to.as_ref().unwrap().id, f.source);
    let before = counts(&f);
    assert_eq!(
        f.agent
            .agent_artifact_reply(&f.space, &f.source, &f.owner, "Different retry")
            .unwrap(),
        answer
    );
    assert_eq!(counts(&f), before);
    f.agent
        .app_binary_save(
            f.base.item_id.clone(),
            f.base.version,
            f.base.sha256.clone(),
            STANDARD.encode(CHANGED),
            "outside_answer".into(),
            "33".repeat(32),
        )
        .unwrap();
    let before = counts(&f);
    assert!(f
        .agent
        .agent_artifact_reply(&f.space, &f.source, &f.owner, "Late answer")
        .is_err());
    assert_eq!(counts(&f), before);

    Ok(())
}

#[test]
fn signed_recipient_refuses_another_agent_before_read_reply_or_tool_proposal(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    scope(&mut f, |r| {
        r.artifact.as_mut().unwrap().agent_id = Some("other_agent".into())
    })?;
    let before = counts(&f);
    assert!(f
        .agent
        .agent_artifact_request(&f.space, &f.source, &f.owner)
        .is_err());
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_err());
    assert!(f
        .agent
        .agent_artifact_reply(&f.space, &f.source, &f.owner, "Not for me")
        .is_err());
    assert_eq!(counts(&f), before);
    let agent = f.agent.account().ok_or("agent account")?.identity_id;
    scope(&mut f, |r| {
        r.artifact.as_mut().unwrap().agent_id = Some(agent)
    })?;
    assert!(f
        .agent
        .agent_artifact_request(&f.space, &f.source, &f.owner)?
        .is_some());
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_ok());
    Ok(())
}
