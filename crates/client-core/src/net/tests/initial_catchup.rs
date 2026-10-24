//! Original production WebSocket initial-catch-up liveness regressions.
//! Fixture budgets are deliberate test policy, not a production SLA or Signal constants.
use super::*;

const SPACE: &str = "sp_initial_catchup";

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn healthy_pongs_without_first_sync_done_cannot_retain_pending_outbox_forever() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("initial-catchup").unwrap()));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("pending.sqlite");
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    let (_, device_secret, _) = engine.create_account("Owner", "owner", &url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let author = engine.net.author.clone().unwrap();
    let mut head = None;
    let genesis = sequence(
        Envelope::plain(&author.sign_event(
            SPACE,
            "initial-create",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Initial catch-up".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )),
        &mut head,
    );
    assert_eq!(engine.ingest(genesis), Ingest::Applied);
    let pending = engine
        .send_message(SPACE, "Do not lose or prematurely publish me")
        .unwrap();
    let before = engine.store.outbox().unwrap();
    assert_eq!(before.len(), 1);
    let before = serde_json::to_vec(&before[0].event).unwrap();
    assert_eq!(engine.next_seq(SPACE), 1);
    let engine = Arc::new(Mutex::new(engine));
    let (_commands, mut commands) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, closed) = oneshot::channel();
    let pongs = AtomicUsize::new(0);
    let publishes = AtomicUsize::new(0);
    let server = async {
        let mut socket = catch_up_socket(&listener).await;
        let mut closed = Box::pin(closed);
        let mut heartbeat = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                result = &mut closed => { result.unwrap(); break; }
                _ = heartbeat.tick() => {
                    if socket.send(Message::Binary(ServerFrame::Pong { server_time_ms: None }.encode().into())).await.is_ok() {
                        pongs.fetch_add(1, Ordering::SeqCst);
                    }
                }
                frame = socket.next() => match frame {
                    Some(Ok(Message::Binary(bytes))) => match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Publish { .. } => { publishes.fetch_add(1, Ordering::SeqCst); }
                        ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                        other => panic!("unexpected traffic while first SyncDone is withheld: {other:?}"),
                    },
                    Some(Ok(_)) => {},
                    Some(Err(_)) | None => { closed.await.unwrap(); break; }
                }
            }
        }
    };
    let mut backoff = Duration::from_millis(2500);
    let client = async {
        let exit = session_with_repair_policy(
            &ctx,
            &mut commands,
            &mut backoff,
            CursorRepairPolicy {
                interval: Duration::from_secs(30),
                timeout: Duration::from_millis(120),
                initial_inactivity: Duration::from_millis(120),
            },
        )
        .await;
        finished.send(()).unwrap();
        exit
    };
    let result = tokio::time::timeout(Duration::from_millis(900), async {
        tokio::join!(client, server)
    })
    .await;
    assert!(
        pongs.load(Ordering::SeqCst) >= 3,
        "the socket stayed responsive"
    );
    assert_eq!(
        publishes.load(Ordering::SeqCst),
        0,
        "no publish before first SyncDone"
    );
    assert!(!ctx.status.borrow().synced);
    assert_eq!(
        backoff,
        Duration::from_millis(2500),
        "no successful initial completion resets backoff"
    );
    {
        let engine = lock(&engine);
        assert_eq!(engine.next_seq(SPACE), 1);
        assert_eq!(engine.store.outbox().unwrap().len(), 1);
        assert_eq!(
            serde_json::to_vec(&engine.store.outbox().unwrap()[0].event).unwrap(),
            before
        );
        assert!(engine
            .timeline(SPACE)
            .unwrap()
            .iter()
            .any(|row| row.id == pending.id));
    }
    ctx.engine.revoke();
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device_secret.to_vec())).unwrap());
    assert_eq!(reopened.next_seq(SPACE), 1);
    assert_eq!(reopened.store.outbox().unwrap().len(), 1);
    assert_eq!(
        serde_json::to_vec(&reopened.store.outbox().unwrap()[0].event).unwrap(),
        before
    );
    eprintln!("initial-catchup witness: healthy_pongs={}, publishes={}, durable_pending=1, durable_next_seq=1, cold_reopen=true, session_exited={}", pongs.load(Ordering::SeqCst), publishes.load(Ordering::SeqCst), result.is_ok());
    let (exit, ()) = result.expect("healthy Pongs masked an unanswered FIRST Sync; the session must reconnect without losing pending work");
    assert!(matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out"));
}

