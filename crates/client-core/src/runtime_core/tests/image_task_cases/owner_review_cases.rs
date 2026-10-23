use super::*;
use crate::{image_tasks::ImageTaskScope, Client};
use sha2::Digest;

fn client(engine: Engine) -> std::sync::Arc<Client> {
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    *client.inner.lock().unwrap() = engine;
    client
}

#[test]
fn prospective_image_review_projects_exact_signed_facts_without_an_item() {
    let (f, request, hash) = approved();
    let row = &f.core.engine.state.requests[&request];
    assert!(!f
        .core
        .engine
        .state
        .items
        .contains_key(&row.req.tool_proposal.as_ref().unwrap().artifact));
    let dto = f.core.engine.request_dto(row);
    let tool = dto.tool_approval.unwrap();
    assert!(!tool.artifact_title.is_empty());
    assert_eq!(tool.content_hash, hash);
    assert_eq!(tool.canonical_tool.as_deref(), Some("generate_image"));
    let image = tool.image.unwrap();
    let original = input();
    assert_eq!(image.prompt, original.prompt);
    assert_eq!(image.size, original.definition.size);
    assert_eq!(image.quality, original.definition.quality);
    assert_eq!(image.background, original.definition.background);
    assert_eq!(image.price_digest, original.definition.price_digest);
    assert_eq!(image.definition_version, original.definition.version);
    assert_eq!(image.max_image_bytes, original.definition.max_image_bytes);
    let proposal = row.req.tool_proposal.as_ref().unwrap();
    assert_eq!(image.source_id, proposal.source);
    assert_eq!(image.source_sha256, proposal.source_sha256);
    assert!(dto.cost_cents.is_none(), "a digest is not a monetary quote");
}

#[test]
fn invalid_or_rebound_image_payload_has_no_renderable_review() {
    let (f, request, _) = approved();
    let original = f.core.engine.state.requests[&request]
        .req
        .tool_proposal
        .as_ref()
        .unwrap();
    for edit in ["hash", "definition", "schema", "tool"] {
        let mut changed = original.clone();
        match edit {
            "hash" => changed.sha256 = "0".repeat(64),
            "definition" => {
                let mut args: serde_json::Value =
                    serde_json::from_str(&changed.arguments_json).unwrap();
                args["definition"]["size"] = "1536x1024".into();
                changed.arguments_json =
                    miniapps::deckcraft::agent_tools::canonical_json(&args).unwrap();
                changed.sha256 =
                    hex::encode(sha2::Sha256::digest(changed.arguments_json.as_bytes()));
            }
            "schema" => changed.schema = 99,
            "tool" => changed.canonical_tool = "other".into(),
            _ => panic!("finite fixture"),
        }
        assert!(
            f.core.engine.image_generation_review(&changed).is_none(),
            "{edit}"
        );
    }
}

#[tokio::test]
async fn application_command_stages_one_scoped_review_and_refuses_changed_actor_source_or_owner() {
    let (mut f, _, _) = approved();
    f.message("Create another image");
    let source = f.core.engine.logs[SPACE]
        .events()
        .last()
        .unwrap()
        .client_id
        .clone();
    let scope = ImageTaskScope {
        expected_identity_id: f.agent_profile.id.clone(),
        expected_device_id: f.agent_device.id(),
        space_id: SPACE.into(),
    };
    let original = serde_json::json!({
        "op":"stage_image_generation", "scope":scope,
        "source_id":source, "serving_id":f.owner_profile.id, "input":input()
    });
    let core = client(f.core.engine);
    let before = core.lock().state.requests.len();
    for edit in ["identity", "device", "space", "source", "owner", "unknown"] {
        let mut command = original.clone();
        match edit {
            "identity" => command["scope"]["expected_identity_id"] = "foreign".into(),
            "device" => command["scope"]["expected_device_id"] = "foreign".into(),
            "space" => command["scope"]["space_id"] = "foreign".into(),
            "source" => command["source_id"] = "foreign".into(),
            "owner" => command["serving_id"] = "foreign".into(),
            "unknown" => command["input"]["provider_token"] = "not-authority".into(),
            _ => panic!("finite fixture"),
        }
        assert!(
            core.client_request(command.to_string()).await.is_err(),
            "{edit}"
        );
        assert_eq!(core.lock().state.requests.len(), before);
    }
    let first = core.client_request(original.to_string()).await.unwrap();
    let again = core.client_request(original.to_string()).await.unwrap();
    assert_eq!(first, again, "replay reopens the same signed review");
    let dto: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(dto["tool_approval"]["canonical_tool"], "generate_image");
    assert_eq!(dto["tool_approval"]["image"]["max_image_bytes"], "12582912");
    assert_eq!(core.lock().state.requests.len(), before + 1);
    assert_eq!(dto["status"], "Pending");
}
