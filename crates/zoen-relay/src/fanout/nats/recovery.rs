//! Joined recovery at the application queue, BEFORE any NATS delivery.
//! The existing PR150 journey separately drops a post-broker WS frame.
use super::*;
use crate::log::LogStore;
use client_core::{Delivery, EntryKind, PrivacyDto};
use protocol::{SealedKind, Sequenced};

mod assertions;
mod fixture;
// Reuse the existing real-socket observer. Its downstream-loss selector is never armed.
#[path = "../../../../zoen-cli/tests/common/final_event_proxy.rs"]
mod wire;
use fixture::{count, until, Device, World};
use wire::{FinalEventProxy, Kind, Record};

#[test]
#[ignore = "requires explicitly owned PostgreSQL/FDB/NATS and fresh evidence directory"]
fn actual_publisher_queue_refusal_recovers_final_mls_event_on_the_same_native_socket() {
    // SAFETY: this opt-in test alone owns the FDB network; the private runner
    // selects only this test. Runtime/tasks drop before its network guard.
    let _network = unsafe { foundationdb::boot() };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(run());
}

async fn run() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("ZOEN_PUBLISHER_RECOVERY_EVIDENCE_DIR")
            .expect("explicit new synthetic evidence directory"),
    );
    assert!(
        !directory.exists(),
        "preserve earlier proof; choose a new path"
    );
    std::fs::create_dir_all(&directory).unwrap();
    let world = World::new(&directory).await;
    let proxy = FinalEventProxy::new(world.b.port, directory.clone()).await;
    let sender = Device::new(&directory, "sender", world.a.url()).await;
    let receiver = Device::new(&directory, "receiver", proxy.url.clone()).await;
    let space = sender
        .core
        .create_group_with(
            "Final encrypted publisher refusal".into(),
            vec![receiver.account.identity_id.clone()],
            PrivacyDto::EndToEnd,
        )
        .unwrap();
    let seed = sender
        .core
        .send_message(space.clone(), "settled prefix".into())
        .unwrap();
    until("the real MLS peer must read the prefix", || {
        count(&receiver, &space, &seed.id) == 1
    })
    .await;
    let prefix = settled_prefix(&world, &sender, &receiver, &proxy, &space, &seed.id).await;
    let records = proxy.snapshot();
    let connection = records[0].connection;
    assertions::healthy_socket(&records, connection);
    let initial_done = records
        .iter()
        .find(|r| matches!(r.kind, Kind::SyncDone))
        .unwrap()
        .at;
    assert!(
        initial_done.elapsed() < Duration::from_secs(35),
        "setup consumed the unmodified repair interval"
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r.kind, Kind::Sync { all: true, .. }))
            .count(),
        1
    );

    let publisher = world
        .a
        .state
        .fanout
        .bus
        .publisher_probe()
        .expect("actual NatsBus publisher");
    until(
        "setup publisher must fully drain its real queue and active credits",
        || publisher.capacity() == OUTBOUND_QUEUE && publisher.retained_bytes() == 0,
    )
    .await;
    publisher.pause(&space, &sender.account.identity_id);
    let pressure_to = ["ff".repeat(32)]; // No session or durable account exists for this identity.
    let pressure = ServerFrame::Pong {
        server_time_ms: None,
    };
    world.a.state.fanout.send(&pressure_to, &pressure, None);
    publisher.held().await;
    for _ in 0..OUTBOUND_QUEUE {
        world.a.state.fanout.send(&pressure_to, &pressure, None);
    }
    assert_eq!(
        publisher.capacity(),
        0,
        "real application count queue is full"
    );
    assert!(publisher.retained_bytes() > 0 && publisher.retained_bytes() <= outbound::MAX_BYTES);
    let published_before = publisher.published();
    let dropped_before = publisher.dropped();
    assert!(publisher.refused_event().is_none());
    let checkpoint = proxy.snapshot().len();
    let text = "One final encrypted intent; no successor, retry send, refresh or reconnect";
    // The ONLY final user send. Pongs used for pressure never enter the durable log.
    let sent = sender
        .core
        .send_message(space.clone(), text.into())
        .unwrap();
    until(
        "actual NatsBus admission must refuse the selected canonical event",
        || publisher.refused_event().is_some(),
    )
    .await;
    let refused = publisher.refused_event().unwrap();
    assert_eq!(refused.env.client_id(), sent.id);
    assert_eq!(refused.env.sealed_kind(), Some(SealedKind::Application));
    assert!(!refused.env.sealed_data().unwrap().1.is_empty());
    refused.env.verify().unwrap();
    assert_eq!(publisher.dropped(), dropped_before + 1);
    assert_eq!(
        publisher.published(),
        published_before,
        "paused publisher never handed this event to NATS"
    );
    assert_eq!(publisher.capacity(), 0);
    assert_eq!(count(&receiver, &space, &sent.id), 0);
    let canonical = world.events(&space).await;
    assert_eq!(&canonical[..prefix.len()], prefix);
    assert_eq!(
        canonical.len(),
        prefix.len() + 1,
        "no successor durable event"
    );
    assert_eq!(
        canonical.last(),
        Some(&refused),
        "CID, signature, ciphertext, seq/hash are the actual stored frame"
    );
    assertions::chain(&space, &canonical);
    std::fs::write(directory.join("canonical-envelope.bin"), refused.encode()).unwrap();

    let heartbeat = proxy
        .until(
            Duration::from_secs(30),
            "real Ping/Pong while the final event remains absent",
            |rs| {
                let Some(ping) = rs[checkpoint..]
                    .iter()
                    .find(|r| matches!(r.kind, Kind::Ping))
                else {
                    return false;
                };
                rs[checkpoint..]
                    .iter()
                    .any(|r| r.at > ping.at && matches!(r.kind, Kind::Pong))
            },
        )
        .await;
    assertions::healthy_socket(&heartbeat, connection);
    assert_eq!(count(&receiver, &space, &sent.id), 0);
    assert!(!heartbeat[checkpoint..]
        .iter()
        .any(|r| matches!(r.kind, Kind::Event { .. } | Kind::Sync { .. })));
    assert!(
        !heartbeat[checkpoint..]
            .iter()
            .any(|record| matches!(&record.kind,
            Kind::Delivered { space: delivered_space, seq }
            if delivered_space == &space && *seq >= refused.seq)),
        "native peer cannot acknowledge the final event while it remains absent"
    );
    let recovered = proxy
        .until(
            Duration::from_secs(75),
            "normal periodic full Sync must recover the queue-refused final event",
            |rs| {
                let Some(i) = rs
                    .iter()
                    .position(|r| matches!(&r.kind, Kind::Event { id, .. } if id == &sent.id))
                else {
                    return false;
                };
                rs[i + 1..].iter().any(|r| matches!(r.kind, Kind::SyncDone))
            },
        )
        .await;
    until(
        "actual native MLS ingest must project the final plaintext",
        || count(&receiver, &space, &sent.id) == 1,
    )
    .await;
    assertions::recovered_wire(
        &recovered,
        checkpoint,
        connection,
        initial_done,
        &space,
        &refused,
    );
    let recovered_at_ms = recovered
        .iter()
        .find_map(|record| match &record.kind {
            Kind::Event { id, .. } if id == &sent.id => Some(record.elapsed_ms),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        world.events(&space).await,
        canonical,
        "no new durable operation triggered recovery"
    );
    assert_eq!(
        publisher.published(),
        published_before,
        "recovery is independent of resuming the publisher"
    );
    assert_eq!(publisher.capacity(), 0);
    assert_eq!(publisher.dropped(), dropped_before + 1);
    let timeline = receiver.core.timeline(space.clone()).unwrap();
    let received = timeline.iter().find(|e| e.id == sent.id).unwrap();
    assert_eq!(received.seq, refused.seq);
    assert!(
        matches!(&received.kind, EntryKind::Message { text: body, card: None } if body == text)
    );
    assert_eq!(receiver.core.connection().state, "online");
    proxy.save(); // Exact pre-Stop socket witness, separate from intentional cleanup.
    let path = receiver.path.clone();
    let vault = receiver.vault.clone();
    drop(receiver);
    assertions::cold(&path, vault, &space, &sent.id, &timeline, &refused);
    drop(sender);
    drop(proxy);
    let bytes = publisher.retained_bytes();
    drop(publisher); // Abort only the test-owned paused publisher, dropping pressure frames.
    world.close().await;
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "status":"PASS", "boundary":"actual application publisher count admission before NATS",
            "physical_broker_saturation":false, "queue_limit":OUTBOUND_QUEUE,
            "byte_limit":outbound::MAX_BYTES, "retained_pressure_bytes":bytes,
            "same_native_socket":connection, "final_cid":sent.id, "final_seq":refused.seq,
            "wire_hash":refused.env.wire_hash(), "canonical_hash":refused.hash,
            "same_signed_ciphertext_frame_in_fdb":true, "cold_exact_once":true,
            "final_event_observed_ms_since_proxy_start":recovered_at_ms,
            "new_durable_events_after_target":0, "production_repair_seconds":60
        }))
        .unwrap(),
    )
    .unwrap();
}

async fn settled_prefix(
    world: &World,
    sender: &Device,
    receiver: &Device,
    proxy: &FinalEventProxy,
    space: &str,
    seed: &str,
) -> Vec<Sequenced> {
    let until = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let events = world.events(space).await;
        let next = events.last().map_or(0, |e| e.seq + 1);
        let records = proxy.snapshot();
        let all_seen = events.iter().all(|e| records.iter().any(|r| matches!(&r.kind,
            Kind::Event { id, seq, hash, dropped:false, .. } if id == e.env.client_id() && *seq == e.seq && hash == &e.hash)));
        let delivered =
            sender.core.timeline(space.into()).unwrap().iter().any(|e| {
                e.id == seed && matches!(e.delivery, Delivery::Delivered | Delivery::Read)
            });
        if all_seen
            && delivered
            && sender.core.space(space.into()).unwrap().event_count == next
            && receiver.core.space(space.into()).unwrap().event_count == next
            && sender.core.wait_until_settled(500).await.is_ok()
            && receiver.core.wait_until_settled(500).await.is_ok()
            && world.events(space).await == events
        {
            assertions::chain(space, &events);
            return events;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "MLS/profile/receipt/outbox setup must settle before the fault"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
