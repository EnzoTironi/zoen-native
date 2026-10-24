//! Real signed source + real document CAS; no model, service or worker simulation.
use super::tests::{counts, fixture, turn, Fixture, CHANGED, ORIGINAL};
use super::*;

pub(crate) fn scope(f: &mut Fixture, alter: impl FnOnce(&mut types::ReplyRef)) {
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
        request_id: types::new_id("app_request"),
        text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
    });
    alter(&mut reference);
    let mut e = f.agent.lock();
    e.append(
        &f.space,
        &f.owner,
        types::EventBody::MessagePosted {
            message: types::new_id("m"),
            text: text.into(),
            attaches: None,
            reply: Some(reference),
            media: None,
            forwarded: false,
        },
    )
    .unwrap();
    f.source = e
        .space_state(&f.space)
        .unwrap()
        .entries
        .last()
        .unwrap()
        .id();
}

#[test]
fn signed_scope_fences_other_artifacts_creation_sites_pages_images_and_stale_results() {
    let mut f = fixture();
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
    scope(&mut f, |_| {});
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
}

#[test]
fn own_accepted_chain_allows_multiple_steps_and_lost_ack_but_not_another_source() {
    let mut f = fixture();
    scope(&mut f, |_| {});
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
    scope(&mut f, |_| {});
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, &f.source, &f.owner, None, None)
        .is_err());
}

#[test]
fn malformed_forged_and_changed_signed_scope_fail_closed_without_effects() {
    for alter in [
        |r: &mut types::ReplyRef| r.thread = false,
        |r: &mut types::ReplyRef| r.to = "unrelated".into(),
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().version = 0,
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().sha256 = "00".repeat(32),
        |r: &mut types::ReplyRef| r.artifact.as_mut().unwrap().text_sha256 = "00".repeat(32),
    ] {
        let mut f = fixture();
        scope(&mut f, alter);
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
    let mut f = fixture();
    scope(&mut f, |_| {});
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
}
