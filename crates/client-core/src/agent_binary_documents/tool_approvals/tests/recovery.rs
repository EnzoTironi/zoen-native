//! Whole recovery cases using the original shared fixture and canonical store.

use super::*;

pub(super) fn accepted_receipt_replays_once_and_wrong_mutation_or_intent_never_write(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest)?;
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let before = counts(&fixture);
    let wrong_hash = "00".repeat(32);
    for (id, hash) in [
        ("unreviewed-mutation", intent.as_str()),
        (mutation.as_str(), wrong_hash.as_str()),
    ] {
        assert!(fixture
            .agent
            .agent_binary_result(approved.turn(), id, hash)
            .is_err());
        assert!(fixture
            .agent
            .agent_binary_save(approved.turn(), id, hash, &STANDARD.encode(CHANGED))
            .is_err());
    }
    assert_eq!(counts(&fixture), before);
    assert!(fixture
        .agent
        .agent_binary_result(approved.turn(), &mutation, &intent)
        .unwrap()
        .is_none());
    let accepted = fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.data_base64, STANDARD.encode(CHANGED));
    let before = counts(&fixture);
    assert_eq!(
        fixture
            .agent
            .agent_binary_result(approved.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted.clone())
    );
    assert_eq!(
        fixture
            .agent
            .agent_binary_save(
                approved.turn(),
                &mutation,
                &intent,
                &STANDARD.encode(CHANGED)
            )
            .unwrap(),
        accepted
    );
    let reopened_admission = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    assert_eq!(
        fixture
            .agent
            .agent_binary_result(reopened_admission.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted)
    );
    assert_eq!(counts(&fixture), before);
    assert_eq!(current(&fixture).version, 2);

    Ok(())
}

pub(super) fn canonical_approval_and_accepted_document_survive_a_cold_unlocked_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture();
    signed_trust(&fixture, TrustLevel::Suggest)?;
    let request = stage(&fixture, "deckcraft_slide_notes");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let accepted = fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    let directory = std::env::temp_dir().join(types::new_id("zoen_tool_approval_cold")?);
    std::fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let path = directory.join("state.sqlite");
    fixture
        .agent
        .lock()
        .store
        .conn()
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    let Fixture {
        agent,
        agent_vault,
        space,
        source,
        base,
        ..
    } = fixture;
    drop(agent);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(agent_vault).unwrap());
    assert_eq!(client.account().unwrap().identity_id, accepted.actor_id);
    assert_eq!(
        client.lock().identities[&accepted.actor_id].kind,
        IdentityKind::Agent
    );
    let requests = client.tool_approval_requests(&space, &source);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].id, request.id);
    let admission = client.approved_tool_turn(&request.id, WORKER).unwrap();
    assert_eq!(admission.turn().document().version, 1);
    let before = client
        .lock()
        .store
        .conn()
        .query_row("SELECT count(*) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(
        client
            .agent_binary_result(admission.turn(), &mutation, &intent)
            .unwrap(),
        Some(accepted.clone())
    );
    assert_eq!(
        client
            .agent_binary_save(
                admission.turn(),
                &mutation,
                &intent,
                &STANDARD.encode(CHANGED)
            )
            .unwrap(),
        accepted
    );
    assert_eq!(
        client
            .app_binary_read(base.item_id, None, None)
            .unwrap()
            .version,
        2
    );
    assert_eq!(
        client
            .lock()
            .store
            .conn()
            .query_row("SELECT count(*) FROM events", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        before
    );

    Ok(())
}

pub(super) fn read_only_approval_cannot_authorize_a_result_probe_or_save_even_with_its_exact_intent(
) {
    let fixture = fixture();
    let request = stage(&fixture, "read_deckcraft");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let approved = fixture
        .agent
        .approved_tool_turn(&request.id, WORKER)
        .unwrap();
    let (mutation, intent) = intent(approved.proposal());
    let before = counts(&fixture);
    fixture
        .agent
        .agent_binary_finish_read(approved.turn())
        .unwrap();
    assert!(fixture
        .agent
        .agent_binary_result(approved.turn(), &mutation, &intent)
        .is_err());
    assert!(fixture
        .agent
        .agent_binary_save(
            approved.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&fixture), before);
    assert_v1(&fixture);
}

pub(super) fn completed_read_then_accepted_write_recovers_context_without_relaxing_read_cas_or_authorizing_new_effects(
) {
    let fixture = fixture();
    let request = stage(&fixture, "read_deckcraft");
    owner_decide(
        &fixture,
        &request.id,
        observed(&request),
        ToolRequestDecision::ApproveOnce,
    )
    .unwrap();
    let original = fixture
        .agent
        .tool_approval_requests(&fixture.space, &fixture.source)
        .pop()
        .unwrap();
    let proposal = original.tool_proposal.as_ref().unwrap();
    let original_ask: Value = serde_json::from_str(&proposal.continuation_json).unwrap();
    let cached=json!({"scope":{"artifactId":fixture.base.item_id,"actorId":original.agent,"actorKind":"agent"},"head":{"version":"1","sha256":fixture.base.sha256},"slides":[]}).to_string();
    let args = json!({"artifactId":fixture.base.item_id,"version":1,"sha256":fixture.base.sha256,"slideId":1,"text":"Synthetic reviewed notes"});
    let action = agent_tools::command("deckcraft_slide_notes", &args)
        .unwrap()
        .2
        .unwrap();
    let mut command = (**proposal).clone();
    command.canonical_tool = "deckcraft_slide_notes".into();
    let intent = command_intent(&command, Some(&action)).unwrap();
    let raw = turn(&fixture);
    let accepted = fixture
        .agent
        .agent_binary_save(
            &raw,
            &format!("deckcmd_{intent}"),
            &intent,
            &STANDARD.encode(CHANGED),
        )
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert!(
        fixture
            .agent
            .approved_tool_turn(&request.id, WORKER)
            .is_err(),
        "a completed read must not become a fresh historical-head execution permit"
    );
    let steps = json!([
        {"call":original_ask["call"],"result":cached,"is_error":false},
        {"call":{"id":"prepared_synthetic_write","name":agent_tools::model_name("deckcraft_slide_notes",&fixture.base.item_id).unwrap(),"arguments":args},"result":"","is_error":false}
    ]);
    let before = counts(&fixture);
    let recovered: Value = serde_json::from_str(
        &fixture
            .agent
            .recover_tool_results(&request.id, WORKER, &steps.to_string())
            .unwrap(),
    )
    .unwrap();
    let result: Value = serde_json::from_str(recovered[1]["result"].as_str().unwrap()).unwrap();
    assert_eq!(result["version"], 2);
    assert_eq!(result["replayed"], true);
    assert_eq!(
        counts(&fixture),
        before,
        "completion recovery never appends events, files or grants"
    );
    let mut unaccepted = steps.clone();
    unaccepted[1]["call"]["arguments"]["text"] = json!("Different unaccepted command");
    assert!(fixture
        .agent
        .recover_tool_results(&request.id, WORKER, &unaccepted.to_string())
        .is_err());
    let mut other_source = fixture.agent.lock();
    other_source
        .append(
            &fixture.space,
            &fixture.owner,
            EventBody::MessageAnnotated {
                target: fixture.source.clone(),
                note: Annotation::Edit {
                    text: "Changed source".into(),
                },
            },
        )
        .unwrap();
    drop(other_source);
    assert!(fixture
        .agent
        .recover_tool_results(&request.id, WORKER, &steps.to_string())
        .is_err());
}
