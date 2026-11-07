//! Exact metadata through the app command, real peer receipt and cold replay.
use super::*;
use serde_json::{json, Value};

async fn info(client: &Client, space: &str, message: &str) -> TestResult<Value> {
    let account = client.account().ok_or("metadata account")?;
    Ok(serde_json::from_str(
        &client
            .client_request(
                json!({
                    "op":"message_info", "scope": {
                        "expected_identity_id": account.identity_id,
                        "expected_device_id": account.device_id,
                        "space_id":space, "message_id":message
                    }
                })
                .to_string(),
            )
            .await?,
    )?)
}
fn participant(value: &Value) -> TestResult<&Value> {
    let people = value["participants"]
        .as_array()
        .ok_or("metadata participants")?;
    assert_eq!(people.len(), 1, "exact private DM audience");
    people.first().ok_or_else(|| "missing peer metadata".into())
}
fn timestamp(value: &Value) -> TestResult<i64> {
    if let Some(at) = value.as_i64() {
        return Ok(at);
    }
    Ok(value.as_str().ok_or("metadata timestamp")?.parse()?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_edit_and_peer_read_dates_survive_cold_replay_without_inventing_times(
) -> TestResult {
    let (w, space) = enrolled("message_info_dates").await?;
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    let peer = bob.0.account().ok_or("reader account")?.identity_id;
    let source = alice
        .0
        .send_message(space.clone(), "Metadata original".into())?;
    received(&bob.0, &space, |rows| {
        rows.iter().any(|e| e.id == source.id)
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    let original = info(&alice.0, &space, &source.id).await?;
    assert_eq!(timestamp(&original["sent_at_ms"])?, source.at_ms);
    let before = participant(&original)?;
    assert_eq!(before["person"]["id"], peer);
    assert_eq!(before["read"], false);
    assert!(before["read_reported_at_ms"].is_null());
    assert!(before["read_observed_at_ms"].is_null());

    alice.0.edit_message(
        space.clone(),
        source.id.clone(),
        "Metadata corrected".into(),
    )?;
    received(&bob.0, &space, |rows| {
        rows.iter()
            .any(|e| e.id == source.id && text(e) == "Metadata corrected")
    })
    .await?;
    alice.0.wait_until_settled(20_000).await?;
    let current = find(&alice.0.timeline(space.clone())?, &source.id)?;
    assert!(current.text_revision.is_some());
    let edited_at = alice
        .0
        .log_events(space.clone())?
        .into_iter()
        .find(|e| {
            e.seq > source.seq && e.author.id == source.author.id && e.label == "Message edited"
        })
        .ok_or("exact signed edit")?
        .at_ms;
    let edited = info(&alice.0, &space, &source.id).await?;
    assert_eq!(timestamp(&edited["sent_at_ms"])?, source.at_ms);
    assert_eq!(edited["edited"], true);
    assert_eq!(timestamp(&edited["edited_at_ms"])?, edited_at);

    bob.0.mark_read(space.clone())?;
    let received_info = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let value = info(&alice.0, &space, &source.id).await?;
            if !participant(&value)?["read_reported_at_ms"].is_null() {
                return Ok::<_, Box<dyn std::error::Error>>(value);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    let report = participant(&received_info)?.clone();
    assert_eq!(report["read"], true);
    assert_eq!(report["delivered"], true);
    let recipient = info(&bob.0, &space, &source.id).await?;
    assert_eq!(recipient["read_details"], "not_sender");
    assert_eq!(recipient["participants"], json!([]));

    // Inspect the fixture's local receipt only after its real Core has closed.
    // A second SQLite connection must not bypass native exclusive ownership.
    drop(bob);
    let seq = format!("{:020}", current.seq);
    let peer_reported: i64 = rusqlite::Connection::open_with_flags(w.dir.join("bob/zoen.sqlite"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?.query_row(
        "SELECT reported_at_ms FROM peer_read_times WHERE space=?1 AND identity=?2 AND after_seq<?3 AND through_seq>=?3 ORDER BY observed_at_ms LIMIT 1",
        rusqlite::params![space, peer, seq], |row| row.get(0),
    )?;
    assert_eq!(timestamp(&report["read_reported_at_ms"])?, peer_reported);
    assert!(!report["read_observed_at_ms"].is_null());

    drop(alice);
    let alice = Open::offline(&w, "alice")?;
    let restored = info(&alice.0, &space, &source.id).await?;
    assert_eq!(
        participant(&restored)?,
        &report,
        "cold read preserves the original observation, not the reopen clock"
    );
    assert_eq!(timestamp(&restored["edited_at_ms"])?, edited_at);
    drop(alice);
    let alice = Open::online(&w, "alice").await?;
    let bob = Open::online(&w, "bob").await?;
    bob.0.wait_until_settled(20_000).await?;
    alice.0.wait_until_settled(20_000).await?;
    assert_eq!(
        participant(&info(&alice.0, &space, &source.id).await?)?,
        &report,
        "replayed peer receipt cannot invent a newer read date"
    );
    Ok(())
}
