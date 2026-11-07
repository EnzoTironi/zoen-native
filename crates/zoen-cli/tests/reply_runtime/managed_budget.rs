//! First budget through the exact native review/confirmation commands, not operator import.
use super::*;
use std::time::{Duration, Instant};

async fn request(owner: &Client, input: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let response = owner
        .client_request(input.to_string())
        .await
        .map_err(|error| format!("native budget {}: {error}", input["op"]))?;
    Ok(serde_json::from_str(&response)?)
}

pub(super) async fn install(
    w: &World,
    owner: &Client,
    service: &mut managed_support::ServiceProcess,
    agent: &str,
    profile: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let account = owner.account().ok_or("budget owner")?;
    let space = request(owner, json!({"op":"start_owner_policy_direct",
        "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,"agent_id":agent}))
        .await?.as_str().ok_or("budget conversation")?.to_owned();
    let scope = json!({"expected_identity_id":account.identity_id,
        "expected_device_id":account.device_id,"agent_id":agent,"space_id":space});
    let inquiry = request(
        owner,
        json!({"op":"request_account_policy_options","scope":scope}),
    )
    .await?;
    // The phone's canonical-change callback performs the same reconciliation
    // after its must-seal inquiry and private MLS bootstrap settle. Pending
    // delivery is not permission to bypass the ready-group read guard.
    owner.wait_until_settled(20_000).await?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let context = loop {
        if let Some(exit) = service.child.try_wait()? {
            return Err(format!("managed budget service exited: {exit}").into());
        }
        let status = request(
            owner,
            json!({"op":"owner_policy_options","scope":scope,"source":inquiry["source"]}),
        )
        .await?;
        if status["context"].is_object() {
            break status["context"].clone();
        }
        if status["state"] == "refused" || Instant::now() >= deadline {
            return Err(format!("managed budget options did not arrive: {status}").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(context["service_kind"], "conversation");
    assert_eq!(context["profile_digest"], profile);
    assert_eq!(context["reservation_units"], "110");
    assert!(context["current_policy_digest"].is_null());
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    let prepared = request(
        owner,
        json!({"op":"prepare_owner_policy","scope":scope,
        "source":inquiry["source"],"options_digest":context["options_digest"],
        "limit_units":"500","max_attempt_units":"110","enabled":true}),
    )
    .await?;
    let reviewed = request(
        owner,
        json!({"op":"review_owner_policy","scope":scope,
        "policy_json":prepared["canonical_policy_json"]}),
    )
    .await?;
    assert_eq!(
        prepared["confirmation_digest"],
        reviewed["confirmation_digest"]
    );
    let confirmation = json!({"op":"submit_owner_policy","scope":scope,
        "policy_json":reviewed["canonical_policy_json"],"confirmation_digest":reviewed["confirmation_digest"]});
    let submitted = request(owner, confirmation.clone()).await?;
    let repeated = request(owner, confirmation).await?;
    assert_eq!(
        submitted["source"], repeated["source"],
        "Lost confirmation ACK reuses the exact original"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(exit) = service.child.try_wait()? {
            return Err(format!("managed budget service exited: {exit}").into());
        }
        let status = request(
            owner,
            json!({"op":"owner_policy_status","scope":scope,"source":submitted["source"]}),
        )
        .await?;
        if status["state"] == "installed" {
            assert_eq!(status["policy_digest"], reviewed["policy_digest"]);
            break;
        }
        if status["state"] == "refused" || Instant::now() >= deadline {
            return Err(format!("managed budget confirmation did not settle: {status}").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(w.count("SELECT count(*) FROM runtime_policies").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    Ok(space)
}
