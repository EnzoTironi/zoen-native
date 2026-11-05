//! Exact identity selectors compose with FTS, periods, media and generation-bound cursors.
use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn search_identity_filters_combine_before_limit_and_bind_every_cursor() -> TestResult {
    let core = Client::open(":memory:".into(), "en".into())?;
    let account = core.create_account(
        "Search".into(),
        "search_filter".into(),
        "http://127.0.0.1:1".into(),
        std::sync::Arc::new(MemVault::default()),
    )?;
    let visible = core.create_group_with("Room".into(), vec![], PrivacyDto::Closed)?;
    let other = core.create_group_with("Room".into(), vec![], PrivacyDto::Closed)?;
    for i in 0..35 {
        core.send_message(other.clone(), format!("needle excluded {i}"))?;
        core.send_message(visible.clone(), format!("needle included {i}"))?;
    }
    let request = serde_json::json!({"op":"universal_search_page", "query":"needle", "kinds":[], "cursor":null, "limit":10,
        "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
        "messages":{"space_id":visible,"author_id":account.identity_id},
        "period":{"start_ms":0,"end_ms":8_640_000_000_000_000_i64}});
    let mut next = request.clone();
    let mut ids = std::collections::BTreeSet::new();
    let mut first_cursor = None;
    loop {
        let result: serde_json::Value =
            serde_json::from_str(&core.client_request(next.to_string()).await?)?;
        for hit in result["hits"].as_array().ok_or("hits")? {
            assert_eq!(hit["kind"], "message");
            assert_eq!(hit["space_id"], visible);
            assert_eq!(hit["persona"]["id"], account.identity_id);
            assert!(ids.insert(hit["ref_id"].as_str().ok_or("id")?.to_owned()));
        }
        if result["next_cursor"].is_null() {
            break;
        }
        first_cursor.get_or_insert_with(|| result["next_cursor"].clone());
        next["cursor"] = result["next_cursor"].clone();
    }
    assert_eq!(ids.len(), 35);
    for (field, value) in [
        ("space_id", other.as_str()),
        ("author_id", "unmatched-author"),
    ] {
        let mut changed = request.clone();
        changed["cursor"] = first_cursor.clone().ok_or("cursor")?;
        changed["messages"][field] = value.into();
        assert!(core.client_request(changed.to_string()).await.is_err());
    }
    let mut changed = request.clone();
    changed["messages"]["author_id"] = "unmatched-author".into();
    let result: serde_json::Value =
        serde_json::from_str(&core.client_request(changed.to_string()).await?)?;
    assert_eq!(result["hits"], serde_json::json!([]));
    let mut browse = request.clone();
    browse["query"] = "".into();
    let result: serde_json::Value =
        serde_json::from_str(&core.client_request(browse.to_string()).await?)?;
    assert_eq!(result["hits"].as_array().ok_or("hits")?.len(), 10);
    for invalid in ["", "bad\nidentity"] {
        changed["messages"]["author_id"] = invalid.into();
        assert!(core.client_request(changed.to_string()).await.is_err());
    }
    changed["messages"] = serde_json::json!({"space_id":"inaccessible"});
    assert!(core.client_request(changed.to_string()).await.is_err());
    Ok(())
}
