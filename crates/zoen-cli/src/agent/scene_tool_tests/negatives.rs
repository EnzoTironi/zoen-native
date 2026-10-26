//! Same signed fixture and public Core boundaries, including stale held permits.
use super::*;
use fixture::{append, owner_change, rewrite, signer, Fixture};
use sha2::{Digest, Sha256};
use types::{Capability, EventBody, GrantScope};

type R<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn held() -> R<(Fixture, types::AgentRequest)> {
    let f = fixture().await?;
    let journal = f.path.join("negative-scene-steps.json");
    let cx = context(&f.client, &journal, &f.item, Arc::new(AtomicUsize::new(0)));
    canonical_tool_approval_tests::handle(&f.client, &cx, &f.space, &f.source).await;
    let request = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .pop()
        .unwrap();
    assert_eq!(request.tool_proposal.as_ref().unwrap().schema, 6);
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    drop(cx);
    Ok((f, request))
}

fn command(request: &types::AgentRequest) -> scene::ReplaceScene {
    serde_json::from_str(&request.tool_proposal.as_ref().unwrap().arguments_json).unwrap()
}

async fn approved() -> R<(
    Fixture,
    types::AgentRequest,
    client_core::agent_scenes::ApprovedSceneTurn,
)> {
    let (mut f, request) = held().await?;
    let decision = request.clone();
    owner_change(&mut f, |owner| async move {
        owner.decide_tool_request(
            decision.id,
            decision.content_hash,
            client_core::ToolRequestDecision::ApproveOnce,
        )?;
        Ok(())
    })
    .await?;
    let permit = f.client.approved_scene_tool_turn(&request.id)?;
    assert_eq!(permit.turn().snapshot().version, 1);
    Ok((f, request, permit))
}

fn unchanged_after_refused_write(
    f: &Fixture,
    request: &types::AgentRequest,
    permit: &client_core::agent_scenes::ApprovedSceneTurn,
) {
    let before = f.client.app_document_read(f.item.clone()).unwrap();
    assert!(f.client.approved_scene_tool_turn(&request.id).is_err());
    assert!(f
        .client
        .agent_scene_replace(permit.turn(), &command(request))
        .is_err());
    assert_eq!(f.client.app_document_read(f.item.clone()).unwrap(), before);
}

