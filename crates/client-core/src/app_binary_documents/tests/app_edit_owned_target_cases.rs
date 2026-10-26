//! Owned Excalidraw uses the exact UI pin and the real execution permission.
use super::app_edit_request_cases::with_agent;
use super::*;
use crate::app_edit_requests::AppEditRequestInput;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn owned_target_requires_real_permission_and_displayed_pin_before_any_disclosure(
) -> TestResult {
    let (f, agent) = with_agent(":memory:")?;
    let html = "<!doctype html><p>Owned MCP App package</p>";
    let package = f.client.file_add(
        f.space.clone(),
        String::new(),
        "drawing.html".into(),
        crate::app_resources::MIME.into(),
        html.as_bytes().to_vec(),
        None,
    )?;
    let account = f.client.account().ok_or("owner account")?;
    let admission = crate::app_resources::Install {
        expected_identity_id: account.identity_id.clone(),
        expected_device_id: account.device_id.clone(),
        space_id: f.space.clone(),
        package_item_id: package.id,
        package_version: 1,
        package_sha256: hash(html.as_bytes()),
        title: "Owned drawing".into(),
        mutation_id: "owned_target_install".into(),
    };
    let item = f.client.lock().install_app_resource(admission.clone())?;
    let pin = item.app.ok_or("owned App")?.resource_uri;
    let doc = f.client.app_document_read(item.id.clone())?;
    let mut request = AppEditRequestInput {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: f.space.clone(),
        item_id: item.id.clone(),
        expected_version: doc.version,
        expected_sha256: doc.sha256.clone(),
        expected_resource_uri: Some(pin.clone()),
        target_agent_id: Some(agent.clone()),
        expected_target_revision: None,
        text: "Edit this drawing only".into(),
        mutation_id: "owned_target_request".into(),
        share_current: true,
    };
    super::app_edit_target_cases::confirm(&f)?;
    let context = f.client.lock().app_edit_targets(&(&request).into())?;
    assert_eq!(context.agents.len(), 1);
    assert!(context.agents[0].delivery_ready);
    assert!(context.agents[0].permission_required);
    assert!(
        !context.agents[0].eligible,
        "implicit owned Act is not schema6 readiness"
    );
    request.expected_target_revision = Some(context.revision.clone());
    let before = counts(&f);
    assert!(f.client.lock().request_app_edit(request.clone()).is_err());
    assert_eq!(counts(&f), before);

    f.client
        .lock()
        .set_trust(&agent, &f.space, types::TrustLevel::Suggest)?;
    let next = f.client.lock().app_edit_targets(&(&request).into())?;
    assert_ne!(context.revision, next.revision);
    assert!(next.agents[0].eligible && !next.agents[0].permission_required);
    // The old selection never inherits the new consent silently.
    let before = counts(&f);
    assert!(f.client.lock().request_app_edit(request.clone()).is_err());
    assert_eq!(counts(&f), before);
    request.expected_target_revision = Some(next.revision);
    for wrong_pin in [None, Some("ui://forged".into())] {
        let mut wrong = request.clone();
        wrong.expected_resource_uri = wrong_pin;
        assert!(f.client.lock().request_app_edit(wrong).is_err());
        assert_eq!(counts(&f), before);
    }
    let read: serde_json::Value = serde_json::from_str(&f.client.client_request(
        json!({"op":"app_edit_targets","input":crate::app_edit_requests::AppEditTargetInput::from(&request)}).to_string()).await?)?;
    assert_eq!(read["agents"][0]["agent"]["id"], agent);
    assert_eq!(read["agents"][0]["eligible"], true);
    assert_eq!(
        read["revision"],
        request.expected_target_revision.clone().ok_or("revision")?
    );

    // An independent package update keeps the scene bytes but retires this pin.
    let new_html = "<!doctype html><p>Owned MCP App release two</p>";
    f.client.file_new_version(
        admission.package_item_id.clone(),
        new_html.as_bytes().to_vec(),
        None,
        "New UI".into(),
    )?;
    let updated = f
        .client
        .lock()
        .update_app_resource(crate::app_resources::Update {
            admission: crate::app_resources::Install {
                package_version: 2,
                package_sha256: hash(new_html.as_bytes()),
                mutation_id: "new_owned_ui".into(),
                ..admission
            },
            item_id: item.id,
            expected_version: doc.version,
            expected_sha256: doc.sha256,
            expected_resource_uri: pin,
        })?;
    assert_eq!(updated.sha256, request.expected_sha256);
    let before = counts(&f);
    assert!(f.client.lock().request_app_edit(request.clone()).is_err());
    assert_eq!(counts(&f), before);
    super::app_edit_target_cases::confirm(&f)?;
    let fresh_pin = f
        .client
        .item(request.item_id.clone())?
        .app
        .ok_or("updated App")?
        .resource_uri;
    request.expected_version = updated.version;
    request.expected_resource_uri = Some(fresh_pin);
    request.mutation_id = "fresh_owned_target".into();
    let fresh = f.client.lock().app_edit_targets(&(&request).into())?;
    request.expected_target_revision = Some(fresh.revision);
    let receipt = f.client.lock().request_app_edit(request.clone())?;
    let after = counts(&f);
    assert_eq!(f.client.lock().request_app_edit(request)?.id, receipt.id);
    assert_eq!(counts(&f), after);
    assert_eq!(
        f.client
            .lock()
            .artifact_request(&f.space, &receipt.id)?
            .and_then(|r| r.agent_id),
        Some(agent)
    );
    Ok(())
}
