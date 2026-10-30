//! Two independent MLS clients use the production relay and their original vaults.
//! Private removal must never become shared deletion or a fabricated read receipt.
mod common;
#[path = "../src/file_vault.rs"]
mod file_vault;

use client_core::{
    Client, CoreError, EntryKind, MessageDeletion, MessageDeletionTarget, TimelineEntry,
};
use common::World;
use std::{sync::Arc, time::Duration};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Open(Arc<Client>);
impl Drop for Open {
    fn drop(&mut self) {
        self.0.stop_sync();
    }
}
impl Open {
    fn offline(w: &World, who: &str) -> TestResult<Self> {
        let home = w.dir.join(who);
        let client = Client::open(
            home.join("zoen.sqlite").to_string_lossy().into_owned(),
            "en".into(),
        )?;
        assert!(client.unlock(Arc::new(file_vault::FileVault {
            dir: home.join("vault")
        }))?);
        Ok(Self(client))
    }
    async fn online(w: &World, who: &str) -> TestResult<Self> {
        let client = Self::offline(w, who)?;
        client.0.start_sync(None)?;
        client.0.wait_until_settled(20_000).await?;
        Ok(client)
    }
}

async fn enrolled(name: &str) -> TestResult<(World, String)> {
    let w = World::new(name).await;
    w.init("alice", "Alice");
    w.init("bob", "Bob");
    w.zoen("alice", &["dm", "@bob", "Initial encrypted message"]);
    assert!(w
        .zoen("bob", &["read", "@alice"])
        .contains("Initial encrypted message"));
    let alice = Open::offline(&w, "alice")?;
    let bob = Open::offline(&w, "bob")?;
    let bob_id = bob.0.account().ok_or("Bob")?.identity_id;
    let space = alice
        .0
        .spaces()
        .into_iter()
        .find(|s| {
            s.counterpart
                .as_ref()
                .is_some_and(|person| person.id == bob_id)
        })
        .ok_or("Alice's DM")?
        .id;
    let alice_keys = alice
        .0
        .group_keys(space.clone())
        .ok_or("Alice's MLS keys")?;
    assert_eq!(Some(alice_keys), bob.0.group_keys(space.clone()));
    assert_ne!(
        alice.0.account().ok_or("Alice")?.identity_id,
        bob.0.account().ok_or("Bob")?.identity_id
    );
    assert_ne!(
        alice.0.account().ok_or("Alice")?.device_id,
        bob.0.account().ok_or("Bob")?.device_id
    );
    Ok((w, space))
}

fn deletion(
    client: &Client,
    space: &str,
    entries: &[TimelineEntry],
    for_me: bool,
) -> TestResult<MessageDeletion> {
    let account = client.account().ok_or("account")?;
    Ok(MessageDeletion {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.into(),
        for_me: Some(for_me),
        targets: entries
            .iter()
            .map(|e| {
                Ok(MessageDeletionTarget {
                    message_id: e.id.clone(),
                    expected_revision: e.text_revision.clone().ok_or("live revision")?,
                })
            })
            .collect::<TestResult<_>>()?,
    })
}
fn text(entry: &TimelineEntry) -> &str {
    match &entry.kind {
        EntryKind::Message { text, .. } => text,
        _ => "",
    }
}
fn find(entries: &[TimelineEntry], id: &str) -> TestResult<TimelineEntry> {
    entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| format!("missing {id}").into())
}

