//! Service review -> actual iOS command boundary -> cold service continuation.
use super::managed_support::*;
use super::*;
use std::time::Duration;

async fn native(owner: &Client, input: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let output = Box::pin(owner.client_request(input.to_string())).await?;
    Ok(serde_json::from_str(&output)?)
}

pub(super) async fn resolve(
    w: &World,
    path: &Path,
    mut process: ServiceProcess,
    space: &str,
    provider: &provider::Provider,
    decision: client_core::ToolRequestDecision,
) -> Result<ServiceProcess, Box<dyn std::error::Error>> {
    let expected_status = match decision {
        client_core::ToolRequestDecision::ApproveOnce => "Approved",
        client_core::ToolRequestDecision::Deny => "Denied",
        client_core::ToolRequestDecision::Cancel => "Cancelled",
        client_core::ToolRequestDecision::RequestReview => {
            return Err("renewal is not a terminal decision".into())
        }
    };
    let approved = decision == client_core::ToolRequestDecision::ApproveOnce;
    let review = process.frame("review").await?;
    assert_eq!(review["review_status"], "Pending");
    assert_eq!(review["financial"], json!({"status":"settled","units":6}));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    // The service is actually dead during the owner decision. Nothing injects
    // a resolution into its database or approves through an operator endpoint.
    drop(process);
    let home = w.dir.join("owner-linked");
    let owner = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: home.join("vault")
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    let requests = native(&owner, json!({"op":"requests"})).await?;
    let request = requests
        .as_array()
        .ok_or("request list")?
        .iter()
        .find(|request| request["id"] == review["request"])
        .ok_or("visible review")?;
    assert_eq!(request["status"], "Pending");
    assert_eq!(request["space_id"], space);
    assert_eq!(request["agent"]["is_mine"], true);
    let tool = &request["tool_approval"];
    assert_eq!(tool["canonical_tool"], "search_conversation_history");
    assert_eq!(tool["preview_kind"], "conversation");
    assert_eq!(tool["artifact_id"], "");
    assert_eq!(tool["version"], 0);
    assert_eq!(tool["content_hash"], review["content_hash"]);
    // These exact envelope shapes are the RequestCard command contract.
    assert!(native(
        &owner,
        json!({"op":"decide_request","request_id":request["id"],"decision":"Approve"})
    )
    .await
    .is_err());
    assert!(native(
        &owner,
        json!({"op":"decide_tool_request","request_id":request["id"],
        "observed_content_hash":"00".repeat(32),"decision":"ApproveOnce"})
    )
    .await
    .is_err());
    let command = json!({"op":"decide_tool_request","request_id":request["id"],
        "observed_content_hash":tool["content_hash"],"decision":decision});
    native(&owner, command.clone()).await?;
    // A repeat cannot open a standing grant, create another review or spend.
    let _ = native(&owner, command).await;
    owner.wait_until_settled(20_000).await?;
    let resolved = native(&owner, json!({"op":"requests"})).await?;
    let resolved = resolved.as_array().ok_or("resolved requests")?;
    let matching: Vec<_> = resolved
        .iter()
        .filter(|row| row["id"] == request["id"])
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0]["status"], expected_status);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
    assert_eq!(
        w.scalar("SELECT sum(spent_units)::text FROM runtime_periods")
            .await,
        "6"
    );
    assert_eq!(
        w.scalar("SELECT sum(held_units)::text FROM runtime_periods")
            .await,
        "0"
    );
    owner.stop_sync();
    drop(owner);
    let mut process = ServiceProcess::start(w, path)?;
    process.frame("worker_ready").await?;
    if !approved {
        let denied = process.frame_for("review", Duration::from_secs(30)).await?;
        assert_eq!(denied["request"], review["request"]);
        assert_eq!(denied["review_status"], expected_status);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        drop(process);
        process = ServiceProcess::start(w, path)?;
        process.frame("worker_ready").await?;
        let repeated = process.frame("review").await?;
        assert_eq!(repeated["review_status"], expected_status);
        assert_eq!(repeated["request"], review["request"]);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 1);
        let history = w.zoen(
            "owner",
            &[
                "read",
                &format!(
                    "@{}",
                    request["agent"]["handle"].as_str().ok_or("agent handle")?
                ),
            ],
        );
        assert!(!history.contains(provider::ANSWER));
    }
    Ok(process)
}
