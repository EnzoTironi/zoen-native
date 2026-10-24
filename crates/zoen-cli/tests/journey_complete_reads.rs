//! More than two pages of actual Agent cursors, created only through the same
//! registration, MLS invitation and Advance APIs used by the host. No provider,
//! SQL cursor seeding, FDB membership injection or quota override.
#[path = "complete_reads/arrival.rs"]
mod arrival;
mod common;
#[path = "../src/file_vault.rs"]
mod file_vault;
#[path = "complete_reads/socket.rs"]
mod socket;
use client_core::{Client, PrivacyDto};
use common::World;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

struct Online(Arc<Client>);
impl Drop for Online {
    fn drop(&mut self) {
        self.0.stop_sync();
    }
}
impl Online {
    async fn open(w: &World, who: &str) -> Self {
        let home = w.dir.join(who);
        let client = Client::open(
            home.join("zoen.sqlite").to_string_lossy().into_owned(),
            "en".into(),
        )
        .unwrap();
        assert!(client
            .unlock(Arc::new(file_vault::FileVault {
                dir: home.join("vault")
            }))
            .unwrap());
        client.start_sync(None).unwrap();
        client.wait_until_settled(20_000).await.unwrap();
        Self(client)
    }
}
fn enroll(w: &World) -> String {
    w.init("owner", "Owner");
    let agent = w.zoen("citizen", &["agent-keys"]).trim().to_owned();
    let proof = w
        .zoen("owner", &["authorize-agent", &agent])
        .trim()
        .to_owned();
    let owner = {
        let home = w.dir.join("owner");
        let client = Client::open(
            home.join("zoen.sqlite").to_string_lossy().into_owned(),
            "en".into(),
        )
        .unwrap();
        client.account().unwrap().identity_id
    };
    let definition = w.dir.join("definition.json");
    std::fs::write(&definition, br#"{"version":1,"kind":"user","persona":"Read only in this journey","rules":[],"state":{},"model_tier":"cheap"}"#).unwrap();
    w.zoen(
        "citizen",
        &[
            "agent-init",
            "--name",
            "Reader",
            "--handle",
            "paged_reader",
            "--owner",
            &owner,
            "--proof",
            &proof,
            "--definition",
            definition.to_str().unwrap(),
        ],
    );
    w.zoen(
        "owner",
        &["dm", "@paged_reader", "Initial encrypted source"],
    );
    assert!(w
        .zoen("citizen", &["read", "@owner"])
        .contains("Initial encrypted source"));
    agent
}
fn cursors(state: protocol::AgentStateRecord) -> BTreeMap<String, u64> {
    assert_eq!(state.status, "active");
    assert_eq!(state.kind, "User");
    assert_eq!(state.definition_version, 1);
    let definition: serde_json::Value = serde_json::from_slice(&state.definition).unwrap();
    assert_eq!(definition["persona"], "Read only in this journey");
    let count = state.cursors.len();
    let map: BTreeMap<_, _> = state.cursors.into_iter().collect();
    assert_eq!(map.len(), count, "duplicate cursor");
    map
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn complete_agent_state_and_cli_inspection_keep_257_existing_cursors_across_membership_changes(
) {
    let mut w = World::new("complete_agent_reads").await;
    let agent_id = enroll(&w);
    let owner = Online::open(&w, "owner").await;
    let agent = Online::open(&w, "citizen").await;
    let mut expected = BTreeMap::new();
    for n in 0..257 {
        // Both genuine MLS clients remain online: published stock is replenished
        // through the normal maintenance path, never by inserting KeyPackages.
        let space = owner
            .0
            .create_group_with(
                format!("Cursor {n:03}"),
                vec![agent_id.clone()],
                PrivacyDto::EndToEnd,
            )
            .unwrap();
        owner
            .0
            .send_message(space.clone(), format!("Unhandled source {n}"))
            .unwrap();
        arrival::settled_message(&owner, &agent, &space, &format!("Unhandled source {n}")).await;
        // This is still an exact message assertion on the real recipient, not
        // local emptiness or the sender's accepted echo.
        assert!(agent
            .0
            .timeline(space.clone())
            .unwrap()
            .iter()
            .any(|e| matches!(&e.kind, client_core::EntryKind::Message { text, .. } if text == &format!("Unhandled source {n}"))), "missing received source {n}");
        agent.0.agent_advance(space.clone(), 0).await.unwrap();
        expected.insert(space, 0);
        // Stay below the existing request/device quota. This is setup pacing,
        // not a retry, raised quota, larger request timeout or cursor fallback.
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(cursors(agent.0.agent_state().await.unwrap()), expected);
    let mut reader = socket::Reader::open(&w, "citizen", agent.0.account().unwrap()).await;
    let (first, mut stage) = reader.begin().await;
    assert_eq!(first.cursors.len(), protocol::complete_reads::PAGE_ROWS);
    assert!(!first.complete);
    let removed = expected.keys().next_back().unwrap().clone();
    let advanced = expected.keys().rev().nth(1).unwrap().clone();
    // Both writes commit AFTER the fixed FDB version and paired PG snapshot.
    agent.0.agent_advance(advanced.clone(), 9).await.unwrap();
    owner
        .0
        .remove_member(removed.clone(), agent_id.clone())
        .unwrap();
    owner.0.wait_until_settled(20_000).await.unwrap();
    agent.0.wait_until_settled(20_000).await.unwrap();
    let original = reader.finish(first, &mut stage).await;
    assert_eq!(cursors(original), expected, "mixed cursor snapshot");
    expected.insert(advanced, 9);
    expected.remove(&removed);
    assert_eq!(cursors(agent.0.agent_state().await.unwrap()), expected);
    // Filtering a departed space must not delete its cursor. Re-invitation uses
    // an actual fresh Welcome and exposes the retained original cursor again.
    owner.0.add_member(removed.clone(), agent_id).unwrap();
    owner.0.wait_until_settled(20_000).await.unwrap();
    agent.0.wait_until_settled(20_000).await.unwrap();
    expected.insert(removed, 0);
    assert_eq!(cursors(agent.0.agent_state().await.unwrap()), expected);
    drop(reader);
    drop(agent);
    drop(owner);
    // Keep the existing closed Core deadline observation in this exact failing
    // consumer's stderr. A hosted timeout can cancel artifact upload, whereas
    // World::zoen already includes failed-child stderr in the original assertion.
    // This adds no wait/retry and never changes the returned settlement error.
    w.set_client_env("ZOEN_SYNC_WAIT_DIAGNOSTICS", "1");
    // The real CLI consumer uses the completed state without running a model,
    // seeding a missing cursor, moving an existing one or writing a watermark.
    let report: serde_json::Value =
        serde_json::from_str(&w.zoen("citizen", &["agent-inspect"])).unwrap();
    let observed: BTreeMap<String, u64> =
        serde_json::from_value(report["cursors"].clone()).unwrap();
    assert_eq!(observed, expected);
    assert_eq!(report["status"], "active");
    assert_eq!(report["definitionVersion"], 1);
}
