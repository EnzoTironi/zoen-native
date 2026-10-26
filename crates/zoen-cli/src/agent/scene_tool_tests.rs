//! Actual normal host, native owned document and signed owner review. No worker,
//! provider, device, transport service or guest UI is started by these tests.
use super::*;
use client_core::agent_scenes::{tools as scene, SceneSnapshot};
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::{testing::ControlledToolBrain, BrainOut};

mod fixture;
mod negatives;
use fixture::{approve, fixture};

fn edited_scene() -> String {
    json!({"type":"excalidraw","version":2,"appState":{},"files":{},"elements":[{
        "id":"reviewed_text","type":"text","x":10,"y":20,"width":240,"height":32,
        "text":"Owner approved this scene 🧪","isDeleted":false
    }],"unknown":{"preserve":"exact"}})
    .to_string()
}

fn context(client: &Arc<Client>, journal: &Path, item: &str, starts: Arc<AtomicUsize>) -> Ctx {
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
                on_device: Some(Box::new(page_tool_tests::Allow)),
                ..Gate::default()
            },
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    let (tool, arguments) = match steps {
                        [] => {
                            starts.fetch_add(1, Ordering::SeqCst);
                            ("read_scene", json!({"artifactId":item}))
                        }
                        [read] => {
                            assert!(!read.is_error, "{}", read.result);
                            let snapshot: SceneSnapshot =
                                serde_json::from_str(&read.result).unwrap();
                            (
                                "replace_scene",
                                json!({"artifactId":item,"version":snapshot.version,
                                "sha256":snapshot.sha256,"resourceUri":snapshot.resource_uri,
                                "sceneJson":edited_scene()}),
                            )
                        }
                        [read, write] => {
                            assert!(!read.is_error && !write.is_error, "{}", write.result);
                            let accepted: client_core::AppDocumentDto =
                                serde_json::from_str(&write.result).unwrap();
                            assert_eq!(accepted.version, 2);
                            assert_eq!(accepted.scene_json, edited_scene());
                            return Ok(BrainOut::Say("Applied the reviewed scene once.".into()));
                        }
                        _ => return Err("Unexpected scene fixture progress".into()),
                    };
                    Ok(BrainOut::CallTools(vec![ToolCall {
                        id: format!("scene-{tool}"),
                        name: scene::model_name(tool, &item).unwrap(),
                        arguments,
                        requires_review: false,
                    }]))
                }),
            }),
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn owned_scene_host_reviews_once_and_cold_resume_recovers_the_same_pinned_artifact(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture().await?;
    let journal = f.path.join("scene-steps.json");
    let starts = Arc::new(AtomicUsize::new(0));
    let cx = context(&f.client, &journal, &f.item, starts.clone());
    {
        let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
        let offered = tools.tools();
        assert_eq!(
            offered.len(),
            2,
            "the signed source offers only its owned App"
        );
        for tool in ["read_scene", "replace_scene"] {
            assert!(offered
                .iter()
                .any(|d| Some(d.name.as_str()) == scene::model_name(tool, &f.item).as_deref()));
        }
    }
    canonical_tool_approval_tests::handle(&f.client, &cx, &f.space, &f.source).await;
    let request = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    let proposal = request.tool_proposal.as_ref().unwrap();
    assert_eq!(proposal.schema, 6);
    assert_eq!(proposal.gate_rule, "native:review-required");
    assert_eq!(proposal.canonical_tool, "replace_scene");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.client.app_document_read(f.item.clone()).unwrap().version,
        1
    );
    {
        let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
        let arguments = serde_json::from_str(&proposal.arguments_json).unwrap();
        let bypass = ToolCall {
            id: "direct-write".into(),
            name: scene::model_name("replace_scene", &f.item).unwrap(),
            arguments,
            requires_review: false,
        };
        assert!(tools.requires_native_review(&bypass));
        assert!(
            tools.call(&bypass).is_err(),
            "direct host calls never borrow the pending approval"
        );
    }
    canonical_tool_approval_tests::handle(&f.client, &cx, &f.space, &f.source).await;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    let (path, vault, owner_vault, item, space, source, uri) = (
        f.path.clone(),
        f.vault.clone(),
        f.owner_vault.clone(),
        f.item.clone(),
        f.space.clone(),
        f.source.clone(),
        f.resource_uri.clone(),
    );
    drop(cx);
    drop(f);
    approve(&path, &space, &request, &owner_vault)?;
    let client = canonical_tool_approval_tests::reopen(&path, vault.clone());
    let cx = context(&client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    let accepted = client.app_document_read(item.clone()).unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.scene_json, edited_scene());
    assert_eq!(
        client.item(item.clone()).unwrap().app.unwrap().resource_uri,
        uri
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    drop(cx);
    drop(client);
    let client = canonical_tool_approval_tests::reopen(&path, vault);
    let cx = context(&client, &journal, &item, starts.clone());
    canonical_tool_approval_tests::handle(&client, &cx, &space, &source).await;
    assert_eq!(client.app_document_read(item).unwrap().version, 2);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    drop(cx);
    drop(client);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scoped_scene_source_never_falls_back_to_a_cached_sibling_call(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let f = fixture().await?;
    let journal = f.path.join("scene-scope-steps.json");
    let cx = context(&f.client, &journal, &f.item, Arc::new(AtomicUsize::new(0)));
    let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
    let unoffered = ToolCall {
        id: "cached-sibling".into(),
        name: scene::model_name("read_scene", "it_outside_scope").unwrap(),
        arguments: json!({"artifactId":"it_outside_scope"}),
        requires_review: false,
    };
    tools.record(ToolStep {
        call: unoffered.clone(),
        result: "cached sibling data".into(),
        is_error: false,
    });
    let before = std::fs::read(&journal).unwrap();
    assert!(tools.call(&unoffered).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    let path = f.path.clone();
    drop(tools);
    drop(cx);
    drop(f);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn absent_scene_binding_never_replays_a_cached_success() {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    let path = std::env::temp_dir().join(types::new_id("unoffered-scene").unwrap());
    std::fs::create_dir_all(&path).unwrap();
    let journal = path.join("steps.json");
    let tools = SpaceTools {
        e: &client,
        journal: &journal,
        source: "old-source",
        space: "old-space".into(),
        serving: "old-owner",
        names: vec![],
        deckcraft: None,
        deckcraft_items: vec![],
        page_items: vec![],
        scene_items: vec![],
        selection: None,
    };
    let call = ToolCall {
        id: "cached".into(),
        name: scene::model_name("read_scene", "it_old_scene").unwrap(),
        arguments: json!({"artifactId":"it_old_scene"}),
        requires_review: false,
    };
    tools.record(ToolStep {
        call: call.clone(),
        result: "stale private scene".into(),
        is_error: false,
    });
    let before = std::fs::read(&journal).unwrap();
    assert!(tools.call(&call).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    std::fs::remove_dir_all(path).unwrap();
}
