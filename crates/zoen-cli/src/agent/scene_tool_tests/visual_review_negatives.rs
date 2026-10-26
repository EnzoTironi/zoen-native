//! Preview reauthorization uses the same signed pending request and real owner APIs.
use super::*;
use fixture::{append, owner_change, rewrite, signer, Fixture};
use sha2::{Digest, Sha256};
use types::{Capability, EventBody, GrantScope};
type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn preview(f: &mut Fixture, r: &types::AgentRequest, allowed: bool) -> R<()> {
    let r = r.clone();
    let item = f.item.clone();
    let pin = f.resource_uri.clone();
    let space = f.space.clone();
    owner_change(f, |owner| async move {
        let account = owner.account().unwrap();
        let before = owner.app_document_read(item.clone())?;
        let result = owner
            .client_request(
                json!({"op":"scene_tool_review","input":{
            "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
            "item_id":item,"space_id":space,"expected_resource_uri":pin,
            "request_id":r.id,"expected_hash":r.content_hash}})
                .to_string(),
            )
            .await;
        assert_eq!(result.is_ok(), allowed, "preview returned {result:?}");
        assert_eq!(
            owner.app_document_read(item)?,
            before,
            "preview never changes the document"
        );
        Ok(())
    })
    .await
}
fn dispose(f: Fixture) -> R<()> {
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_rejects_a_changed_canonical_document_base() -> R<()> {
    let (mut f, r) = negatives::held().await?;
    preview(&mut f, &r, true).await?;
    let item = f.item.clone();
    let pin = f.resource_uri.clone();
    owner_change(&mut f, |owner| async move {
        let base = owner.app_document_read(item.clone())?;
        owner.client_request(json!({"op":"app_document_save_pinned","item_id":item,
            "expected_resource_uri":pin,"expected_version":base.version,"expected_sha256":base.sha256,
            "scene_json":edited_scene(),"mutation_id":"preview_owner_base"}).to_string()).await?;
        Ok(())
    }).await?;
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 2);
    preview(&mut f, &r, false).await?;
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_rejects_a_changed_admitted_ui_pin() -> R<()> {
    let (mut f, r) = negatives::held().await?;
    preview(&mut f, &r, true).await?;
    let item = f.item.clone();
    let pin = f.resource_uri.clone();
    let package = pin
        .strip_prefix("ui://zoen/package/")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    let space = f.space.clone();
    owner_change(&mut f, |owner| async move {
        let html = b"<!doctype html><p>Another immutable release</p>";
        let file = owner.file_new_version(package.clone(), html.to_vec(), None, "UI release".into())?;
        let base = owner.app_document_read(item.clone())?;
        let account = owner.account().unwrap();
        let title = owner.item(item.clone())?.title;
        owner.client_request(json!({"op":"app_resource_update","input":{
            "admission":{"expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
                "space_id":space,"package_item_id":package,"package_version":file.version,
                "package_sha256":hex::encode(Sha256::digest(html)),"title":title,"mutation_id":"preview_owner_pin"},
            "item_id":item,"expected_version":base.version,"expected_sha256":base.sha256,
            "expected_resource_uri":pin}}).to_string()).await?;
        Ok(())
    }).await?;
    assert_ne!(
        f.client.item(f.item.clone())?.app.unwrap().resource_uri,
        f.resource_uri
    );
    preview(&mut f, &r, false).await?;
    dispose(f)
}

async fn revoked(app: bool) -> R<()> {
    let (mut f, r) = negatives::held().await?;
    preview(&mut f, &r, true).await?;
    let owner = signer(&f.owner_vault);
    let item = f.item.clone();
    let space = f.space.clone();
    let agent = r.agent.clone();
    rewrite(&mut f, |store, log| {
        let grant = log
            .events()
            .iter()
            .rev()
            .find_map(|e| match &e.body {
                EventBody::GrantIssued { grant }
                    if grant.grantor == owner.identity
                        && matches!(grant.capability, Capability::Trust(_))
                        && if app {
                            grant.scope == GrantScope::Item(item.clone())
                                && grant.grantee.as_deref() == Some(format!("app:{item}").as_str())
                        } else {
                            grant.scope == GrantScope::Space(space.clone())
                                && grant.grantee.as_deref() == Some(agent.as_str())
                        } =>
                {
                    Some(grant.id.clone())
                }
                _ => None,
            })
            .unwrap();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;
        append(
            store,
            log,
            &space,
            &owner,
            at,
            EventBody::GrantRevoked { grant },
        )
    })?;
    preview(&mut f, &r, false).await?;
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    dispose(f)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_reauthorizes_the_live_app_grant() -> R<()> {
    revoked(true).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_reauthorizes_the_live_agent_grant() -> R<()> {
    revoked(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_visual_review_refuses_an_authentic_pending_proposal_after_expiry() -> R<()> {
    let (mut f, mut r) = negatives::held().await?;
    preview(&mut f, &r, true).await?;
    r.id = types::new_id("expired_scene_preview")?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    r.tool_proposal.as_mut().unwrap().expires_at_ms = at - 2000;
    r.content_hash = r.tool_proposal_hash(&f.space).unwrap();
    let agent = signer(&f.vault);
    let space = f.space.clone();
    rewrite(&mut f, |store, log| {
        append(
            store,
            log,
            &space,
            &agent,
            at - 3000,
            EventBody::RequestOpened { request: r.clone() },
        )
    })?;
    assert!(
        f.client.requests().iter().any(|v| v.id == r.id),
        "real signed expired request still projects"
    );
    preview(&mut f, &r, false).await?;
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    dispose(f)
}