fn pending_engine(url: &str, path: &str) -> (Engine, [u8; 32], Author, Option<Seen>, Sequenced) {
    let mut engine = Engine::open(path).unwrap();
    let (_, secret, _) = engine.create_account("Owner", "owner", url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let author = engine.net.author.clone().unwrap();
    let mut head = None;
    let genesis = sequence(
        Envelope::plain(&author.sign_event(
            SPACE,
            "initial-create",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Initial catch-up".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )),
        &mut head,
    );
    assert_eq!(engine.ingest(genesis.clone()), Ingest::Applied);
    engine
        .send_message(SPACE, "Retain this offline intent")
        .unwrap();
    (engine, secret, author, head, genesis)
}

fn context(engine: &Arc<Mutex<Engine>>) -> Ctx {
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    }
}

fn policy() -> CursorRepairPolicy {
    CursorRepairPolicy {
        initial_inactivity: Duration::from_millis(240),
        interval: Duration::from_secs(30),
        timeout: Duration::from_secs(5),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn genuinely_advancing_replay_pages_outlive_one_inactivity_window_then_flush_exact_intent() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("initial-pages").unwrap()));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("pages.sqlite");
    let (engine, secret, author, mut head, _) = pending_engine(&url, path.to_str().unwrap());
    let original = engine.store.outbox().unwrap()[0].event.clone();
    // The real protocol streams Event frames and one ordered SyncDone; there is
    // no page marker. These are three separately delayed fixture pages, each
    // carrying actual signed canonical prefix growth, not a server paginator mock.
    let mut pages = Vec::new();
    for index in 0..3 {
        let mut page = Vec::new();
        for offset in 0..2 {
            let ordinal = index * 2 + offset;
            page.push(sequence(
                Envelope::plain(&author.sign_event(
                    SPACE,
                    &format!("history-{ordinal}"),
                    2 + ordinal,
                    head.clone(),
                    EventBody::MessagePosted {
                        message: format!("history-body-{ordinal}"),
                        text: format!("Historical page {index}, message {offset}"),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                )),
                &mut head,
            ));
        }
        pages.push(page);
    }
    let engine = Arc::new(Mutex::new(engine));
    let ctx = context(&engine);
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (finished, closed) = oneshot::channel();
    let started = tokio::time::Instant::now();
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        let streaming_started = tokio::time::Instant::now();
        for (index, page) in pages.iter().enumerate() {
            let until = tokio::time::Instant::now() + Duration::from_millis(140);
            loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(until) => break,
                    frame = request(&mut socket) => match frame {
                        ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {},
                        other => panic!("no offline publication before initial completion: {other:?}"),
                    }
                }
            }
            for ev in page {
                reply(&mut socket, ServerFrame::Event { ev: ev.clone() }).await;
            }
            // An actual command/response ordering barrier observes prior ingress,
            // rather than guessing that a sleep caused SQLite acceptance.
            let (answer, answered) = oneshot::channel();
            let marker = format!("page-{index}");
            cmd.send(Cmd::Req {
                op: Op::FetchLink { id: marker.clone() },
                reply: answer,
            })
            .unwrap();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Req {
                        id,
                        op: Op::FetchLink { id: actual },
                    } if actual == marker => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Link(None)),
                            },
                        )
                        .await;
                        break;
                    }
                    ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                    other => panic!("unexpected page barrier traffic: {other:?}"),
                }
            }
            assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
            let e = lock(&engine);
            assert_eq!(e.next_seq(SPACE), 1 + 2 * (index as u64 + 1));
            assert_eq!(e.store.outbox().unwrap().len(), 1);
            assert!(!ctx.status.borrow().synced);
        }
        assert!(streaming_started.elapsed() > policy().initial_inactivity);
        reply(&mut socket, ServerFrame::SyncDone).await;
        let published = loop {
            match request(&mut socket).await {
                ClientFrame::Publish { env } => break env,
                ClientFrame::Req { id, .. } => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await
                }
                ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                other => panic!("unexpected after completion: {other:?}"),
            }
        };
        assert_eq!(published.plain_event(), Some(original.clone()));
        let confirm = sequence(published, &mut head);
        reply(
            &mut socket,
            ServerFrame::Accepted {
                space: SPACE.into(),
                client_id: original.client_id.clone(),
                seq: confirm.seq,
            },
        )
        .await;
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: confirm.clone(),
            },
        )
        .await;
        reply(&mut socket, ServerFrame::Event { ev: confirm }).await;
        let (answer, answered) = oneshot::channel();
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: "confirmed".into(),
            },
            reply: answer,
        })
        .unwrap();
        loop {
            match request(&mut socket).await {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: marker },
                } if marker == "confirmed" => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Req { id, .. } => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await
                }
                ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                other => panic!("confirmed intent cannot publish twice: {other:?}"),
            }
        }
        assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
        assert!(ctx.status.borrow().synced);
        assert_eq!(lock(&engine).next_seq(SPACE), 8);
        assert!(lock(&engine).store.outbox().unwrap().is_empty());
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
    };
    let mut backoff = Duration::from_secs(4);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("legitimate pages cannot be replaced by an absolute history deadline");
    assert!(matches!(exit, Exit::Stop));
    assert_eq!(backoff, Duration::from_millis(500));
    assert!(started.elapsed() > policy().initial_inactivity);
    ctx.engine.revoke();
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(secret.to_vec())).unwrap());
    assert_eq!(reopened.next_seq(SPACE), 8);
    assert!(reopened.store.outbox().unwrap().is_empty());
    assert!(reopened.verify_log(SPACE).valid);
    let history = reopened.timeline(SPACE).unwrap();
    assert_eq!(history.len(), 7);
    assert_eq!(
        history
            .iter()
            .filter(|row| row.id == original.client_id)
            .count(),
        1
    );
}

#[tokio::test(flavor = "current_thread")]
async fn stop_during_initial_catchup_retires_watchdog_and_retains_pending_intent() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (engine, _, _, _, _) = pending_engine(&url, ":memory:");
    let original = serde_json::to_vec(&engine.store.outbox().unwrap()[0].event).unwrap();
    let engine = Arc::new(Mutex::new(engine));
    let ctx = context(&engine);
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (finished, closed) = oneshot::channel();
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(
            &mut socket,
            ServerFrame::Pong {
                server_time_ms: None,
            },
        )
        .await;
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
    };
    let mut backoff = Duration::from_secs(2);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("Stop cannot wait for the initial inactivity deadline or socket close");
    assert!(matches!(exit, Exit::Stop));
    assert!(!ctx.status.borrow().synced);
    assert_eq!(backoff, Duration::from_secs(2));
    assert_eq!(lock(&engine).next_seq(SPACE), 1);
    assert_eq!(
        serde_json::to_vec(&lock(&engine).store.outbox().unwrap()[0].event).unwrap(),
        original
    );
    ctx.engine.revoke();
}

mod inactivity_cases;
