use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture() -> Result<(std::sync::Arc<Client>, AccountDto, String), Box<dyn std::error::Error>> {
    let core = Client::open(":memory:".into(), "en".into())?;
    core.create_account(
        "Search".into(),
        "search_pages".into(),
        "http://127.0.0.1:1".into(),
        std::sync::Arc::new(MemVault::default()),
    )?;
    let account = core.account().ok_or("account missing")?;
    let space = core.create_group_with("Needle room".into(), vec![], PrivacyDto::Closed)?;
    Ok((core, account, space))
}

fn page(
    core: &Client,
    account: &AccountDto,
    cursor: Option<String>,
    limit: u32,
) -> Result<UniversalSearchPage, CoreError> {
    core.universal_search_page(
        "needle".into(),
        vec![],
        cursor,
        limit,
        account.identity_id.clone(),
        account.device_id.clone(),
    )
}

#[test]
fn global_pages_are_complete_stable_and_total_bounded() -> TestResult {
    let (core, account, space) = fixture()?;
    let other = core.create_group_with("Needle other".into(), vec![], PrivacyDto::Closed)?;
    let mut expected = Vec::new();
    for i in 0..67 {
        expected.push(
            core.send_message(
                if i % 2 == 0 {
                    space.clone()
                } else {
                    other.clone()
                },
                format!("needle entry {i}"),
            )?
            .id,
        );
    }
    let mut cursor = None;
    let mut hits = Vec::new();
    loop {
        let result = page(&core, &account, cursor, 30)?;
        assert!(result.hits.len() <= 30);
        hits.extend(result.hits);
        cursor = result.next_cursor;
        if cursor.is_none() {
            break;
        }
        assert!(hits.len() <= 69);
    }
    let identities: std::collections::BTreeSet<_> = hits
        .iter()
        .map(|hit| (&hit.kind, &hit.space_id, &hit.ref_id))
        .collect();
    assert_eq!(hits.len(), identities.len());
    assert_eq!(hits.len(), 69);
    let mut actual: Vec<_> = hits
        .iter()
        .filter(|hit| hit.kind == "message")
        .map(|hit| hit.ref_id.clone())
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    let all = page(&core, &account, None, 100)?;
    assert_eq!(hits, all.hits);
    assert!(all.next_cursor.is_none());
    Ok(())
}

#[test]
fn global_cursor_rejects_query_filter_owner_and_index_mutations() -> TestResult {
    let (core, account, space) = fixture()?;
    for _ in 0..3 {
        core.send_message(space.clone(), "needle".into())?;
    }
    let cursor = page(&core, &account, None, 1)?
        .next_cursor
        .ok_or("cursor missing")?;
    for (query, kinds, identity, device) in [
        (
            "different",
            vec![],
            account.identity_id.clone(),
            account.device_id.clone(),
        ),
        (
            "needle",
            vec!["message".into()],
            account.identity_id.clone(),
            account.device_id.clone(),
        ),
        ("needle", vec![], "other".into(), account.device_id.clone()),
        (
            "needle",
            vec![],
            account.identity_id.clone(),
            "other".into(),
        ),
    ] {
        assert!(core
            .universal_search_page(
                query.into(),
                kinds,
                Some(cursor.clone()),
                1,
                identity,
                device
            )
            .is_err());
    }
    core.send_message(space, "needle concurrent".into())?;
    assert!(page(&core, &account, Some(cursor), 1).is_err());
    assert_eq!(page(&core, &account, None, 100)?.hits.len(), 5);
    Ok(())
}

#[test]
fn global_page_filters_revoked_visibility_before_limit_and_rejects_old_access() -> TestResult {
    let (core, account, space) = fixture()?;
    for _ in 0..35 {
        core.send_message(space.clone(), "needle private".into())?;
    }
    let visible = core.create_group_with("Needle visible".into(), vec![], PrivacyDto::Closed)?;
    core.send_message(visible.clone(), "needle visible".into())?;
    let cursor = page(&core, &account, None, 1)?
        .next_cursor
        .ok_or("cursor missing")?;
    // Isolate the access fence from index invalidation: stale index still contains
    // the private rows, while canonical membership has already been revoked.
    core.lock()
        .state
        .spaces
        .get_mut(&space)
        .ok_or("space missing")?
        .members
        .clear();
    assert!(page(&core, &account, Some(cursor), 30).is_err());
    let remaining = page(&core, &account, None, 30)?;
    assert_eq!(remaining.hits.len(), 2);
    assert!(remaining
        .hits
        .iter()
        .all(|hit| hit.space_id.as_deref() == Some(visible.as_str())));
    for limit in [0, 101, u32::MAX] {
        assert!(page(&core, &account, None, limit).is_err());
    }
    assert!(page(&core, &account, Some("invalid".into()), 30).is_err());
    Ok(())
}

#[tokio::test]
async fn global_search_command_returns_real_pages_and_refuses_stale_edit_delete() -> TestResult {
    let (core, account, space) = fixture()?;
    let first = core.send_message(space.clone(), "needle first".into())?;
    core.send_message(space.clone(), "needle second".into())?;
    let request = serde_json::json!({
        "op": "universal_search_page", "query": "needle", "kinds": ["message"],
        "cursor": null, "limit": 1, "expected_identity_id": account.identity_id,
        "expected_device_id": account.device_id
    });
    let response: serde_json::Value =
        serde_json::from_str(&core.client_request(request.to_string()).await?)?;
    assert_eq!(response["hits"].as_array().ok_or("hits missing")?.len(), 1);
    assert!(response["next_cursor"].is_string());
    assert!(
        response["hits"][0]["at_ms"].is_string(),
        "bridge preserves exact i64 timestamps"
    );
    core.edit_message(space.clone(), first.id.clone(), "needle changed".into())?;
    let mut continuation = request.clone();
    continuation["cursor"] = response["next_cursor"].clone();
    assert!(core.client_request(continuation.to_string()).await.is_err());
    let refreshed: serde_json::Value =
        serde_json::from_str(&core.client_request(request.to_string()).await?)?;
    core.delete_message(space, first.id)?;
    continuation["cursor"] = refreshed["next_cursor"].clone();
    assert!(core.client_request(continuation.to_string()).await.is_err());
    let remaining: serde_json::Value =
        serde_json::from_str(&core.client_request(request.to_string()).await?)?;
    assert_eq!(remaining["hits"].as_array().ok_or("hits missing")?.len(), 1);
    assert!(remaining["next_cursor"].is_null());
    Ok(())
}
