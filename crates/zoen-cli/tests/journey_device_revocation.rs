//! An unlinked device's existing sockets lose authorization on every relay node.

mod common;
use common::*;
use roda_log::{Author, Signer};
use roda_proto::{ClientFrame, Envelope, ErrorCode, Op, ServerFrame};
use roda_types::{EventBody, Privacy, Seen, SpaceKind};
use sqlx::{Connection, PgConnection};

async fn barrier(client: &mut RawClient) {
    assert!(client.send_if_open(&ClientFrame::Ping).await);
    while !matches!(client.recv().await, ServerFrame::Pong) {}
}

async fn refused_or_closed(client: &mut RawClient) {
    while let Some(frame) = client.recv_or_close().await {
        assert!(
            matches!(
                frame,
                ServerFrame::Error {
                    code: ErrorCode::Unauthorized,
                    ..
                }
            ),
            "revoked device received authorized output: {frame:?}"
        );
    }
}

async fn revoke_live_sockets(two_nodes: bool, miss_bus_invalidation: bool) {
    let nats = two_nodes.then(|| std::env::var("ZOEN_NATS_URL").expect("set ZOEN_NATS_URL"));
    // One connection in each bounded pool exposes nested-acquisition deadlocks.
    let mut w = World::new_with("revoke_live", nats, &[("ZOEN_DB_POOL", "1")]).await;
    w.init("ana", "Ana");
    let read_key = |name: &str| {
        let bytes: [u8; 32] = std::fs::read(w.dir.join("ana/vault").join(name))
            .unwrap()
            .try_into()
            .unwrap();
        Signer::from_secret(&bytes)
    };
    let root = read_key("zoen.identity.v1");
    let first = Author::device(&root, read_key("zoen.device.v1"));
    let lost = Author::device(&root, Signer::generate());
    let other_url = if two_nodes {
        format!("http://127.0.0.1:{}", w.start_node())
    } else {
        w.relay_url()
    };
    let mut controller = RawClient::reconnect(&w.relay_url(), first).await;
    let mut syncing = RawClient::reconnect(&other_url, lost.clone()).await;
    let mut publishing = RawClient::reconnect(&other_url, lost.clone()).await;
    let mut receiving = RawClient::reconnect(&other_url, lost.clone()).await;
    barrier(&mut controller).await;
    barrier(&mut syncing).await;
    barrier(&mut publishing).await;
    barrier(&mut receiving).await;
    let space = roda_types::new_id("readable");
    let created = syncing
        .publish_body(
            &space,
            None,
            EventBody::SpaceCreated {
                title: "Still online".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .await
        .unwrap();
    // Receipt on the controller proves that the remote node's NATS subscription is live.
    loop {
        if matches!(controller.recv().await, ServerFrame::Event { ev } if ev.env.client_id() == created.env.client_id())
        {
            break;
        }
    }
    barrier(&mut syncing).await;
    barrier(&mut publishing).await;
    barrier(&mut receiving).await;
    if miss_bus_invalidation {
        // Commit the authoritative revocation without publishing any NATS notice.
        // The online remote sockets must independently fence Sync, writes and sends.
        let mut pg = PgConnection::connect(&w.db_url).await.unwrap();
        let changed = sqlx::query("UPDATE devices SET revoked_at = now() WHERE device = $1")
            .bind(lost.device.as_ref().unwrap())
            .execute(&mut pg)
            .await
            .unwrap();
        assert_eq!(changed.rows_affected(), 1);
    } else {
        controller
            .request(Op::Unlink {
                device: lost.device.clone().unwrap(),
            })
            .await
            .unwrap();
    }
    let attempt = lost.sign_event(
        &space,
        "revoked-write",
        now_ms(),
        Some(Seen {
            seq: created.seq,
            hash: created.hash.clone(),
        }),
        EventBody::MessagePosted {
            message: "revoked".into(),
            text: "must not land".into(),
            attaches: None,
            reply: None,
        },
    );
    syncing
        .send_if_open(&ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        })
        .await;
    publishing
        .send_if_open(&ClientFrame::Publish {
            env: Envelope::plain(&attempt),
        })
        .await;
    controller
        .publish_body(
            &space,
            Some(Seen {
                seq: created.seq,
                hash: created.hash,
            }),
            EventBody::MessagePosted {
                message: "active".into(),
                text: "the surviving device still writes".into(),
                attaches: None,
                reply: None,
            },
        )
        .await
        .unwrap();
    // `receiving` never asks for anything after revocation: even pushed plaintext
    // from another node must be refused when that node missed the invalidation.
    refused_or_closed(&mut receiving).await;
    refused_or_closed(&mut syncing).await;
    refused_or_closed(&mut publishing).await;
    let events = w.events_in(&space).await;
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|ev| ev.env.client_id() != "revoked-write"));
}

#[tokio::test]
async fn unlink_closes_already_online_sync_and_publish_sockets() {
    revoke_live_sockets(false, false).await;
}

#[tokio::test]
async fn unlink_closes_already_online_sockets_on_another_node() {
    revoke_live_sockets(true, false).await;
}

#[tokio::test]
async fn missed_bus_invalidation_cannot_deliver_or_authorize_on_another_node() {
    revoke_live_sockets(true, true).await;
}

#[tokio::test]
async fn a_device_can_unlink_its_own_online_session_with_a_single_connection() {
    let w = World::with_env("revoke_self", &[("ZOEN_DB_POOL", "1")]).await;
    let mut client = RawClient::connect(&w.relay_url(), "self_revoke").await;
    barrier(&mut client).await;
    let device = client.author.device.clone().unwrap();
    assert!(
        client
            .send_if_open(&ClientFrame::Req {
                id: 71,
                op: Op::Unlink {
                    device: device.clone()
                },
            })
            .await
    );
    refused_or_closed(&mut client).await;
    let mut pg = PgConnection::connect(&w.db_url).await.unwrap();
    let revoked: bool =
        sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM devices WHERE device = $1")
            .bind(device)
            .fetch_one(&mut pg)
            .await
            .unwrap();
    assert!(revoked);
}
