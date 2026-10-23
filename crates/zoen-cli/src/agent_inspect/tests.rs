use super::*;
use client_core::{AgentRequestDto, PrivacyDto, ToolApprovalDto};
use std::time::Duration;
mod relay;
use relay::{citizen, online, relay, Mode, PRIVATE};

#[test]
fn arguments_and_home_preflight_fail_without_creating_a_store() {
    let argv = |parts: &[&str]| {
        parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
    };
    assert!(validate_args(&argv(&["agent-inspect"])).is_ok());
    assert!(validate_args(&argv(&[
        "--home",
        "/synthetic",
        "agent-inspect",
        "--timeout",
        "1000"
    ]))
    .is_ok());
    for parts in [
        vec![],
        vec!["agent-inspect", "--artifact", "it_owned"],
        vec!["agent-inspect", "--offline"],
        vec!["agent-inspect", "--for", "invalid"],
        vec!["agent-inspect", "--timeout"],
        vec!["agent-inspect", "--timeout", "0"],
        vec!["agent-inspect", "--timeout", "invalid"],
        vec!["agent-inspect", "--home", "/one", "--home", "/two"],
    ] {
        assert!(validate_args(&argv(&parts)).is_err());
    }
    assert!(crate::agent_enrollment::requires_home_lock("agent-inspect"));
    let missing = std::env::temp_dir().join(types::new_id("zoen-inspect-missing"));
    assert_eq!(
        existing_home(&missing),
        Err("agent_inspect_existing_home_required")
    );
    assert!(!missing.exists());
    assert_eq!(
        open_existing(&missing).err(),
        Some("agent_inspect_existing_home_required")
    );
    assert!(!missing.exists());
}

