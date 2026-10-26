//! Schema6 uses the same real signed owner-intent lineage as other artifacts.
use super::pending_history::*;
use super::*;

#[test]
fn expired_pending_owned_scene_read_renews_once_and_recovers_cold_without_a_new_source(
) -> TestResult {
    for targeted in [false, true] {
        let mut o = owned(targeted)?;
        let baseline = read_owned(&o)?;
        assert_eq!(baseline.resource_uri, o.pin);
        let live = stage_scene(&o)?;
        let old = expired_pending(&o.f, &live)?;
        let original = o.f.agent.lock().state.requests[&old.id].clone();
        let sources = source_count(&o.f);
        let request =
            o.f.agent
                .agent_artifact_request(&o.f.space, &o.f.source, &o.f.owner)?
                .ok_or("source")?;
        assert_eq!(
            request.agent_id.is_some(),
            targeted,
            "legacy source shape stays exact"
        );
        assert!(owner_decision(&o, &old, ToolRequestDecision::ApproveOnce).is_err());
        deliver(&o.f.agent, &o.f.owner_client, &o.f.space)?;
        let visible =
            o.f.owner_client
                .requests()
                .into_iter()
                .find(|r| r.id == old.id)
                .ok_or("owner view")?;
        assert_eq!(visible.status, RequestStatus::Stale);
        assert_eq!(
            visible
                .tool_approval
                .as_ref()
                .and_then(|a| a.review.as_ref())
                .map(|r| r.state),
            Some(ToolReviewState::Available)
        );
        let before = counts(&o.f);
        owner_decision(&o, &old, ToolRequestDecision::RequestReview)?;
        let intent =
            o.f.agent
                .tool_review_pending_intent(&old.id)
                .ok_or("owner intent")?;
        assert_eq!(read_owned(&o)?, baseline);
        assert_eq!(
            &counts(&o.f)[1..4],
            &before[1..4],
            "owner intent performs no IO/effect"
        );
        let after_owner = counts(&o.f);
        owner_decision(&o, &old, ToolRequestDecision::RequestReview)?;
        assert_eq!(
            counts(&o.f),
            after_owner,
            "lost owner ACK recovers the same intent"
        );
        let next =
            o.f.agent
                .stage_tool_review_successor(&old.id, &intent)?
                .ok_or("successor")?;
        assert_eq!(next.status, RequestStatus::Pending);
        assert!(o.f.agent.approved_scene_tool_turn(&old.id).is_err());
        assert!(o.f.agent.approved_scene_tool_turn(&next.id).is_err());
        let new = o.f.agent.lock().state.requests[&next.id].req.clone();
        let proposal = new.tool_proposal.as_ref().ok_or("successor proposal")?;
        let mut expected = original
            .req
            .tool_proposal
            .as_ref()
            .ok_or("old proposal")?
            .as_ref()
            .clone();
        expected.expires_at_ms = proposal.expires_at_ms;
        expected.review = proposal.review.clone();
        assert_eq!(
            &expected,
            proposal.as_ref(),
            "call/source/head/policy/worker remain frozen"
        );
        assert_eq!(
            proposal.review.as_ref().map(|r| r.predecessor.as_str()),
            Some(old.id.as_str())
        );
        assert_eq!(source_count(&o.f), sources);
        confirm_pending(&o.f)?;
        let directory = std::env::temp_dir().join(types::new_id("pending_scene_cold")?);
        std::fs::create_dir(&directory)?;
        let path = directory.join("agent.sqlite");
        // Isolated fixture snapshot, matching existing cold-store tests. No live home or seed row.
        o.f.agent
            .lock()
            .store
            .conn()
            .execute("VACUUM INTO ?1", [path.to_str().ok_or("fixture path")?])?;
        let cold = Client::open(path.to_str().ok_or("fixture path")?.into(), "en".into())?;
        assert!(cold.unlock(o.f.agent_vault.clone())?);
        o.f.agent = cold;
        let after_stage = counts(&o.f);
        assert_eq!(
            o.f.agent
                .stage_tool_review_successor(&old.id, &intent)?
                .map(|r| r.id),
            Some(next.id.clone())
        );
        assert_eq!(
            counts(&o.f),
            after_stage,
            "cold lost staging ACK does not duplicate successor"
        );
        assert!(o.f.agent.approved_scene_tool_turn(&next.id).is_err());
        assert_eq!(read_owned(&o)?, baseline);
        owner_decision(&o, &next, ToolRequestDecision::ApproveOnce)?;
        let permit = o.f.agent.approved_scene_tool_turn(&next.id)?;
        assert_eq!(o.f.agent.agent_scene_finish_read(permit.turn())?, baseline);
        let old_state = o.f.agent.lock().state.requests[&old.id].clone();
        assert_eq!(old_state.req, original.req);
        assert!(
            old_state.resolution.is_none(),
            "renewal never manufactures an old approval"
        );
        assert_eq!(source_count(&o.f), sources);
        assert!(o.f.agent.approved_scene_tool_turn(&old.id).is_err());
        drop(permit);
        drop(o);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn pending_owned_scene_renewal_refuses_changed_head_pin_grant_or_source() -> TestResult {
    for change in ["head", "pin", "grant", "source"] {
        let o = owned(false)?;
        let old = expired_pending(&o.f, &stage_scene(&o)?)?;
        deliver(&o.f.agent, &o.f.owner_client, &o.f.space)?;
        let document = o.f.owner_client.app_document_read(o.item.clone())?;
        match change {
            "head" => {
                let mut scene: Value = serde_json::from_str(&document.scene_json)?;
                scene["appState"]["viewBackgroundColor"] = json!("#ffeecc");
                let scene = scene.to_string();
                assert_ne!(scene, document.scene_json);
                o.f.owner_client.lock().save_app_document(
                    &o.item,
                    document.version,
                    (&document.sha256, Some(&o.pin)),
                    &scene,
                    "owner_changed_scene",
                )?;
            }
            "pin" => {
                let html = b"<!doctype html><p>Different admitted renderer</p>";
                let version = o.f.owner_client.file_new_version(
                    o.install.package_item_id.clone(),
                    html.to_vec(),
                    None,
                    "New renderer".into(),
                )?;
                let mut admission = o.install.clone();
                admission.package_version = version.version;
                admission.package_sha256 = sha(html);
                admission.mutation_id = "owner_changed_renderer".into();
                let updated =
                    o.f.owner_client
                        .lock()
                        .update_app_resource(crate::app_resources::Update {
                            admission,
                            item_id: o.item.clone(),
                            expected_version: document.version,
                            expected_sha256: document.sha256.clone(),
                            expected_resource_uri: o.pin.clone(),
                        })?;
                assert_eq!(
                    updated.sha256, document.sha256,
                    "scene bytes unchanged across UI release"
                );
                assert_ne!(
                    o.f.owner_client
                        .item(o.item.clone())?
                        .app
                        .ok_or("App")?
                        .resource_uri,
                    o.pin
                );
            }
            "grant" => {
                let agent = o.f.agent.account().ok_or("Agent account")?.identity_id;
                o.f.owner_client
                    .lock()
                    .set_trust(&agent, &o.f.space, TrustLevel::Listen)?;
            }
            _ => {
                o.f.owner_client.edit_message(
                    o.f.space.clone(),
                    o.f.source.clone(),
                    "Changed source".into(),
                )?;
            }
        }
        deliver(&o.f.owner_client, &o.f.agent, &o.f.space)?;
        let before = counts(&o.f);
        assert!(
            owner_decision(&o, &old, ToolRequestDecision::RequestReview).is_err(),
            "{change}"
        );
        assert!(o.f.agent.tool_review_pending_intent(&old.id).is_none());
        assert_eq!(counts(&o.f), before, "{change}");
    }
    Ok(())
}

#[test]
fn explicit_owned_source_for_another_agent_never_stages_a_review_or_renewal() -> TestResult {
    let mut o = owned(true)?;
    let source = append_source(&o.f, &o.item, Some("another_agent".into()))?;
    deliver(&o.f.owner_client, &o.f.agent, &o.f.space)?;
    o.f.source = source;
    let before = counts(&o.f);
    assert!(o
        .f
        .agent
        .agent_artifact_request(&o.f.space, &o.f.source, &o.f.owner)
        .is_err());
    assert!(o
        .f
        .agent
        .agent_scene_turn(&o.item, &o.f.source, &o.f.owner)
        .is_err());
    assert!(stage_scene(&o).is_err());
    assert_eq!(counts(&o.f), before);
    Ok(())
}
