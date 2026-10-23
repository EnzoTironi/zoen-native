//! One fresh signed fixture; no provider login, services or product home.
use super::*;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::{testing::ControlledToolBrain, BrainOut};

fn context(
    client: &Arc<Client>,
    worker: Option<Arc<zoen_agentd::deckcraft::DeckcraftWorker>>,
    journal: &Path,
    space: &str,
    starts: &Arc<AtomicUsize>,
) -> Ctx {
    let account = client.account().unwrap();
    let space = space.to_string();
    let starts = starts.clone();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().unwrap(),
        deckcraft: worker,
        journal: journal.into(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(page_tool_tests::Allow)),
                ..Gate::default()
            },
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    let (name, arguments, requires_review) = match steps {
                        [] => {
                            starts.fetch_add(1, Ordering::SeqCst);
                            (
                                client_core::agent_app_creation::model_name(&space).unwrap(),
                                json!({"title":"New requested presentation"}),
                                false,
                            )
                        }
                        [create] => {
                            assert!(!create.is_error, "creation failed: {}", create.result);
                            let result: Value = serde_json::from_str(&create.result).unwrap();
                            let item = result["itemId"].as_str().unwrap();
                            (
                                crate::deckcraft::model_name("read_deckcraft", item).unwrap(),
                                json!({"artifactId":item}),
                                false,
                            )
                        }
                        [create, read] => {
                            assert!(
                                !create.is_error && !read.is_error,
                                "new artifact discovery/read failed"
                            );
                            let result: Value = serde_json::from_str(&read.result).unwrap();
                            let item = result["scope"]["artifactId"].as_str().unwrap();
                            (
                                crate::deckcraft::model_name("deckcraft_slide_notes", item)
                                    .unwrap(),
                                json!({"artifactId":item,"version":result["head"]["version"].as_str().unwrap().parse::<u32>().unwrap(),"sha256":result["head"]["sha256"],"slideId":result["slides"][0]["id"],"text":"Created and edited through the same reviewed request."}),
                                true,
                            )
                        }
                        [create, read, write] => {
                            assert!(!create.is_error && !read.is_error && !write.is_error);
                            return Ok(BrainOut::Say(
                                "Created and edited the reviewed presentation.".into(),
                            ));
                        }
                        _ => return Err("Unexpected creation progress".into()),
                    };
                    Ok(BrainOut::CallTools(vec![ToolCall {
                        id: format!("create-flow-{}", steps.len()),
                        name,
                        arguments,
                        requires_review,
                    }]))
                }),
            }),
        ),
    }
}

#[test]
fn binary_ui_transports_are_not_offered_and_cannot_replay_a_cached_model_success() {
    let fixture = crate::deckcraft::tests::fixture();
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let journal = fixture.path.join("creation.json");
    let starts = Arc::new(AtomicUsize::new(0));
    let cx = context(&fixture.client, None, &journal, &space, &starts);
    let host = SpaceTools::new(
        &fixture.client,
        &cx,
        &space,
        &fixture.owner,
        &fixture.source,
    );
    assert!(host
        .tools()
        .iter()
        .all(|tool| !tool.name.starts_with("app_binary_")));
    let call = ToolCall {
        id: "old-transport".into(),
        name: "app_binary_commit".into(),
        arguments: json!({}),
        requires_review: false,
    };
    host.record(ToolStep {
        call: call.clone(),
        result: "old cached success".into(),
        is_error: false,
    });
    let before = std::fs::read(&journal).unwrap();
    assert!(host.call(&call).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    drop(cx);
    drop(fixture.client);
    std::fs::remove_dir_all(fixture.path).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit pinned offline WASI component and configured genuine editor; no providers or services"]
async fn offline_create_approve_lost_result_recover_edit_and_cold_reopen() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let worker = crate::deckcraft::tests::worker();
    let fixture = crate::deckcraft::tests::fixture();
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    assert!(
        client_core::agent_app_creation::definition(&space).is_some(),
        "qualify with genuine editor build profile"
    );
    let path = fixture.path.clone();
    let source = fixture.source.clone();
    let owner = fixture.owner.clone();
    let vault = fixture.vault.clone();
    let journal = path.join("create-edit.json");
    let starts = Arc::new(AtomicUsize::new(0));
    let mut client = fixture.client.clone();
    drop(fixture);
    let before = client.items().len();
    let cx = context(&client, Some(worker.clone()), &journal, &space, &starts);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let request = client
        .tool_approval_requests(&space, &source)
        .last()
        .unwrap()
        .clone();
    assert_eq!(request.tool_proposal.as_ref().unwrap().schema, 3);
    assert_eq!(client.items().len(), before, "staging is inert");
    drop(cx);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &request, &owner);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, Some(worker.clone()), &journal, &space, &starts);
    let host = SpaceTools::new(&client, &cx, &space, &owner, &source);
    assert!(host
        .begin_resume(&request.id, &request.content_hash)
        .unwrap());
    host.entering_canonical_executor().unwrap();
    let permit = client.approved_app_creation(&request.id).unwrap();
    let result: Value =
        serde_json::from_str(&client.execute_app_creation(&permit).unwrap()).unwrap();
    let item = result["itemId"].as_str().unwrap().to_string();
    assert_eq!(client.items().len(), before + 1);
    assert!(
        JournalState::load(&journal).unwrap().steps.is_empty(),
        "simulate effect before journal ACK"
    );
    drop(permit);
    drop(host);
    drop(cx);
    drop(client);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, Some(worker.clone()), &journal, &space, &starts);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "no replacement proposal or repeated creation"
    );
    let edit = client
        .tool_approval_requests(&space, &source)
        .last()
        .unwrap()
        .clone();
    assert_eq!(
        edit.tool_proposal.as_ref().unwrap().canonical_tool,
        "deckcraft_slide_notes"
    );
    assert_eq!(edit.tool_proposal.as_ref().unwrap().step, 2);
    assert_eq!(client.items().len(), before + 1);
    drop(cx);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &edit, &owner);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, Some(worker.clone()), &journal, &space, &starts);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    let accepted = client.app_binary_read(item.clone(), None, None).unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(client.items().len(), before + 1);
    assert_eq!(JournalState::load(&journal).unwrap().steps[0].1.len(), 3);
    drop(cx);
    drop(client);
    let cold = canonical_tool_approval_tests::reopen(&path, vault);
    assert_eq!(
        cold.app_binary_read(item.clone(), None, None).unwrap(),
        accepted
    );
    let read: Value = serde_json::from_str(
        &crate::deckcraft::call(
            &cold,
            &worker,
            &item,
            &source,
            &owner,
            "read_deckcraft",
            &json!({"artifactId":item}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        read["slides"][0]["notes"]["paragraphs"][0]["runs"][0]["text"],
        "Created and edited through the same reviewed request."
    );
    assert!(cold.verify_all().iter().all(|result| result.valid));
    drop(cold);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn canonical_creation_name_keeps_gate_sensitive_to_semantic_credentials() {
    let name =
        client_core::agent_app_creation::model_name(&format!("sp_{}", "ab".repeat(32))).unwrap();
    let check = |title: &str| {
        ::gate::Check::new(
            ::gate::Point::ToolCall,
            format!(
                "mini-app tool {name} Create presentation {}",
                json!({"title":title})
            ),
        )
    };
    assert!(::gate::hard_rules(&check("A new presentation")).is_none());
    assert!(
        matches!(::gate::hard_rules(&check(&format!("sk-test-{}", "a1".repeat(24)))), Some((::gate::Outcome::Block, reason)) if reason == "hard:credential")
    );
}
