//! Real local socket/Net lifecycle; no fabricated authenticated Net or channel.
use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientFrame, DeviceRecord, ServerFrame, PROTOCOL_VERSION};
use std::{cell::Cell, sync::Arc, time::Duration};
use tokio_tungstenite::tungstenite::Message;

pub(crate) struct Relay {
    pub(crate) url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) async fn relay(peer: String) -> Relay {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut sockets = tokio::task::JoinSet::new();
        let mut connection = 0;
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let fresh = connection > 0;
            connection += 1;
            let peer = peer.clone();
            sockets.spawn(async move {
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                    return;
                };
                let ClientFrame::Hello {
                    identity,
                    device,
                    cert,
                    ..
                } = ClientFrame::decode(&bytes).unwrap()
                else {
                    panic!("expected signed device Hello");
                };
                assert!(event_log::verify_sig(
                    &identity,
                    &event_log::device_cert_message(&device),
                    &cert,
                ));
                socket
                    .send(Message::Binary(
                        ServerFrame::Challenge {
                            nonce: "directory-generation".into(),
                            relay: "directory-generation".into(),
                            protocol: PROTOCOL_VERSION,
                            capabilities: Vec::new(),
                            server_time_ms: None,
                        }
                        .encode()
                        .into(),
                    ))
                    .await
                    .unwrap();
                let Some(Ok(Message::Binary(bytes))) = socket.next().await else {
                    return;
                };
                let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                    panic!("expected device authentication");
                };
                assert!(event_log::verify_sig(
                    &device,
                    &protocol::auth_message("directory-generation", "directory-generation"),
                    &sig,
                ));
                socket
                    .send(Message::Binary(
                        ServerFrame::Ready {
                            identity,
                            registered: true,
                        }
                        .encode()
                        .into(),
                    ))
                    .await
                    .unwrap();
                while let Some(Ok(Message::Binary(bytes))) = socket.next().await {
                    let frame = match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Req {
                            id,
                            op: Op::Devices,
                        } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Devices(vec![
                                DeviceRecord {
                                    device: device.clone(),
                                    revoked: false,
                                },
                                DeviceRecord {
                                    device: peer.clone(),
                                    revoked: fresh,
                                },
                            ])),
                        },
                        ClientFrame::Req {
                            id,
                            op: Op::Unlink { device: target },
                        } => {
                            assert_eq!(target, peer);
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        }
                        ClientFrame::Req { id, .. } => ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                        ClientFrame::Sync { .. } => ServerFrame::SyncDone,
                        ClientFrame::Ping => ServerFrame::Pong {
                            server_time_ms: None,
                        },
                        _ => continue,
                    };
                    if socket
                        .send(Message::Binary(frame.encode().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    Relay { url, task }
}

struct Fixture {
    client: Arc<Client>,
    _relay: Relay,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.client.stop_sync();
    }
}
pub(crate) async fn online(client: &Client) {
    client.start_sync(None).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.connection().state != "online" || !client.connection().synced {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_old_directory_and_unlink_reply_cannot_apply_after_same_owner_net_replacement() {
    let peer = Signer::from_secret(&[74; 32]).id();
    let relay = relay(peer.clone()).await;
    let client = Client::open(":memory:".into(), "en".into()).unwrap();
    client
        .lock()
        .create_account("Directory owner", "directory_owner", &relay.url)
        .unwrap();
    // Synthetic socket fixture only: avoid the unrelated registration workflow.
    // The real Hello certificate and Auth signatures are still verified above.
    client.lock().set_registered(true).unwrap();
    let fixture = Fixture {
        client,
        _relay: relay,
    };
    let client = &fixture.client;
    online(client).await;
    let (old_owner, old_channel, old_reply) =
        client.device_directory_request(Op::Devices).await.unwrap();
    let (unlink_owner, unlink_channel, unlink_reply) = client
        .device_directory_request(Op::Unlink {
            device: peer.clone(),
        })
        .await
        .unwrap();
    assert!(matches!(unlink_reply, Reply::Done));
    let Reply::Devices(old_list) = old_reply else {
        panic!("old directory response");
    };
    assert!(old_list
        .iter()
        .any(|record| record.device == peer && !record.revoked));

    // The old network I/O is completely finished. Holding these real results is
    // the deterministic barrier before their caller's completion continuation.
    client.stop_sync();
    online(client).await;
    let fresh = client.devices_impl().await.unwrap();
    assert!(fresh
        .iter()
        .any(|record| record.device_id == peer && record.revoked));
    assert!(DirectoryOwner::capture(&client.lock()).unwrap() == old_owner);
    let before =
        ["devices.active", "devices.revoked"].map(|key| client.lock().store.meta(key).unwrap());

    let directory_applied = Cell::new(false);
    let late_directory =
        client.with_device_directory_completion(&old_owner, &old_channel, |engine| {
            directory_applied.set(true);
            old_owner.list(engine, old_list)
        });
    assert!(matches!(late_directory, Err(CoreError::Stale { .. })));
    assert!(
        !directory_applied.get(),
        "a retired channel must not enter the cache writer"
    );
    let unlink_applied = Cell::new(false);
    let late_unlink =
        client.with_device_directory_completion(&unlink_owner, &unlink_channel, |engine| {
            unlink_applied.set(true);
            unlink_owner.unlinked(engine, &peer)
        });
    assert!(matches!(late_unlink, Err(CoreError::Stale { .. })));
    assert!(
        !unlink_applied.get(),
        "an old acknowledgement must not enter the replacement engine"
    );
    assert_eq!(
        ["devices.active", "devices.revoked"].map(|key| client.lock().store.meta(key).unwrap()),
        before,
    );
    assert!(client.lock().revoked_devices().contains(&peer));
}
