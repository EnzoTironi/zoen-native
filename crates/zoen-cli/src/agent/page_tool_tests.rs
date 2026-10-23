//! No relay, worker or inference: actual normal host + signed Core Page approval.
use super::*;
use client_core::{agent_page_tools as page, AgentPageSnapshot};
use gate::{Answer, DecisionProvider, DecisionRequest, DecisionResponse, GateError, Question};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Allow;
impl DecisionProvider for Allow {
    fn name(&self) -> String {
        "page-agent-offline-fixture".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, r: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        Ok(DecisionResponse {
            answers: r
                .questions
                .iter()
                .map(|q| {
                    (
                        q.name().into(),
                        match q {
                            Question::Predicate { name, .. } => Answer::Predicate {
                                p: if name == "matches_user_intent" {
                                    1.0
                                } else {
                                    0.0
                                },
                            },
                            Question::Score { .. } => Answer::Score {
                                score: 0.0,
                                confidence: 1.0,
                            },
                            Question::Choice { .. } => unreachable!(),
                        },
                    )
                })
                .collect(),
        })
    }
}
fn cx(client: &Arc<Client>, journal: &Path, item: &str, starts: Arc<AtomicUsize>) -> Ctx {
    let account = client.account().unwrap();
    let item = item.to_string();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().unwrap(),
        deckcraft: None,
        journal: journal.into(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(Allow)),
                ..Gate::default()
            },
            Box::new(zoen_agentd::citizen::testing::ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    let (tool, args) = match steps {
                        [] => {
                            starts.fetch_add(1, Ordering::SeqCst);
                            ("read_page", json!({"artifactId":item}))
                        }
                        [read] => {
                            assert!(!read.is_error);
                            let snapshot: AgentPageSnapshot =
                                serde_json::from_str(&read.result).unwrap();
                            let before = snapshot
                                .blocks
                                .iter()
                                .find(|b| b.text == "Public text")
                                .unwrap()
                                .clone();
                            let mut after = before.clone();
                            after.text = "Owner reviewed exact replacement 🧪".into();
                            (
                                "replace_page_block",
                                serde_json::to_value(page::ReplaceBlock {
                                    artifact_id: item.clone(),
                                    version: snapshot.version,
                                    prefix_sha256: snapshot.prefix_sha256,
                                    content_sha256: snapshot.content_sha256,
                                    before,
                                    after,
                                })
                                .unwrap(),
                            )
                        }
                        [read, write] => {
                            assert!(!read.is_error, "fixture read refused: {}", read.result);
                            assert!(!write.is_error, "fixture write refused: {}", write.result);
                            let receipt: client_core::AgentPageResult =
                                serde_json::from_str(&write.result).unwrap();
                            assert_eq!(receipt.observed_base_version, 1);
                            return Ok(zoen_agentd::citizen::BrainOut::Say(
                                "Applied the reviewed Page text once.".into(),
                            ));
                        }
                        _ => return Err("Unexpected fixture progress".into()),
                    };
                    Ok(zoen_agentd::citizen::BrainOut::CallTools(vec![ToolCall {
                        id: format!("fixture-{tool}"),
                        name: page::model_name(tool, &item).unwrap(),
                        arguments: args,
                        requires_review: false,
                    }]))
                }),
            }),
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn canonical_page_read_approve_once_write_and_cold_resume_use_the_normal_host_without_worker()
{
    let fixture = crate::deckcraft::tests::fixture();
    let client = &fixture.client;
    let space = client.item(fixture.base.item_id.clone()).unwrap().space_id;
    let item = client
        .page_import_markdown(
            space.clone(),
            "shared.md".into(),
            "# Page\n\nPublic text\n\nKeep this sibling.\n".into(),
        )
        .unwrap()
        .id;
    let journal = fixture.path.join("page-steps.json");
    let starts = Arc::new(AtomicUsize::new(0));
    let context = cx(client, &journal, &item, starts.clone());
    let tools = SpaceTools::new(client, &context, &space, &fixture.owner, &fixture.source);
    let offered = tools.tools();
    assert!(offered
        .iter()
        .any(|d| d.name == page::model_name("read_page", &item).unwrap()));
    assert!(offered
        .iter()
        .any(|d| d.name == page::model_name("replace_page_block", &item).unwrap()));
    let read_call = ToolCall {
        id: "direct".into(),
        name: page::model_name("read_page", &item).unwrap(),
        arguments: json!({"artifactId":item}),
        requires_review: false,
    };
    let read: AgentPageSnapshot = serde_json::from_str(&tools.call(&read_call).unwrap()).unwrap();
    assert_eq!(read.version, 1);
    // Remove only this test's explicit read checkpoint: normal run below starts fresh.
    drop(tools);
    drop(context);
    std::fs::remove_file(&journal).unwrap();
    let context = cx(client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(client, &context, &space, &fixture.source).await;
    let request = client
        .tool_approval_requests(&space, &fixture.source)
        .pop()
        .unwrap();
    assert_eq!(request.tool_proposal.as_ref().unwrap().schema, 2);
    assert_eq!(client.page(item.clone()).unwrap().version, 1);
    assert_eq!(
        request.tool_proposal.as_ref().unwrap().gate_rule,
        "native:review-required"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    canonical_tool_approval_tests::handle(client, &context, &space, &fixture.source).await;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let path = fixture.path.clone();
    let vault = fixture.vault.clone();
    let source = fixture.source.clone();
    let owner = fixture.owner.clone();
    drop(context);
    drop(fixture);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &request, &owner);
    let client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let context = cx(&client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&client, &context, &space, &source).await;
    let updated = client.page(item.clone()).unwrap();
    assert_eq!(updated.version, 2);
    assert!(updated
        .blocks
        .iter()
        .any(|b| b.text == "Owner reviewed exact replacement 🧪"));
    assert!(updated
        .blocks
        .iter()
        .any(|b| b.text == "Keep this sibling."));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let history = client.page_at(item.clone(), 1).unwrap();
    assert!(history.blocks.iter().any(|b| b.text == "Public text"));
    drop(context);
    drop(client);
    let client = canonical_tool_approval_tests::reopen(&path, vault);
    let context = cx(&client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&client, &context, &space, &source).await;
    assert_eq!(client.page(item).unwrap().version, 2);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    drop(context);
    drop(client);
    std::fs::remove_dir_all(path).unwrap();
}