#[tokio::test]
async fn missing_person_and_zero_timeout_refuse_before_agent_state() {
    let server = relay(Mode::Normal).await;
    let empty = Client::open(":memory:".into(), "en".into()).unwrap();
    assert_eq!(
        inspect(&empty, 100).await,
        Err("agent_inspect_missing_account")
    );
    let person = citizen(&server.url, false);
    assert_eq!(
        inspect(&person.0, 100).await,
        Err("agent_inspect_not_agent")
    );
    let agent = citizen(&server.url, true);
    assert_eq!(
        preflight(&agent.0),
        Err("agent_inspect_existing_registered_agent_required")
    );
    assert_eq!(
        inspect(&agent.0, 0).await,
        Err("agent_inspect_positive_timeout_required")
    );
    assert!(server.calls.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_client_inventory_preserves_cursor_and_emits_no_action_or_receipt() {
    let server = relay(Mode::Normal).await;
    let agent = citizen(&server.url, true);
    let space = agent
        .0
        .create_group_with(PRIVATE.into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let source = agent.0.send_message(space.clone(), PRIVATE.into()).unwrap();
    agent
        .0
        .send_reply(space.clone(), PRIVATE.into(), source.id.clone(), false)
        .unwrap();
    online(&agent.0, true).await;
    server.cursors.lock().unwrap().push((space.clone(), 0));
    server.calls.lock().unwrap().clear();
    let before = agent.0.timeline(space.clone()).unwrap();
    let text = inspect(&agent.0, 1000).await.unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert!(!text.contains(PRIVATE));
    assert_eq!(value["schema"], "zoen.agent.inspect.v1");
    assert_eq!(value["agentId"], agent.0.account().unwrap().identity_id);
    assert_eq!(value["ownerId"], agent.0.agent_owner().unwrap());
    assert_eq!(value["definitionVersion"], 2);
    assert_eq!(value["cursors"][&space], 0);
    assert_eq!(value["spaces"][0]["cursor"], 0);
    let rows = value["spaces"][0]["entries"].as_array().unwrap();
    assert_eq!(rows.len(), before.len());
    assert!(
        rows.iter().find(|entry| entry["id"] == source.id).unwrap()["replyExists"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        value["spaces"][0]["head"],
        before.iter().map(|entry| entry.seq).max().unwrap()
    );
    assert_eq!(agent.0.timeline(space.clone()).unwrap(), before);
    assert_eq!(*server.cursors.lock().unwrap(), vec![(space, 0)]);
    assert_eq!(*server.calls.lock().unwrap(), vec!["agent_state"]);
    assert_eq!(value["coverage"]["wholeRelayHistory"], false);
    assert_eq!(value["coverage"]["atomicSnapshot"], false);
    assert_eq!(value["coverage"]["olderHistoryFetched"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_cursor_is_reported_without_seeding_or_readiness() {
    let server = relay(Mode::Normal).await;
    let agent = citizen(&server.url, true);
    agent
        .0
        .create_group_with(PRIVATE.into(), vec![], PrivacyDto::Closed)
        .unwrap();
    online(&agent.0, true).await;
    server.calls.lock().unwrap().clear();
    let output: Value = serde_json::from_str(&inspect(&agent.0, 1000).await.unwrap()).unwrap();
    assert!(output["spaces"][0]["cursor"].is_null());
    assert!(server.cursors.lock().unwrap().is_empty());
    assert_eq!(*server.calls.lock().unwrap(), vec!["agent_state"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_refusal_invalid_state_and_duplicate_cursor_are_closed_errors() {
    for (mode, expected) in [
        (Mode::StateDenied, "agent_inspect_state_unavailable"),
        (Mode::InvalidState, "agent_inspect_invalid_state"),
        (Mode::DuplicateCursor, "agent_inspect_duplicate_cursor"),
    ] {
        let server = relay(mode).await;
        let agent = citizen(&server.url, true);
        online(&agent.0, true).await;
        server.calls.lock().unwrap().clear();
        let error = inspect(&agent.0, 1000).await.unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.contains(PRIVATE));
        assert_eq!(*server.calls.lock().unwrap(), vec!["agent_state"]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn online_but_unsynced_never_queries_agent_state() {
    let server = relay(Mode::Unsynced).await;
    let agent = citizen(&server.url, true);
    online(&agent.0, false).await;
    assert_eq!(
        inspect(&agent.0, 100).await,
        Err("agent_inspect_sync_unsettled")
    );
    assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_retirement_during_state_read_cannot_return_old_inventory() {
    let server = relay(Mode::HoldState).await;
    let agent = citizen(&server.url, true);
    online(&agent.0, true).await;
    server.calls.lock().unwrap().clear();
    let client = agent.0.clone();
    let pending = tokio::spawn(async move { inspect(&client, 1000).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !server.calls.lock().unwrap().contains(&"agent_state") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    agent.0.reset_demo().unwrap();
    server.release.notify_one();
    assert!(pending.await.unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unacknowledged_state_read_is_bounded_without_retry_or_partial_inventory() {
    let server = relay(Mode::HoldState).await;
    let agent = citizen(&server.url, true);
    online(&agent.0, true).await;
    server.calls.lock().unwrap().clear();
    let result = tokio::time::timeout(Duration::from_secs(2), inspect(&agent.0, 100))
        .await
        .unwrap();
    assert_eq!(result, Err("agent_inspect_state_unavailable"));
    assert_eq!(*server.calls.lock().unwrap(), vec!["agent_state"]);
    server.release.notify_one();
}

#[test]
fn request_projection_exports_only_closed_fields_and_keeps_unknown_source_null() {
    let agent = citizen("http://127.0.0.1:1", true);
    let dto = AgentRequestDto {
        tool_approval: Some(ToolApprovalDto {
            preview_kind: None,
            review: None,
            content_hash: PRIVATE.into(),
            artifact_id: "it_deck_owned".into(),
            artifact_title: PRIVATE.into(),
            version: 27,
            operation: PRIVATE.into(),
            expires_ms: 1,
        }),
        id: "rq_inspect".into(),
        agent: agent.0.me().unwrap(),
        space_id: "sp_inspect".into(),
        space_title: PRIVATE.into(),
        title: PRIVATE.into(),
        detail: PRIVATE.into(),
        audience: PRIVATE.into(),
        action_label: PRIVATE.into(),
        cost_cents: Some(123),
        reason: PRIVATE.into(),
        status: RequestStatus::Pending,
        opened_ms: 1,
        resolved_ms: None,
        item_id: None,
        line_id: Some(PRIVATE.into()),
        action_key: PRIVATE.into(),
        can_always_approve: true,
        by_standing: false,
    };
    let output = request(&dto, &BTreeMap::new()).unwrap();
    assert!(output["sourceId"].is_null());
    assert_eq!(output["version"], 27);
    assert_eq!(
        output
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "id",
            "spaceId",
            "kind",
            "status",
            "artifactId",
            "version",
            "sourceId"
        ])
    );
    assert!(!output.to_string().contains(PRIVATE));
    let mut foreign = dto.clone();
    foreign.agent.id = "foreign_agent".into();
    let (rows, unknown) =
        request_rows(vec![dto.clone(), foreign], &dto.agent.id, &BTreeMap::new()).unwrap();
    assert_eq!(rows, vec![output]);
    assert_eq!(
        unknown, 1,
        "an unresolved source must remain explicit coverage debt"
    );
    let sources = BTreeMap::from([("rq_inspect".into(), "source_inspect".into())]);
    assert_eq!(
        request(&dto, &sources).unwrap()["sourceId"],
        "source_inspect"
    );
    assert_eq!(
        request_rows(vec![dto.clone()], &dto.agent.id, &sources)
            .unwrap()
            .1,
        0
    );
    for (status, expected) in [
        (RequestStatus::Cancelled, "cancelled"),
        (RequestStatus::Pending, "pending"),
        (RequestStatus::Approved, "approved"),
        (RequestStatus::Denied, "denied"),
        (RequestStatus::Stale, "stale"),
    ] {
        assert_eq!(request_status(status), expected);
    }
}

#[test]
fn malformed_identifier_is_refused_instead_of_exporting_unstructured_text() {
    for value in [
        "",
        "private text",
        "https://private/path",
        "id\nraw",
        &"x".repeat(129),
    ] {
        assert!(identifier(value).is_err());
    }
    assert_eq!(
        identifier("sp_01m5gzx268502veq1swckhwrfg"),
        Ok("sp_01m5gzx268502veq1swckhwrfg")
    );
}

#[test]
fn typed_message_card_scope_is_visible_without_caption_or_card_details() {
    let agent = citizen("http://127.0.0.1:1", true);
    let space = agent
        .0
        .create_group_with(PRIVATE.into(), vec![], PrivacyDto::Closed)
        .unwrap();
    let mut source = agent.0.send_message(space, PRIVATE.into()).unwrap();
    source.kind = EntryKind::Message {
        text: PRIVATE.into(),
        card: Some(client_core::ItemCard {
            item_id: "it_deck_owned".into(),
            title: PRIVATE.into(),
            summary: PRIVATE.into(),
            kind_label: PRIVATE.into(),
            kind_id: "app".into(),
            version: 27,
            total_cents: Some(123),
            budget_cents: Some(456),
            line_count: 1,
            done_count: 0,
            app: None,
        }),
    };
    let output = entry(&source, &BTreeSet::new()).unwrap();
    assert_eq!(output["kind"], "message");
    assert_eq!(output["artifactId"], "it_deck_owned");
    assert_eq!(output["version"], 27);
    assert!(!output.to_string().contains(PRIVATE));
}
