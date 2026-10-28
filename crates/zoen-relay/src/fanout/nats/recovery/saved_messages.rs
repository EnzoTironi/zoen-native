//! Actual Clients, certified linking, relay TCP handlers, PG, FDB and NATS.
use super::{
    fixture::{until, Device, Vault, World},
    wire::{FinalEventProxy, Kind},
};
use crate::log::LogStore;
use client_core::{Client, EntryKind, SpaceKindDto};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Barrier;

#[test]
#[ignore = "requires owned PostgreSQL/FDB/NATS and a fresh evidence directory"]
fn saved_messages_certified_devices_converge_after_concurrent_creation_and_lost_ack() {
    let _network = unsafe { foundationdb::boot() };
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
}
async fn ensure(device: &Device) -> Result<serde_json::Value, client_core::CoreError> {
    let output = device
        .core
        .client_request(
            serde_json::json!({
                "op":"saved_messages", "expected_identity_id":device.account.identity_id,
                "expected_device_id":device.account.device_id,
            })
            .to_string(),
        )
        .await?;
    Ok(serde_json::from_str(&output).unwrap())
}
fn reopen(device: Device, relay: Option<&str>) -> Device {
    let path = device.path.clone();
    let vault = device.vault.clone();
    let mut account = device.account.clone();
    device.core.stop_sync();
    drop(device);
    // Existing journey World::cmd_at uses the same closed synthetic-account routing
    // to exercise another node of one cell. Enrollment still used public linking.
    if let Some(relay) = relay {
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE meta SET value=json_set(value,'$.relay_url',?1) WHERE key='account'",
                [relay],
            )
            .unwrap();
        account.relay_url = relay.into();
    }
    let core = Client::open(path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
    assert!(core.unlock(vault.clone()).unwrap());
    Device {
        core,
        account,
        path,
        vault,
    }
}
fn readable(device: &Device, space: &str, id: &str, text: &str) -> usize {
    device
        .core
        .timeline(space.into())
        .unwrap()
        .iter()
        .filter(|entry| {
            entry.id == id
                && matches!(&entry.kind, EntryKind::Message {text: body, ..} if body == text)
        })
        .count()
}
async fn run() {
    let directory = PathBuf::from(
        std::env::var_os("ZOEN_SAVED_EVIDENCE_DIR").expect("fresh owned evidence path"),
    );
    assert!(!directory.exists(), "preserve previous failures");
    std::fs::create_dir_all(&directory).unwrap();
    let world = World::new(&directory).await;
    let gate = Arc::new(Barrier::new(2));
    let a_wire =
        FinalEventProxy::saved(world.a.port, directory.join("a-wire"), gate.clone(), true).await;
    let b_wire = FinalEventProxy::saved(world.b.port, directory.join("b-wire"), gate, false).await;
    let first = Device::new(&directory, "saved_owner", a_wire.url.clone()).await;
    let core = Client::open(
        directory
            .join("linked.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en-US".into(),
    )
    .unwrap();
    let vault = Arc::new(Vault::default());
    let request = core
        .link_request(a_wire.url.clone(), vault.clone())
        .unwrap();
    let (sent, received) = tokio::join!(
        first.core.link_device(request.code, first.vault.clone()),
        core.link_wait(vault.clone(), 15_000)
    );
    sent.unwrap();
    let account = received.unwrap();
    assert_eq!(account.identity_id, first.account.identity_id);
    assert_ne!(account.device_id, first.account.device_id);
    core.start_sync(None).unwrap();
    let second = Device {
        core,
        account,
        path: directory.join("linked.sqlite"),
        vault,
    };
    first
        .core
        .send_history(second.account.device_id.clone(), None)
        .await
        .unwrap();
    assert_eq!(second.core.wait_until_idle(15_000).await.state, "online");
    second.core.receive_history(15_000, None).await.unwrap();
    let second = reopen(second, Some(&b_wire.url));
    second.core.start_sync(None).unwrap();
    assert_eq!(second.core.wait_until_idle(15_000).await.state, "online");
    assert_eq!(
        first
            .core
            .devices()
            .await
            .unwrap()
            .iter()
            .filter(|d| !d.revoked)
            .count(),
        2
    );
    // Both actual public commands prepare before either proposal crosses its real socket.
    let (lost, accepted) = tokio::join!(ensure(&first), ensure(&second));
    assert!(lost.is_err(), "receipt and genesis were intentionally lost");
    let canonical = accepted.unwrap();
    let space = canonical["id"].as_str().unwrap().to_owned();
    assert_eq!(canonical["kind"], "saved");
    assert!(a_wire
        .snapshot()
        .iter()
        .any(|r| matches!(r.kind, Kind::SavedReceipt { dropped: true })));
    let first_path = first.path.clone();
    let first_vault = first.vault.clone();
    let first_account = first.account.clone();
    first.core.stop_sync();
    drop(first); // Release SQLite's actual exclusive owner before inspecting its outbox.
    let original = {
        let store = store::Store::open(first_path.to_str().unwrap()).unwrap();
        let rows: Vec<_> = store
            .outbox()
            .unwrap()
            .into_iter()
            .filter(|row| row.event.space == space)
            .collect();
        assert_eq!(rows.len(), 1, "unknown ACK retains exact bootstrap");
        rows[0].event.clone()
    };
    a_wire.resume_saved_receipt();
    let core = Client::open(first_path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
    assert!(core.unlock(first_vault.clone()).unwrap());
    let first = Device {
        core,
        account: first_account,
        path: first_path,
        vault: first_vault,
    };
    first.core.start_sync(None).unwrap();
    let status = first.core.wait_until_idle(15_000).await;
    assert_eq!(status.state, "online");
    let retry = ensure(&first).await.unwrap();
    a_wire.resume_saved();
    assert_eq!(retry["id"], space);
    // Catch-up genesis remained dropped until the explicit original-byte retry resolved.
    let attempts: Vec<_> = a_wire
        .snapshot()
        .into_iter()
        .filter_map(|r| {
            if let Kind::SavedEnsure {
                client_id,
                wire_hash,
            } = r.kind
            {
                Some((client_id, wire_hash))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        attempts.len(),
        2,
        "one original proposal and one explicit cold retry"
    );
    assert!(attempts.iter().all(|(id, _)| id == &original.client_id));
    assert!(
        attempts.windows(2).all(|pair| pair[0] == pair[1]),
        "no re-sign on retry"
    );
    let other_candidate = b_wire
        .snapshot()
        .into_iter()
        .find_map(|r| match r.kind {
            Kind::SavedEnsure { client_id, .. } => Some(client_id),
            _ => None,
        })
        .unwrap();
    assert_ne!(
        other_candidate, original.client_id,
        "independent certified device proposals"
    );
    until(
        "both certified devices must finish actual MLS joining",
        || {
            [&first, &second]
                .iter()
                .all(|d| d.core.spaces().iter().any(|s| s.id == space && s.can_write))
        },
    )
    .await;
    let a = first
        .core
        .send_message(space.clone(), "first own-device encrypted note".into())
        .unwrap();
    until("second device decrypts first note", || {
        readable(&second, &space, &a.id, "first own-device encrypted note") == 1
    })
    .await;
    let b = second
        .core
        .send_message(space.clone(), "second own-device encrypted note".into())
        .unwrap();
    until("first device decrypts reply", || {
        readable(&first, &space, &b.id, "second own-device encrypted note") == 1
    })
    .await;
    let events = world.events(&space).await;
    super::assertions::chain(&space, &events);
    assert!(events
        .iter()
        .all(|event| event.env.author() == first.account.identity_id));
    assert_eq!(
        world.log.members(&space).await.unwrap(),
        vec![(first.account.identity_id.clone(), types::Role::Owner)]
    );
    assert!([original.client_id.as_str(), other_candidate.as_str()]
        .contains(&events[0].env.client_id()));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.env.body(), Some(types::EventBody::SpaceCreated { .. })))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.env.sealed_kind() == Some(protocol::SealedKind::Application))
            .count(),
        2
    );
    assert!(events.iter().all(|e| !e
        .env
        .content()
        .windows(b"own-device encrypted note".len())
        .any(|v| v == b"own-device encrypted note")));
    for device in [&first, &second] {
        device.core.wait_until_settled(15_000).await.unwrap();
    }
    for device in [first, second] {
        let cold = reopen(device, None);
        cold.core.start_sync(None).unwrap();
        cold.core.wait_until_settled(15_000).await.unwrap();
        let timeline = cold.core.timeline(space.clone()).unwrap();
        for (id, text) in [
            (&a.id, "first own-device encrypted note"),
            (&b.id, "second own-device encrypted note"),
        ] {
            assert_eq!(timeline.iter().filter(|e| &e.id == id).count(), 1);
            assert_eq!(
                readable(&cold, &space, id, text),
                1,
                "cold client reads actual plaintext, not a sealed placeholder"
            );
        }
        assert_eq!(
            cold.core
                .spaces()
                .iter()
                .filter(|s| s.kind == SpaceKindDto::Saved)
                .count(),
            1
        );
        assert_eq!(
            cold.core.wait_until_settled(15_000).await.unwrap().pending,
            0
        );
    }
    a_wire.save();
    b_wire.save();
    world.close().await;
    std::fs::write(directory.join("result.txt"),"PASS real Client -> TCP relay handlers -> PostgreSQL/FDB/NATS -> same-identity certified Client; concurrent proposals, unknown ACK, MLS ciphertext and cold reopen; no App/device proof\n").unwrap();
}
