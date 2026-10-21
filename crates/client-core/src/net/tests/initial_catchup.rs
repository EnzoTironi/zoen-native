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
    let directory = Directory(std::env::temp_dir().join(types::new_id("initial-catchup")));
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
                // RED control: the current owner applies this only AFTER its
                // first SyncDone. Initial admission has no deadline at all.
                timeout: Duration::from_millis(120),
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
