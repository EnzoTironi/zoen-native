//! Real group, scoped app command, selected link lifetime and one authorized join.
use super::*;
use serde_json::{json, Value};

async fn context(client: &Client, space: &str) -> TestResult<Value> {
    let account = client.account().ok_or("invite actor")?;
    Ok(serde_json::from_str(&client.client_request(json!({
        "op":"space_member_context", "space_id":space, "identity_id":account.identity_id,
        "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
    }).to_string()).await?)?)
}
fn command(context: &Value, uses: u32) -> Value {
    json!({"op":"create_invite_reviewed","input":{
        "space_id":context["space_id"],"expected_identity_id":context["identity_id"],
        "expected_device_id":context["device_id"],"observed_revision":context["revision"],
        "ttl_secs":3600,"max_uses":uses,
    }})
}
fn now() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_invite_uses_selected_limits_and_existing_relay_join_authority() -> TestResult {
    let w = World::new("reviewed_space_invite").await;
    for (id, name) in [("alice", "Alice"), ("bob", "Bob"), ("carla", "Carla")] {
        w.init(id, name);
    }
    w.zoen("alice", &["group", "Reviewed group"]);
    let alice = Open::online(&w, "alice").await?;
    let space = alice
        .0
        .spaces()
        .into_iter()
        .find(|s| s.title == "Reviewed group")
        .ok_or("created group")?
        .id;
    let review = context(&alice.0, &space).await?;
    assert_eq!(review["can_manage_members"], true);
    let legacy: Value = serde_json::from_str(
        &alice
            .0
            .client_request(json!({"op":"create_invite","space_id":space}).to_string())
            .await?,
    )?;
    assert!(protocol::valid_invite_code(
        legacy["code"].as_str().ok_or("legacy invite")?
    ));
    let before = now()?;
    let invite: Value = serde_json::from_str(
        &alice
            .0
            .client_request(command(&review, 1).to_string())
            .await?,
    )?;
    let after = now()?;
    let expiry = if let Some(value) = invite["expires_at_ms"].as_i64() {
        value
    } else {
        invite["expires_at_ms"]
            .as_str()
            .ok_or("invite expiry")?
            .parse::<i64>()?
    };
    assert!(
        (before + 3_600_000..=after + 3_600_000).contains(&expiry),
        "selected one-hour TTL must not become the old seven-day default"
    );
    let code = invite["code"].as_str().ok_or("invite code")?;
    assert_eq!(invite["link"], protocol::links::space(code));
    w.zoen("bob", &["join", code]);
    let bob = Open::online(&w, "bob").await?;
    received(&alice.0, &space, |_| {
        alice
            .0
            .spaces()
            .iter()
            .find(|s| s.id == space)
            .is_some_and(|s| s.members.len() == 2)
    })
    .await?;
    let bob_review = context(&bob.0, &space).await?;
    assert_eq!(bob_review["can_manage_members"], false);
    assert!(bob
        .0
        .client_request(command(&bob_review, 1).to_string())
        .await
        .is_err());
    let carla = Open::online(&w, "carla").await?;
    assert!(
        carla.0.join_invite(code.into()).await.is_err(),
        "one use must be exhausted by the actual Bob join"
    );
    assert_eq!(alice.0.space(space.clone())?.members.len(), 2);
    let mut malformed = command(&review, 1);
    malformed["input"]["role"] = "Owner".into();
    assert!(alice.0.client_request(malformed.to_string()).await.is_err());
    assert!(alice
        .0
        .client_request(command(&review, 0).to_string())
        .await
        .is_err());
    Ok(())
}