fn dispose(f: Fixture) -> R<()> {
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_held_approval_cannot_overwrite_a_new_owner_document_base() -> R<()> {
    let (mut f, request, permit) = approved().await?;
    let item = f.item.clone();
    let uri = f.resource_uri.clone();
    let text = edited_scene().replace("Owner approved this scene", "Owner changed this base");
    owner_change(&mut f, |owner| async move {
        let base = owner.app_document_read(item.clone())?;
        owner
            .client_request(
                json!({"op":"app_document_save_pinned","item_id":item,
            "expected_resource_uri":uri,"expected_version":base.version,
            "expected_sha256":base.sha256,"scene_json":text,"mutation_id":"human_base_change"})
                .to_string(),
            )
            .await?;
        Ok(())
    })
    .await?;
    let before = f.client.app_document_read(f.item.clone())?;
    assert_eq!(before.version, 2);
    assert!(before.scene_json.contains("Owner changed this base"));
    unchanged_after_refused_write(&f, &request, &permit);
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_held_approval_cannot_cross_an_owner_ui_release_update() -> R<()> {
    let (mut f, request, permit) = approved().await?;
    let item = f.item.clone();
    let old_uri = f.resource_uri.clone();
    let package = old_uri
        .strip_prefix("ui://zoen/package/")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let uri = old_uri.clone();
    let space = f.space.clone();
    owner_change(&mut f, |owner| async move {
        let html = b"<!doctype html><p>A different admitted release</p>";
        let file = owner.file_new_version(package.clone(), html.to_vec(), None, "UI release".into())?;
        assert_eq!(file.version, 2);
        let base = owner.app_document_read(item.clone())?;
        let account = owner.account().unwrap();
        let title = owner.item(item.clone())?.title;
        owner.client_request(json!({"op":"app_resource_update","input":{
            "admission":{"expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
                "space_id":space,"package_item_id":package,"package_version":2,
                "package_sha256":hex::encode(Sha256::digest(html)),"title":title,"mutation_id":"human_ui_change"},
            "item_id":item,"expected_version":base.version,"expected_sha256":base.sha256,
            "expected_resource_uri":uri}}).to_string()).await?;
        Ok(())
    }).await?;
    assert_ne!(
        f.client.item(f.item.clone())?.app.unwrap().resource_uri,
        old_uri
    );
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 2);
    unchanged_after_refused_write(&f, &request, &permit);
    dispose(f)
}

async fn revoked(app: bool) -> R<()> {
    let (mut f, request, permit) = approved().await?;
    let owner = signer(&f.owner_vault);
    let space = f.space.clone();
    let item = f.item.clone();
    let agent = request.agent.clone();
    rewrite(&mut f, |store, log| {
        let grant = log
            .events()
            .iter()
            .rev()
            .find_map(|event| match &event.body {
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
    unchanged_after_refused_write(&f, &request, &permit);
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_held_approval_loses_the_revoked_app_act_grant() -> R<()> {
    revoked(true).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_held_approval_loses_the_revoked_agent_suggest_grant() -> R<()> {
    revoked(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_owner_identity_and_observed_proposal_hash_are_exact() -> R<()> {
    let (mut f, request) = held().await?;
    assert!(
        f.client
            .decide_tool_request(
                request.id.clone(),
                request.content_hash.clone(),
                client_core::ToolRequestDecision::ApproveOnce
            )
            .is_err(),
        "Agent is not its Owner"
    );
    let req = request.clone();
    owner_change(&mut f, |owner| async move {
        assert!(owner
            .decide_tool_request(
                req.id.clone(),
                "0".repeat(64),
                client_core::ToolRequestDecision::ApproveOnce
            )
            .is_err());
        owner.decide_tool_request(
            req.id,
            req.content_hash,
            client_core::ToolRequestDecision::ApproveOnce,
        )?;
        Ok(())
    })
    .await?;
    let permit = f.client.approved_scene_tool_turn(&request.id)?;
    let accepted = f
        .client
        .agent_scene_replace(permit.turn(), &command(&request))?;
    assert_eq!(accepted.version, 2);
    assert_eq!(
        f.client
            .agent_scene_replace(permit.turn(), &command(&request))?,
        accepted
    );
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_expired_signed_proposal_cannot_gain_owner_approval_without_sleeping() -> R<()> {
    let (mut f, original) = held().await?;
    let mut expired = original.clone();
    expired.id = types::new_id("expired_scene")?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = at - 1000;
    expired.content_hash = expired.tool_proposal_hash(&f.space).unwrap();
    let agent = signer(&f.vault);
    let space = f.space.clone();
    rewrite(&mut f, |store, log| {
        append(
            store,
            log,
            &space,
            &agent,
            at - 2000,
            EventBody::RequestOpened {
                request: expired.clone(),
            },
        )
    })?;
    assert!(
        f.client.requests().iter().any(|r| r.id == expired.id),
        "valid expired proposal is retained"
    );
    let old = expired.clone();
    let valid = original.clone();
    owner_change(&mut f, |owner| async move {
        assert!(owner
            .decide_tool_request(
                old.id,
                old.content_hash,
                client_core::ToolRequestDecision::ApproveOnce
            )
            .is_err());
        owner.decide_tool_request(
            valid.id,
            valid.content_hash,
            client_core::ToolRequestDecision::ApproveOnce,
        )?;
        Ok(())
    })
    .await?;
    assert!(f.client.approved_scene_tool_turn(&expired.id).is_err());
    assert!(
        f.client.approved_scene_tool_turn(&original.id).is_ok(),
        "same Owner can approve the live proposal"
    );
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_signed_review_cannot_lie_about_the_frozen_scene_even_with_a_valid_hash() -> R<()>
{
    let (mut f, original) = held().await?;
    let mut valid = original;
    valid.id = types::new_id("valid_scene_review")?;
    valid.content_hash = valid.tool_proposal_hash(&f.space).unwrap();
    let mut spoofed = valid.clone();
    spoofed.id = types::new_id("lying_scene_review")?;
    spoofed.detail = "This will only read; no drawing content changes".into();
    spoofed.content_hash = spoofed.tool_proposal_hash(&f.space).unwrap();
    assert_eq!(
        spoofed.tool_proposal, valid.tool_proposal,
        "same pinned scene payload"
    );
    let agent = signer(&f.vault);
    let space = f.space.clone();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    rewrite(&mut f, |store, log| {
        append(
            store,
            log,
            &space,
            &agent,
            at,
            EventBody::RequestOpened {
                request: spoofed.clone(),
            },
        )?;
        append(
            store,
            log,
            &space,
            &agent,
            at,
            EventBody::RequestOpened {
                request: valid.clone(),
            },
        )
    })?;
    assert!(!f.client.requests().iter().any(|r| r.id == spoofed.id));
    assert!(
        f.client.requests().iter().any(|r| r.id == valid.id),
        "correct signed detail still projects"
    );
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    dispose(f)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scene_tool_real_owner_renewal_executes_once_and_recovers_after_cold_reopen() -> R<()> {
    let (mut f, original) = held().await?;
    let mut expired = original;
    expired.id = types::new_id("expired_approved_scene")?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    expired.tool_proposal.as_mut().unwrap().expires_at_ms = at - 2000;
    expired.content_hash = expired.tool_proposal_hash(&f.space).unwrap();
    let agent = signer(&f.vault);
    let owner = signer(&f.owner_vault);
    let space = f.space.clone();
    // The old approval was valid when signed. Only its clock has elapsed; its
    // frozen call, native document, renderer and access remain unchanged.
    rewrite(&mut f, |store, log| {
        append(
            store,
            log,
            &space,
            &agent,
            at - 3000,
            EventBody::RequestOpened {
                request: expired.clone(),
            },
        )?;
        append(
            store,
            log,
            &space,
            &owner,
            at - 2500,
            EventBody::RequestResolved {
                request: expired.id.clone(),
                approved: true,
                content_hash: expired.content_hash.clone(),
                resolution: Some(types::RequestResolution {
                    message: "Approved before its expiry".into(),
                    created_item: None,
                    ai_cost_cents: 0,
                    standing_grant: None,
                    tool_decision: Some(types::ToolDecision::ApproveOnce),
                }),
            },
        )
    })?;
    assert_eq!(
        f.client
            .requests()
            .iter()
            .find(|r| r.id == expired.id)
            .unwrap()
            .status,
        client_core::RequestStatus::Approved
    );
    let old_permit = f.client.approved_scene_tool_turn(&expired.id)?;
    assert!(f
        .client
        .agent_scene_replace(old_permit.turn(), &command(&expired))
        .is_err());
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);

    let old = expired.clone();
    owner_change(&mut f, |owner| async move {
        owner.decide_tool_request(
            old.id,
            old.content_hash,
            client_core::ToolRequestDecision::RequestReview,
        )?;
        Ok(())
    })
    .await?;
    let intent = f.client.tool_review_pending_intent(&expired.id).unwrap();
    let pending = f
        .client
        .stage_tool_review_successor(&expired.id, &intent)?
        .unwrap();
    assert_eq!(
        f.client
            .stage_tool_review_successor(&expired.id, &intent)?
            .unwrap()
            .id,
        pending.id,
        "preparing again recovers the existing successor"
    );
    let next = f
        .client
        .tool_approval_requests(&f.space, &f.source)
        .into_iter()
        .find(|r| r.id == pending.id)
        .unwrap();
    let before = expired.tool_proposal.as_ref().unwrap();
    let after = next.tool_proposal.as_ref().unwrap();
    assert_eq!(after.arguments_json, before.arguments_json);
    assert_eq!(after.continuation_json, before.continuation_json);
    assert_eq!(after.review.as_ref().unwrap().predecessor, expired.id);
    assert_eq!(after.review.as_ref().unwrap().generation, 1);
    assert!(after.expires_at_ms > before.expires_at_ms);
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 1);
    let decision = next.clone();
    owner_change(&mut f, |owner| async move {
        owner.decide_tool_request(
            decision.id,
            decision.content_hash,
            client_core::ToolRequestDecision::ApproveOnce,
        )?;
        Ok(())
    })
    .await?;
    let permit = f.client.approved_scene_tool_turn(&next.id)?;
    let accepted = f
        .client
        .agent_scene_replace(permit.turn(), &command(&next))?;
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.scene_json, edited_scene());
    assert_eq!(
        f.client
            .agent_scene_replace(permit.turn(), &command(&next))?,
        accepted
    );
    assert_eq!(
        f.client.item(f.item.clone())?.app.unwrap().resource_uri,
        f.resource_uri
    );
    // Normal signed-event sequencing and unlock reconstruct the same receipt;
    // neither the expired approval nor cold retry can mint another version.
    rewrite(&mut f, |_, _| Ok(()))?;
    let cold = f.client.approved_scene_tool_turn(&next.id)?;
    assert_eq!(
        f.client.agent_scene_replace(cold.turn(), &command(&next))?,
        accepted
    );
    assert_eq!(f.client.app_document_read(f.item.clone())?.version, 2);
    dispose(f)
}
