use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;
#[tokio::test]
async fn canonical_media_categories_bind_cursor_scope_and_observe_deletion() -> TestResult {
    let core = Client::open(":memory:".into(), "en".into())?;
    let account = core.create_account(
        "Search".into(),
        "media_category".into(),
        "http://127.0.0.1:1".into(),
        std::sync::Arc::new(MemVault::default()),
    )?;
    let space = core.create_group_with("Visible".into(), vec![], PrivacyDto::Closed)?;
    let other = core.create_group_with("Other".into(), vec![], PrivacyDto::Closed)?;
    let mut expected = std::collections::BTreeMap::new();
    for (index, kind, mime) in [
        (1, "photo", "image/png"),
        (2, "video", "video/mp4"),
        (3, "audio", "audio/mpeg"),
        (4, "file", "application/pdf"),
    ] {
        let entry = core.send_file_message_with_caption(
            space.clone(),
            format!("file-{index}"),
            mime.into(),
            vec![index; 64],
            None,
            None,
            false,
            format!("category-{index}"),
            "needle".into(),
        )?;
        expected.insert(kind, entry.id);
    }
    let voice = core.send_voice_note(
        space.clone(),
        vec![7; 64],
        "audio/mp4".into(),
        1000,
        vec![0.2, 0.4],
        "needle".into(),
        None,
        false,
    )?;
    expected.insert("voice", voice.id);
    for i in 0..35 {
        core.send_file_message(
            other.clone(),
            format!("outside-{i}"),
            "image/png".into(),
            vec![6; 64],
            None,
            None,
            false,
            format!("outside-{i}"),
        )?;
    }
    let base = serde_json::json!({"op":"universal_search_page", "query":"", "kinds":["media"], "cursor":null, "limit":1,
        "expected_identity_id":account.identity_id,"expected_device_id":account.device_id,
        "messages":{"space_id":space,"author_id":account.identity_id}});
    for (kind, id) in &expected {
        let mut query = base.clone();
        query["kinds"] = serde_json::json!([format!("media:{kind}")]);
        let value: serde_json::Value =
            serde_json::from_str(&core.client_request(query.to_string()).await?)?;
        let hits = value["hits"].as_array().ok_or("hits")?;
        assert_eq!(hits.len(), 1, "{kind}");
        assert_eq!(hits[0]["ref_id"], *id);
        assert_eq!(hits[0]["space_id"], space);
        assert!(value["next_cursor"].is_null());
    }
    let all: serde_json::Value =
        serde_json::from_str(&core.client_request(base.to_string()).await?)?;
    let mut changed = base.clone();
    changed["kinds"] = serde_json::json!(["media:photo"]);
    changed["cursor"] = all["next_cursor"].clone();
    assert!(core.client_request(changed.to_string()).await.is_err());
    changed["cursor"] = serde_json::Value::Null;
    core.delete_message(space.clone(), expected["photo"].clone())?;
    let deleted: serde_json::Value =
        serde_json::from_str(&core.client_request(changed.to_string()).await?)?;
    assert_eq!(deleted["hits"], serde_json::json!([]));
    for kinds in [
        serde_json::json!(["media:unknown"]),
        serde_json::json!(["media:photo", "message"]),
    ] {
        changed["kinds"] = kinds;
        assert!(core.client_request(changed.to_string()).await.is_err());
    }
    Ok(())
}