/// Settlement only covers known local work. Require the recipient's actual view
/// within one deadline; these reads never drive Sync or create acknowledgements.
async fn received(
    client: &Client,
    space: &str,
    predicate: impl Fn(&[TimelineEntry]) -> bool,
) -> TestResult<Vec<TimelineEntry>> {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rows = client.timeline(space.into())?;
            if predicate(&rows) {
                return Ok(rows);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .map_err(|_| format!("recipient did not converge: {:?}", client.connection()))?
}

async fn read_evidence(
    client: &Client,
    space: &str,
    message: &str,
    reader: &str,
) -> TestResult<serde_json::Value> {
    let account = client.account().ok_or("receipt owner")?;
    let response = client
        .client_request(
            serde_json::json!({
                "op": "message_info", "scope": {
                    "expected_identity_id": account.identity_id,
                    "expected_device_id": account.device_id,
                    "space_id": space, "message_id": message
                }
            })
            .to_string(),
        )
        .await?;
    let info: serde_json::Value = serde_json::from_str(&response)?;
    let participant = info["participants"]
        .as_array()
        .ok_or("receipt participants")?
        .iter()
        .find(|p| p["person"]["id"] == reader)
        .ok_or("actual recipient receipt")?;
    assert_eq!(
        participant["read"], false,
        "setup unexpectedly marked the new message read"
    );
    Ok(serde_json::json!({
        "read": participant["read"],
        "reported": participant["read_reported_at_ms"],
        "observed": participant["read_observed_at_ms"]
    }))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_mixed_removal_is_local_and_survives_cold_reopen_without_source_or_receipt_writes(
) -> TestResult {
    let (w, space) = enrolled("private_deletion_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let parent = bob
        .0
        .send_message(space.clone(), "Private parent unique phrase".into())?;
    let mine = alice
        .0
        .send_message(space.clone(), "Private own leaf".into())?;
    let reply = bob.0.send_reply(
        space.clone(),
        "Keep thread reply".into(),
        parent.id.clone(),
        true,
    )?;
    received(&alice.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == parent.id && e.thread_replies == 1)
    })
    .await?;
    received(&bob.0, &space, |rows| rows.iter().any(|e| e.id == mine.id)).await?;
    alice.0.wait_until_settled(20_000).await?;
    bob.0.wait_until_settled(20_000).await?;
    let input = deletion(&alice.0, &space, &[parent.clone(), mine.clone()], true)?;
    let source = alice.0.log_events(space.clone())?;
    let peer_source = bob.0.log_events(space.clone())?;
    let relay = w.events_in(&space).await;
    let alice_id = alice.0.account().ok_or("Alice")?.identity_id;
    let marker = read_evidence(&bob.0, &space, &parent.id, &alice_id).await?;
    let unread = alice.0.space(space.clone())?.unread;
    assert!(unread > 0);
    let pending = alice.0.connection().pending;
    let ids = alice.0.delete_messages(input.clone())?;
    assert_eq!(ids, [parent.id.clone(), mine.id.clone()]);
    assert_eq!(alice.0.connection().pending, pending);
    assert_eq!(
        read_evidence(&bob.0, &space, &parent.id, &alice_id).await?,
        marker
    );
    assert_eq!(alice.0.space(space.clone())?.unread, unread - 1);
    assert_eq!(alice.0.log_events(space.clone())?, source);
    assert_eq!(bob.0.log_events(space.clone())?, peer_source);
    assert_eq!(w.events_in(&space).await, relay);
    let rows = alice.0.timeline(space.clone())?;
    let anchor = find(&rows, &parent.id)?;
    assert!(anchor.deleted && text(&anchor).is_empty() && anchor.thread_replies == 1);
    assert!(!rows.iter().any(|e| e.id == mine.id));
    assert_eq!(
        text(&find(
            &alice.0.thread(space.clone(), parent.id.clone())?,
            &reply.id
        )?),
        "Keep thread reply"
    );
    assert!(alice
        .0
        .search_messages(
            space.clone(),
            "Private parent unique phrase".into(),
            None,
            20
        )?
        .hits
        .is_empty());
    assert_eq!(
        bob.0
            .search_messages(
                space.clone(),
                "Private parent unique phrase".into(),
                None,
                20
            )?
            .hits
            .len(),
        1
    );
    assert_eq!(
        text(&find(&bob.0.timeline(space.clone())?, &parent.id)?),
        text(&parent)
    );
    drop(alice);
    let alice = Open::offline(&w, "alice")?;
    let before = alice.0.stats().events;
    assert_eq!(alice.0.delete_messages(input)?, ids);
    assert_eq!(
        alice.0.stats().events,
        before,
        "cold retry appended another private event"
    );
    assert_eq!(alice.0.connection().pending, pending);
    assert_eq!(alice.0.log_events(space.clone())?, source);
    assert_eq!(
        read_evidence(&bob.0, &space, &parent.id, &alice_id).await?,
        marker
    );
    assert_eq!(alice.0.space(space.clone())?.unread, unread - 1);
    assert!(!alice
        .0
        .timeline(space.clone())?
        .iter()
        .any(|e| e.id == mine.id));
    assert!(
        find(
            &alice.0.thread(space.clone(), parent.id.clone())?,
            &parent.id
        )?
        .deleted
    );
    assert_eq!(w.events_in(&space).await, relay);
    assert!(alice.0.verify_all().iter().all(|log| log.valid));
    assert!(bob.0.verify_all().iter().all(|log| log.valid));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reviewed_shared_deletion_rejects_stale_or_foreign_sets_then_recovers_offline_exactly_once(
) -> TestResult {
    let (mut w, space) = enrolled("shared_deletion_peers").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let parent = alice
        .0
        .send_message(space.clone(), "Shared parent".into())?;
    let leaf = alice
        .0
        .send_message(space.clone(), "Shared leaf before edit".into())?;
    received(&bob.0, &space, |rows| rows.iter().any(|e| e.id == leaf.id)).await?;
    let reply = bob.0.send_reply(
        space.clone(),
        "Unselected reply survives".into(),
        parent.id.clone(),
        true,
    )?;
    received(&alice.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == parent.id && e.thread_replies == 1)
    })
    .await?;
    let stale = deletion(&alice.0, &space, &[parent.clone(), leaf.clone()], false)?;
    let foreign = deletion(&bob.0, &space, &[parent.clone(), leaf.clone()], false)?;
    assert!(matches!(
        bob.0.delete_messages(foreign),
        Err(CoreError::Forbidden { .. })
    ));
    let edit_request = serde_json::json!({
        "op": "edit_message",
        "expected_identity_id": stale.expected_identity_id,
        "expected_device_id": stale.expected_device_id,
        "space_id": space,
        "message_id": leaf.id,
        "expected_revision": leaf.text_revision,
        "text": "Shared leaf revised"
    })
    .to_string();
    alice.0.client_request(edit_request.clone()).await?;
    let rows = received(&bob.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == leaf.id && text(e) == "Shared leaf revised")
    })
    .await?;
    let revised = find(&rows, &leaf.id)?;
    assert_ne!(revised.text_revision, leaf.text_revision);
    alice.0.wait_until_settled(20_000).await?;
    bob.0.wait_until_settled(20_000).await?;
    let source = alice.0.log_events(space.clone())?;
    let before = w.events_in(&space).await;
    // The same app command reconciles an uncertain acknowledgement without
    // publishing another edit; the following exact source checks cover both retries.
    alice.0.client_request(edit_request.clone()).await?;
    alice.0.wait_until_settled(20_000).await?;
    assert!(matches!(
        alice.0.delete_messages(stale),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(alice.0.log_events(space.clone())?, source);
    assert_eq!(alice.0.connection().pending, 0);
    assert_eq!(w.events_in(&space).await, before);
    let input = deletion(&alice.0, &space, &[parent.clone(), revised], false)?;
    alice.0.stop_sync();
    bob.0.stop_sync();
    w.stop_relay();
    let ids = alice.0.delete_messages(input.clone())?;
    assert_eq!(alice.0.connection().pending, 2);
    assert_eq!(alice.0.delete_messages(input.clone())?, ids);
    assert_eq!(
        alice.0.connection().pending,
        2,
        "retry duplicated pending tombstones"
    );
    drop(alice);
    let alice = Open::offline(&w, "alice")?;
    assert_eq!(alice.0.connection().pending, 2);
    assert_eq!(alice.0.delete_messages(input.clone())?, ids);
    assert!(find(&alice.0.timeline(space.clone())?, &leaf.id)?.deleted);
    drop(bob);
    w.start_relay();
    let bob = Open::online(&w, "bob").await?;
    alice.0.start_sync(None)?;
    alice.0.wait_until_settled(20_000).await?;
    received(&bob.0, &space, |rows| {
        ids.iter().all(|id| {
            rows.iter()
                .any(|e| &e.id == id && e.deleted && text(e).is_empty())
        })
    })
    .await?;
    let thread = bob.0.thread(space.clone(), parent.id.clone())?;
    assert!(find(&thread, &parent.id)?.deleted);
    assert_eq!(
        text(&find(&thread, &reply.id)?),
        "Unselected reply survives"
    );
    let after = w.events_in(&space).await;
    assert_eq!(
        after.len(),
        before.len() + 2,
        "unexpected shared publication count"
    );
    assert_eq!(
        &after[..before.len()],
        before.as_slice(),
        "source prefix changed"
    );
    assert_eq!(alice.0.connection().pending, 0);
    drop(alice);
    let alice = Open::online(&w, "alice").await?;
    assert!(
        alice.0.client_request(edit_request).await.is_err(),
        "a retired edit resurrected the deleted message"
    );
    assert_eq!(alice.0.delete_messages(input)?, ids);
    alice.0.wait_until_settled(20_000).await?;
    assert_eq!(
        w.events_in(&space).await,
        after,
        "confirmed cold retry published again"
    );
    assert!(alice.0.verify_all().iter().all(|log| log.valid));
    assert!(bob.0.verify_all().iter().all(|log| log.valid));
    Ok(())
}
