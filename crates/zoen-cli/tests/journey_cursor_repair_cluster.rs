//! Real PG/FDB/NATS + actual Client/Net. One final live Event is lost at a test-owned
//! relay-B -> client wire boundary. No synthetic relay, Pong, refresh or next content.
//! Production's 60s cursor-repair cadence is exercised unchanged.
mod common;
#[path = "common/final_event_proxy.rs"]
mod final_event_proxy;

use client_core::{AccountDto, Client, Delivery, EntryKind, PrivacyDto, SecretVault};
use common::World;
use final_event_proxy::{FinalEventProxy, Kind, Record};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

struct Device {
    core: Arc<Client>,
    account: AccountDto,
    path: PathBuf,
}
impl Device {
    fn new(w: &World, name: &str, handle: &str, relay: String) -> Self {
        let path = w.dir.join(format!("{handle}.sqlite"));
        let core = Client::open(path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
        let account = core
            .create_account(
                name.into(),
                handle.into(),
                relay,
                Arc::new(Vault::default()),
            )
            .unwrap();
        Self {
            core,
            account,
            path,
        }
    }
    async fn start(&self) {
        self.core.start_sync(None).unwrap();
        let status = self.core.wait_until_idle(15_000).await;
        assert_eq!(status.state, "online", "{status:?}");
        assert!(status.synced, "{status:?}");
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.core.stop_sync();
    }
}

fn target_count(device: &Device, space: &str, id: &str) -> usize {
    device
        .core
        .timeline(space.into())
        .unwrap_or_default()
        .iter()
        .filter(|e| e.id == id)
        .count()
}

async fn wait_entry(device: &Device, space: &str, id: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if target_count(device, space, id) == 1 {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "local projection missing {id}; {:?}",
            device.core.connection()
        );
        // timeline only reads the local engine; it does not send Sync or open the chat.
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Group creation starts real profile-key exchange as well as content delivery.
/// Settle those durable writes and the seed receipt BEFORE choosing the final tail.
/// No quiet-window sleep, fake receipt, explicit Sync or production switch is used.
async fn settled_setup_prefix(
    w: &World,
    ana: &Device,
    bruno: &Device,
    proxy: &FinalEventProxy,
    space: &str,
    seed_id: &str,
    evidence: &std::path::Path,
) -> Vec<protocol::Sequenced> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let canonical = w.events_in(space).await;
        let directed_share = |from: &str, to: &str| {
            canonical.iter().any(|event| {
                event.env.author() == from
                    && event.plain_event().is_some_and(|plain| {
                        matches!(plain.body,
                    types::EventBody::ProfileKeyShared { version, shares }
                        if version > 0 && shares.iter().any(|share| share.to == to))
                    })
            })
        };
        let shares_complete = directed_share(&ana.account.identity_id, &bruno.account.identity_id)
            && directed_share(&bruno.account.identity_id, &ana.account.identity_id);
        let sender_seed = ana
            .core
            .timeline(space.into())
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == seed_id);
        let seed_received = sender_seed
            .as_ref()
            .is_some_and(|entry| matches!(entry.delivery, Delivery::Delivered | Delivery::Read));
        let records = proxy.snapshot();
        let receipt_sent = sender_seed.as_ref().is_some_and(|entry| {
            records.iter().any(|record| {
                matches!(&record.kind, Kind::Delivered { space: receipt_space, seq }
                if receipt_space == space && *seq >= entry.seq)
            })
        });
        let canonical_seen_on_receiver_wire = canonical.iter().all(|event| records.iter().any(|record| {
            matches!(&record.kind, Kind::Event { space: event_space, id, seq, hash, dropped: false }
                if event_space == space && id == event.env.client_id() && *seq == event.seq && hash == &event.hash)
        }));
        // SpaceSummary.event_count is the accepted SpaceLog length / next sequence,
        // not the visible message count or optimistic outbox length (engine.rs).
        let ana_next = ana.core.space(space.into()).unwrap().event_count;
        let bruno_next = bruno.core.space(space.into()).unwrap().event_count;
        let canonical_next = canonical.last().map_or(0, |event| event.seq + 1);
        if shares_complete
            && seed_received
            && receipt_sent
            && canonical_seen_on_receiver_wire
            && ana_next == canonical_next
            && bruno_next == canonical_next
        {
            // These public methods only observe the actual clients' full idle contract,
            // including pending profile work and outbox. They do not request a refresh.
            let ana_settled = ana.core.wait_until_settled(500).await.is_ok();
            let bruno_settled = bruno.core.wait_until_settled(500).await.is_ok();
            if ana_settled && bruno_settled && w.events_in(space).await == canonical {
                let plain = canonical
                    .iter()
                    .map(|event| event.plain_event().unwrap())
                    .collect::<Vec<_>>();
                event_log::SpaceLog::from_events(space.to_owned(), plain.clone()).unwrap();
                std::fs::write(
                    evidence.join("settled-setup-fdb-prefix.json"),
                    serde_json::to_vec_pretty(&plain).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    evidence.join("settled-setup-witness.json"),
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "seed_id": seed_id,
                        "seed_seq": sender_seed.as_ref().unwrap().seq,
                        "seed_delivery": format!("{:?}", sender_seed.as_ref().unwrap().delivery),
                        "real_receiver_delivered_frame_observed": true,
                        "both_canonical_profile_key_share_directions_complete": true,
                        "canonical_next_seq": canonical_next,
                        "sender_accepted_next_seq": ana_next,
                        "receiver_accepted_next_seq": bruno_next,
                        "all_prefix_events_seen_on_receiver_wire": true,
                        "both_actual_clients_settled": true
                    }))
                    .unwrap(),
                )
                .unwrap();
                return canonical;
            }
        }
        assert!(tokio::time::Instant::now() < deadline,
            "initial profile shares, exact accepted cursors and seed delivery must settle before fault; shares_complete={shares_complete}, seed_received={seed_received}, receipt_sent={receipt_sent}, canonical_next={canonical_next}, ana_next={ana_next}, bruno_next={bruno_next}; canonical={canonical:#?}; wire={records:#?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn assert_same_healthy_socket(records: &[Record], connection: u64) {
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r.kind, Kind::Open))
            .count(),
        1,
        "receiver reconnected: {records:#?}"
    );
    assert!(
        records.iter().all(|r| r.connection == connection),
        "receiver changed socket"
    );
    for record in records {
        match &record.kind {
            Kind::Close => panic!(
                "receiver transport closed at {}ms: {records:#?}",
                record.elapsed_ms
            ),
            Kind::TransportError(message) | Kind::RelayError(message) => panic!(
                "receiver transport failed at {}ms: {message}; {records:#?}",
                record.elapsed_ms
            ),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn final_wire_loss_recovers_with_real_cluster_and_same_healthy_client_socket() {
    // The caller supplies an output directory outside World, which removes its owned
    // fixtures on success. No credentials or pre-existing user data enter this test.
    let evidence = std::env::var_os("ZOEN_CURSOR_REPAIR_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("zoen-cursor-repair-{}", types::new_id("proof")))
        });
    assert!(
        !evidence.exists(),
        "use a fresh evidence directory for each qualification"
    );
    std::fs::create_dir_all(&evidence).unwrap();
    // This fixture parses structured startup evidence below. Own the format and
    // INFO filter for both spawned relays instead of inheriting a developer's
    // shell or CI logging defaults (telemetry::init otherwise emits text).
    let nats = std::env::var("ZOEN_NATS_URL").expect("set ZOEN_NATS_URL");
    let mut w = World::new_with(
        "cursor_final_wire_loss",
        Some(nats),
        &[
            ("LOG_FORMAT", "json"),
            ("RUST_LOG", "info,sqlx=warn,tower_http=warn"),
        ],
    )
    .await;
    let relay_b = w.start_node();
    let proxy = FinalEventProxy::new(relay_b, evidence.clone()).await;
    std::fs::write(
        evidence.join("isolation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "boundary": "one post-NATS relay-B-to-client Event frame",
            "relay_a_port": w.port, "relay_b_port": relay_b, "receiver_proxy": proxy.url,
            "owned_postgres_database_and_fdb_cell": w.cell,
            "production_repair_interval_seconds": 60,
            "existing_services": ["ZOEN_TEST_PG", "FDB_CLUSTER_FILE", "ZOEN_NATS_URL"],
            "fresh_synthetic_accounts": true,
            "credentials_copied": false
        }))
        .unwrap(),
    )
    .unwrap();
    let ana = Device::new(&w, "Ana", "ana", w.relay_url());
    ana.start().await;
    let bruno = Device::new(&w, "Bruno", "bruno", proxy.url.clone());
    bruno.start().await;
    // Closed plaintext is intentional: isolate the durable cursor-repair contract.
    // This test makes no MLS/E2EE delivery claim.
    let space = ana
        .core
        .create_group_with(
            "Final live frame".into(),
            vec![bruno.account.identity_id.clone()],
            PrivacyDto::Closed,
        )
        .unwrap();
    let seed = ana
        .core
        .send_message(space.clone(), "known prefix before loss".into())
        .unwrap();
    wait_entry(&bruno, &space, &seed.id, Duration::from_secs(15)).await;
    let canonical_prefix =
        settled_setup_prefix(&w, &ana, &bruno, &proxy, &space, &seed.id, &evidence).await;
    let prepared = proxy
        .until(
            Duration::from_secs(10),
            "all setup Sync requests must complete",
            |records| {
                let submitted = records
                    .iter()
                    .filter(|r| matches!(r.kind, Kind::Sync { .. }))
                    .count();
                let completed = records
                    .iter()
                    .filter(|r| matches!(r.kind, Kind::SyncDone))
                    .count();
                submitted > 0 && submitted == completed
            },
        )
        .await;
    let connection = prepared[0].connection;
    assert_same_healthy_socket(&prepared, connection);
    let initial_done = prepared
        .iter()
        .find(|r| matches!(r.kind, Kind::SyncDone))
        .unwrap()
        .at;
    assert!(
        initial_done.elapsed() < Duration::from_secs(35),
        "fixture setup consumed the production repair window"
    );
    assert_eq!(
        prepared
            .iter()
            .filter(|r| matches!(r.kind, Kind::Sync { all: true, .. }))
            .count(),
        1,
        "a periodic repair already ran during setup"
    );
    let next_seq = canonical_prefix.last().unwrap().seq + 1;
    let text = format!(
        "final synthetic event without a successor {}",
        types::new_id("loss")
    );
    let checkpoint = proxy.arm(&space, &ana.account.identity_id, &text);
    let sent = ana.core.send_message(space.clone(), text.clone()).unwrap();
    // Deliberately do not write another event, open a chat, refresh, reconnect or
    // change membership after this line. Natural maintenance/Ping/Pong stays live.
    let dropped_records = proxy
        .until(
            Duration::from_secs(15),
            "exact final event must traverse real NATS to relay B before the wire fault",
            |records| {
                records[checkpoint..].iter().any(
                    |r| matches!(&r.kind, Kind::Event { id, dropped: true, .. } if id == &sent.id),
                )
            },
        )
        .await;
    let dropped = dropped_records[checkpoint..]
        .iter()
        .find(|r| matches!(&r.kind, Kind::Event { id, dropped: true, .. } if id == &sent.id))
        .unwrap();
    assert!(dropped.at.duration_since(initial_done) < Duration::from_secs(35), "fixture must lose its final frame early enough to witness the next natural 25s Ping/Pong before 60s repair");
    let (seq, hash) = match &dropped.kind {
        Kind::Event { seq, hash, .. } => (*seq, hash.clone()),
        _ => unreachable!(),
    };
    assert_eq!(seq, next_seq, "target must be the sole final successor");
    assert_eq!(
        target_count(&bruno, &space, &sent.id),
        0,
        "fault did not hide the live event"
    );
    assert_same_healthy_socket(&dropped_records, connection);
    let canonical = w.events_in(&space).await; // direct FDB CLI read, never asks receiver to Sync
    assert_eq!(
        canonical.len(),
        canonical_prefix.len() + 1,
        "no later durable content may mask the missing-tail condition"
    );
    assert_eq!(canonical.last().unwrap().env.client_id(), sent.id);
    assert_eq!(canonical.last().unwrap().hash, hash);
    assert_eq!(
        canonical
            .iter()
            .filter(|e| e.env.client_id() == sent.id)
            .count(),
        1
    );
    let canonical_plain = canonical
        .iter()
        .map(|e| e.plain_event().unwrap())
        .collect::<Vec<_>>();
    event_log::SpaceLog::from_events(space.clone(), canonical_plain.clone()).unwrap();
    let durable_target = canonical_plain.last().unwrap();
    assert_eq!(durable_target.author, ana.account.identity_id);
    assert!(
        matches!(&durable_target.body, types::EventBody::MessagePosted { text: actual, .. } if actual == &text)
    );
    let canonical_bytes = serde_json::to_vec_pretty(&canonical_plain).unwrap();
    std::fs::write(evidence.join("canonical-fdb-events.json"), canonical_bytes).unwrap();

    let heartbeat = proxy.until(Duration::from_secs(30), "a real Net Ping and server Pong must cross the same socket while target is still missing", |records| {
        let Some(ping) = records.iter().find(|r| r.at > dropped.at && matches!(r.kind, Kind::Ping)) else { return false };
        records.iter().any(|r| r.at > ping.at && matches!(r.kind, Kind::Pong))
    }).await;
    assert_same_healthy_socket(&heartbeat, connection);
    assert_eq!(
        target_count(&bruno, &space, &sent.id),
        0,
        "target arrived before the heartbeat/missing-tail witness"
    );
    assert!(
        !heartbeat[checkpoint..]
            .iter()
            .any(|r| matches!(r.kind, Kind::Sync { .. })),
        "setup/fault must precede the scheduled repair"
    );

    let recovered = proxy.until(Duration::from_secs(75), "healthy Pongs must not hide the completely lost final live frame: autonomous full Sync and replay required", |records| {
        let Some(replay_index) = records.iter().position(|r| matches!(&r.kind, Kind::Event { id, dropped: false, .. } if id == &sent.id)) else { return false };
        records[replay_index + 1..].iter().any(|r| matches!(r.kind, Kind::SyncDone))
    }).await;
    wait_entry(&bruno, &space, &sent.id, Duration::from_secs(5)).await;
    assert_same_healthy_socket(&recovered, connection);
    let after = &recovered[checkpoint..];
    let repairs = after
        .iter()
        .filter(|r| matches!(r.kind, Kind::Sync { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        repairs.len(),
        1,
        "one autonomous full repair, no partial/explicit refresh"
    );
    let (cursors, all) = match &repairs[0].kind {
        Kind::Sync { cursors, all } => (cursors, *all),
        _ => unreachable!(),
    };
    assert!(all);
    assert_eq!(
        cursors
            .iter()
            .filter(|c| c.space == space)
            .map(|c| c.next_seq)
            .collect::<Vec<_>>(),
        vec![seq],
        "request cursor must name the exact first missing sequence"
    );
    assert!(
        repairs[0].at.duration_since(initial_done) >= Duration::from_millis(59_500),
        "production 60s interval was shortened"
    );
    assert!(
        after
            .iter()
            .any(|r| r.at > dropped.at && r.at < repairs[0].at && matches!(r.kind, Kind::Pong)),
        "healthy heartbeat must precede repair"
    );
    for record in after {
        if let Kind::Joined(joined) = &record.kind {
            panic!("membership must not trigger recovery: {joined}");
        }
        assert!(
            !matches!(record.kind, Kind::Publish),
            "receiver content must not trigger recovery"
        );
    }
    let target_frames = after
        .iter()
        .filter_map(|r| match &r.kind {
            Kind::Event {
                space: event_space,
                id,
                seq: event_seq,
                hash: event_hash,
                dropped,
            } => {
                assert_eq!(
                    event_space, &space,
                    "no unrelated content after the fault was armed"
                );
                assert_eq!(id, &sent.id, "no later content may create a gap");
                assert_eq!(*event_seq, seq);
                assert_eq!(event_hash, &hash);
                Some(*dropped)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        target_frames,
        vec![true, false],
        "drop one exact frame, forward its exact canonical replay once"
    );
    assert_eq!(target_count(&bruno, &space, &sent.id), 1);
    let timeline = bruno.core.timeline(space.clone()).unwrap();
    let received = timeline.iter().find(|entry| entry.id == sent.id).unwrap();
    assert_eq!(received.seq, seq);
    assert_eq!(received.author.id, ana.account.identity_id);
    assert_eq!(received.at_ms, sent.at_ms);
    assert!(
        matches!(&received.kind, EntryKind::Message { text: actual, card: None } if actual == &text)
    );
    assert!(!received.deleted && !received.edited && !received.forwarded);
    assert_eq!(bruno.core.connection().state, "online");
    assert!(bruno.core.connection().synced);
    let relay_logs = w.relay_log_text();
    std::fs::write(evidence.join("owned-relays.log"), &relay_logs).unwrap();
    let nats_nodes = relay_logs
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|entry| {
            entry["target"] == "zoen_relay"
                && entry["fields"]["message"] == "fan-out ready"
                && entry["fields"]["bus"] == "nats"
        })
        .filter_map(|entry| entry["fields"]["node"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    assert_eq!(
        nats_nodes.len(),
        2,
        "both distinct real relays must use NATS"
    );
    std::fs::write(
        evidence.join("receiver-timeline.json"),
        serde_json::to_vec_pretty(&timeline).unwrap(),
    )
    .unwrap();
    let terminal = proxy.snapshot();
    assert_same_healthy_socket(&terminal, connection);
    assert_eq!(
        terminal[checkpoint..]
            .iter()
            .filter(|r| matches!(r.kind, Kind::Sync { .. }))
            .count(),
        1
    );
    assert_eq!(
        terminal[checkpoint..]
            .iter()
            .filter(|r| matches!(r.kind, Kind::Event { .. }))
            .count(),
        2
    );
    assert!(!terminal[checkpoint..]
        .iter()
        .any(|r| matches!(r.kind, Kind::Joined(_) | Kind::Publish)));
    proxy.save();

    // Stop only after same-socket success was proved, then independently read SQLite.
    // No second client is connected and no credentials are copied or reopened.
    let receiver_path = bruno.path.clone();
    drop(bruno);
    let local = store::Store::open(receiver_path.to_str().unwrap()).unwrap();
    let local_events = local.events(&space).unwrap();
    assert_eq!(
        local_events, canonical_plain,
        "accepted local log must equal the complete verified FDB chain"
    );
    event_log::SpaceLog::from_events(space.clone(), local_events.clone()).unwrap();
    assert_eq!(
        local_events
            .iter()
            .filter(|event| event.client_id == sent.id)
            .count(),
        1
    );
    assert_eq!(
        local
            .cursors()
            .unwrap()
            .iter()
            .filter(|(id, _)| id == &space)
            .map(|(_, next)| *next)
            .collect::<Vec<_>>(),
        vec![seq + 1],
        "persisted cursor must advance once, only after exact ingest"
    );
    std::fs::write(evidence.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "status": "PASS", "boundary": "post-broker WS Event loss", "connection": connection,
        "target_id": sent.id, "space": space, "target_seq": seq, "target_hash": hash,
        "request_next_seq": seq, "persisted_next_seq": seq + 1,
        "dropped_target_frames": 1, "forwarded_target_replays": 1, "local_target_rows": 1,
        "repair_ms_after_initial_sync_done": repairs[0].at.duration_since(initial_done).as_millis(),
        "recovered_without_next_content_join_refresh_or_reconnect": true,
        "mls_qualified": false, "nats_broker_loss_qualified": false
    })).unwrap()).unwrap();
    println!("cursor-repair cluster proof: {}", evidence.display());
}
