use super::*;
use futures_util::{SinkExt, StreamExt};
use protocol::complete_reads::{Device, Page};
use tokio::sync::{Notify, Semaphore};
struct Fixture {
    client: Arc<crate::Client>,
    task: tokio::task::JoinHandle<()>,
    entered: Arc<Notify>,
    release: Arc<Semaphore>,
    cancelled: Arc<Notify>,
    peer: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.client.stop_sync();
        self.task.abort();
    }
}
impl Fixture {
    async fn new(cut: u8) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Semaphore::new(0));
        let cancelled = Arc::new(Notify::new());
        let peer = event_log::Signer::from_secret(&[89; 32]).id();
        let (enter, go, cancel, other) = (
            entered.clone(),
            release.clone(),
            cancelled.clone(),
            peer.clone(),
        );
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let bytes = ws.next().await.unwrap().unwrap().into_data();
            let ClientFrame::Hello {
                identity,
                device,
                cert,
                ..
            } = ClientFrame::decode(&bytes).unwrap()
            else {
                panic!("hello")
            };
            assert!(event_log::verify_sig(
                &identity,
                &event_log::device_cert_message(&device),
                &cert
            ));
            ws.send(Message::Binary(
                ServerFrame::Challenge {
                    nonce: "read-snapshot".into(),
                    relay: "read-snapshot".into(),
                    protocol: protocol::PROTOCOL_VERSION,
                    capabilities: vec![protocol::complete_reads::CAPABILITY.into()],
                    server_time_ms: None,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
            let bytes = ws.next().await.unwrap().unwrap().into_data();
            let ClientFrame::Auth { sig } = ClientFrame::decode(&bytes).unwrap() else {
                panic!("auth")
            };
            assert!(event_log::verify_sig(
                &device,
                &protocol::auth_message("read-snapshot", "read-snapshot"),
                &sig
            ));
            ws.send(Message::Binary(
                ServerFrame::Ready {
                    identity: identity.clone(),
                    registered: true,
                }
                .encode()
                .into(),
            ))
            .await
            .unwrap();
            let mut digest = Vec::new();
            while let Some(Ok(message)) = ws.next().await {
                if !message.is_binary() {
                    continue;
                }
                let frame = ClientFrame::decode(&message.into_data()).unwrap();
                let response = match frame {
                    ClientFrame::Sync { .. } => ServerFrame::SyncDone,
                    ClientFrame::Ping => ServerFrame::Pong {
                        server_time_ms: None,
                    },
                    ClientFrame::Req {
                        id,
                        op: Op::CompleteRead(request),
                    } => {
                        if request.cancel {
                            cancel.notify_one();
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            }
                        } else {
                            assert_eq!(request.kind, Kind::Devices as i32);
                            let mut records = vec![
                                Device {
                                    device: device.clone(),
                                    revoked: false,
                                },
                                Device {
                                    device: other.clone(),
                                    revoked: true,
                                },
                            ];
                            records.sort_by(|a, b| a.device.cmp(&b.device));
                            let index = request.ordinal as usize;
                            assert!(index < 2);
                            if index == 1 {
                                enter.notify_one();
                                go.acquire().await.unwrap().forget();
                                if cut == 3 {
                                    std::future::pending::<()>().await;
                                }
                            }
                            let result = if index == 1 && cut == 1 {
                                Err("synthetic lost terminal".into())
                            } else {
                                let mut page = Page {
                                    kind: Kind::Devices as i32,
                                    snapshot: "e".repeat(64),
                                    ordinal: request.ordinal,
                                    identity: if index == 1 && cut == 2 {
                                        "f".repeat(64)
                                    } else {
                                        identity.clone()
                                    },
                                    device: device.clone(),
                                    header: None,
                                    devices: vec![records[index].clone()],
                                    cursors: Vec::new(),
                                    complete: index == 1,
                                    count: request.ordinal + 1,
                                    digest: Vec::new(),
                                };
                                page.digest = page.rolling_digest(&digest);
                                digest = page.digest.clone();
                                Ok(Reply::CompletePage(page))
                            };
                            ServerFrame::Res { id, result }
                        }
                    }
                    ClientFrame::Req { id, .. } => ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                    _ => continue,
                };
                if ws
                    .send(Message::Binary(response.encode().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let client = crate::Client::open(":memory:".into(), "en".into()).unwrap();
        {
            let mut e = client.lock();
            e.create_account("Paged owner", "paged_owner", &url)
                .unwrap();
            e.set_registered(true).unwrap();
            // This fixture owns the explicit read; background discovery has its
            // own production path and must not consume this deliberate barrier.
            e.net.link.ask_devices = false;
            let mine = e.account().unwrap().device.clone();
            e.devices_arrived(vec![
                protocol::DeviceRecord {
                    device: mine,
                    revoked: false,
                },
                protocol::DeviceRecord {
                    device: peer.clone(),
                    revoked: false,
                },
            ])
            .unwrap();
        }
        client.start_sync(None).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while client.connection().state != "online" || !client.connection().synced {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        Self {
            client,
            task,
            entered,
            release,
            cancelled,
            peer,
        }
    }
    fn cache(&self) -> [Option<String>; 2] {
        let e = self.client.lock();
        ["devices.active", "devices.revoked"].map(|key| e.store.meta(key).unwrap())
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_net_stages_every_page_and_only_installs_the_valid_complete_terminal() {
    for cut in 0..3 {
        let f = Fixture::new(cut).await;
        let before = f.cache();
        let client = f.client.clone();
        let work = tokio::spawn(async move { client.devices().await });
        tokio::time::timeout(Duration::from_secs(5), f.entered.notified())
            .await
            .unwrap();
        assert_eq!(f.cache(), before, "partial page installed");
        f.release.add_permits(1);
        let result = work.await.unwrap();
        if cut == 0 {
            let list = result.unwrap();
            assert_eq!(list.len(), 2);
            assert!(list.iter().any(|d| d.device_id == f.peer && d.revoked));
        } else {
            assert!(result.is_err());
            assert_eq!(f.cache(), before);
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_native_read_never_installs_and_releases_its_live_snapshot() {
    let f = Fixture::new(0).await;
    let before = f.cache();
    let client = f.client.clone();
    let work = tokio::spawn(async move { client.devices().await });
    tokio::time::timeout(Duration::from_secs(5), f.entered.notified())
        .await
        .unwrap();
    work.abort();
    let _ = work.await;
    f.release.add_permits(1);
    tokio::time::timeout(Duration::from_secs(5), f.cancelled.notified())
        .await
        .unwrap();
    assert_eq!(f.cache(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_silent_missing_page_expires_the_actual_transport_lane_without_installing() {
    let f = Fixture::new(3).await;
    let before = f.cache();
    let client = f.client.clone();
    let work = tokio::spawn(async move { client.devices().await });
    tokio::time::timeout(Duration::from_secs(5), f.entered.notified())
        .await
        .unwrap();
    f.release.add_permits(1);
    assert!(tokio::time::timeout(Duration::from_secs(17), work)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.client.connection().error.as_deref() == Some("complete_read_expired") {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(f.cache(), before);
}
