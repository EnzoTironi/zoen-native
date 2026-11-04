use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn link_search_uses_current_canonical_text_captions_scope_and_cursor_generation() -> TestResult
{
    let core = Client::open(":memory:".into(), "en".into())?;
    let account = core.create_account(
        "Search".into(),
        "link_search".into(),
        "http://127.0.0.1:1".into(),
        std::sync::Arc::new(MemVault::default()),
    )?;
    let space = core.create_group_with("Visible".into(), vec![], PrivacyDto::Closed)?;
    let other = core.create_group_with("Other".into(), vec![], PrivacyDto::Closed)?;
    let first = core
        .send_message(
            space.clone(),
            "[Release](https://example.org/needle)".into(),
        )?
        .id;
    let second = core
        .send_file_message_with_caption(
            space.clone(),
            "photo.png".into(),
            "image/png".into(),
            vec![1; 64],
            None,
            None,
            false,
            "link-photo".into(),
            "needle https://example.org".into(),
        )?
        .id;
    for text in [
        "needle plain",
        "needle `https://example.org`",
        "needle person@example.org",
        "needle [fake](javascript:alert)",
    ] {
        core.send_message(space.clone(), text.into())?;
    }
    core.send_file_message(
        space.clone(),
        "example.org".into(),
        "image/png".into(),
        vec![2; 64],
        None,
        None,
        false,
        "url-filename".into(),
    )?;
    for _ in 0..35 {
        core.send_message(other.clone(), "needle https://example.org".into())?;
    }
    let base = serde_json::json!({"op":"universal_search_page", "query":"needle", "kinds":["link"], "cursor":null, "limit":1,
        "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
        "messages":{"space_id":space,"author_id":account.identity_id}});
    let page: serde_json::Value =
        serde_json::from_str(&core.client_request(base.to_string()).await?)?;
    assert_eq!(page["hits"].as_array().ok_or("hits")?.len(), 1);
    assert!(page["next_cursor"].is_string());
    let mut next = base.clone();
    next["cursor"] = page["next_cursor"].clone();
    let tail: serde_json::Value =
        serde_json::from_str(&core.client_request(next.to_string()).await?)?;
    assert_eq!(tail["hits"].as_array().ok_or("hits")?.len(), 1);
    assert!(tail["next_cursor"].is_null());
    let actual: std::collections::BTreeSet<_> = [
        page["hits"][0]["ref_id"].as_str().ok_or("first id")?,
        tail["hits"][0]["ref_id"].as_str().ok_or("second id")?,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        actual,
        [first.as_str(), second.as_str()].into_iter().collect()
    );
    let mut changed = next.clone();
    changed["kinds"] = serde_json::json!(["media"]);
    assert!(core.client_request(changed.to_string()).await.is_err());
    core.edit_message(space.clone(), first, "needle no link".into())?;
    assert!(core.client_request(next.to_string()).await.is_err());
    let mut browse = base.clone();
    browse["query"] = serde_json::json!("");
    let remaining: serde_json::Value =
        serde_json::from_str(&core.client_request(browse.to_string()).await?)?;
    assert_eq!(remaining["hits"][0]["ref_id"], second);
    assert!(remaining["next_cursor"].is_null());
    core.delete_message(space, second)?;
    let deleted: serde_json::Value =
        serde_json::from_str(&core.client_request(browse.to_string()).await?)?;
    assert_eq!(deleted["hits"], serde_json::json!([]));
    for kinds in [
        serde_json::json!(["link", "message"]),
        serde_json::json!(["link:unknown"]),
    ] {
        browse["kinds"] = kinds;
        assert!(core.client_request(browse.to_string()).await.is_err());
    }
    Ok(())
}
