//! Real host/Gate/signed FileDoc with process-style close/reopen. No providers,
//! transport services, guest executor or product account are used by this test.
use super::*;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::{testing::ControlledToolBrain, BrainOut};

fn context(client: &Arc<Client>, journal: &Path, starts: &Arc<AtomicUsize>, reversed: bool) -> Ctx {
    let account = client.account().unwrap();
    let starts = starts.clone();
    // The host enables serde_json/preserve_order. Both insertion orders must bind
    // to the same canonical proposal; Core-only BTreeMap tests cannot prove this.
    let mut source = serde_json::Map::new();
    let fields = [
        ("title", json!("Requested Site")),
        ("html", json!("<!doctype html><h1>Created privately</h1>")),
    ];
    for (name, value) in if reversed {
        fields.into_iter().rev().collect::<Vec<_>>()
    } else {
        fields.into_iter().collect()
    } {
        source.insert(name.into(), value);
    }
    let create_arguments = Value::Object(source);
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: client.agent_owner().unwrap(),
        deckcraft: None,
        journal: journal.into(),
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(page_tool_tests::Allow)),
                ..Gate::default()
            },
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    let (name, arguments) = match steps {
                        [] => {
                            starts.fetch_add(1, Ordering::SeqCst);
                            ("create_site".into(), create_arguments.clone())
                        }
                        [create] => {
                            assert!(!create.is_error, "{}", create.result);
                            let r: Value = serde_json::from_str(&create.result).unwrap();
                            let item = r["itemId"].as_str().unwrap();
                            (
                                client_core::agent_sites::model_name("read_site", item).unwrap(),
                                json!({"artifactId":item,"version":r["version"],"sha256":r["sha256"]}),
                            )
                        }
                        [create, read] => {
                            assert!(!create.is_error && !read.is_error);
                            let r: Value = serde_json::from_str(&read.result).unwrap();
                            let item = r["artifactId"].as_str().unwrap();
                            (
                                client_core::agent_sites::model_name("replace_site_source", item)
                                    .unwrap(),
                                json!({"artifactId":item,"version":r["version"],"sha256":r["sha256"],"html":"<!doctype html><h1>Reviewed second version</h1>"}),
                            )
                        }
                        [create, read, edit] => {
                            assert!(!create.is_error && !read.is_error && !edit.is_error);
                            return Ok(BrainOut::Say("The private Site is updated.".into()));
                        }
                        _ => return Err("Unexpected Site progress".into()),
                    };
                    Ok(BrainOut::CallTools(vec![ToolCall {
                        id: format!("site-flow-{}", steps.len()),
                        name,
                        arguments,
                        requires_review: false,
                    }]))
                }),
            }),
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn site_create_approve_lost_ack_cold_read_review_edit_reopen_exact_v2() {
    for reversed in [false, true] {
        site_journey(reversed).await;
    }
}
async fn site_journey(reversed: bool) {
    let path = std::env::temp_dir().join(types::new_id("site-agent-journey"));
    std::fs::create_dir_all(&path).unwrap();
    let fixture = crate::deckcraft::tests::fixture_with_privacy_at(
        path.clone(),
        [81; 32],
        types::Privacy::EndToEnd,
    );
    let space = fixture
        .client
        .item(fixture.base.item_id.clone())
        .unwrap()
        .space_id;
    let source = fixture.source.clone();
    let owner = fixture.owner.clone();
    let vault = fixture.vault.clone();
    let journal = path.join("site-host.json");
    let mut client = fixture.client.clone();
    drop(fixture);
    let before = client.items().len();
    let starts = Arc::new(AtomicUsize::new(0));
    let cx = context(&client, &journal, &starts, reversed);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    let create = client
        .tool_approval_requests(&space, &source)
        .last()
        .unwrap()
        .clone();
    assert_eq!(create.tool_proposal.as_ref().unwrap().schema, 5);
    assert_eq!(client.items().len(), before, "staging must not create");
    drop(cx);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &create, &owner);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, &journal, &starts, reversed);
    let host = SpaceTools::new(&client, &cx, &space, &owner, &source);
    assert!(host.begin_resume(&create.id, &create.content_hash).unwrap());
    host.entering_canonical_executor().unwrap();
    let permit = client.approved_site_tool(&create.id).unwrap();
    let result: Value = serde_json::from_str(&client.execute_site_tool(&permit).unwrap()).unwrap();
    let item = result["itemId"].as_str().unwrap().to_owned();
    let v1 = result["sha256"].as_str().unwrap().to_owned();
    assert_eq!(client.items().len(), before + 1);
    assert!(
        JournalState::load(&journal).unwrap().steps.is_empty(),
        "simulate lost post-effect journal ACK"
    );
    drop(permit);
    drop(host);
    drop(cx);
    drop(client);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, &journal, &starts, reversed);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "must resume original request"
    );
    let edit = client
        .tool_approval_requests(&space, &source)
        .last()
        .unwrap()
        .clone();
    assert_eq!(
        edit.tool_proposal.as_ref().unwrap().canonical_tool,
        "replace_site_source"
    );
    assert_eq!(edit.tool_proposal.as_ref().unwrap().step, 2);
    assert_eq!(client.items().len(), before + 1);
    drop(cx);
    drop(client);
    canonical_tool_approval_tests::signed_owner_resolution(&path, &space, &edit, &owner);
    client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, &journal, &starts, reversed);
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    let info = client.file_info_at(item.clone(), Some(2)).unwrap();
    assert_eq!(client.items().len(), before + 1);
    assert_eq!(JournalState::load(&journal).unwrap().steps[0].1.len(), 3);
    drop(cx);
    drop(client);
    let cold = canonical_tool_approval_tests::reopen(&path, vault);
    assert_eq!(
        cold.agent_site_read(&item, 2, &info.sha256, &source, &owner)
            .unwrap()
            .html,
        "<!doctype html><h1>Reviewed second version</h1>"
    );
    assert_eq!(
        cold.agent_site_read(&item, 1, &v1, &source, &owner)
            .unwrap()
            .html,
        "<!doctype html><h1>Created privately</h1>"
    );
    assert!(cold.file_info_at(item, Some(3)).is_err());
    assert!(cold.verify_all().iter().all(|r| r.valid));
    drop(cold);
    std::fs::remove_dir_all(path).unwrap();
}
