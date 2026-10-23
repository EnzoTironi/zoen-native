//! Preserved session failure scenarios; stable test entries stay in the parent.
use super::*;

#[derive(Clone, Copy)]
enum EndRepair {
    Stop,
    Unavailable,
    Deadline,
}

pub(super) async fn pending_repair_is_session_owned_and_pongs_cannot_mask_failure_or_stop() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for mode in [EndRepair::Stop, EndRepair::Unavailable, EndRepair::Deadline] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        engine.create_account("Owner", "owner", &url).unwrap();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
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
        let server = async {
            let mut socket = catch_up_socket(&listener).await;
            reply(&mut socket, ServerFrame::SyncDone).await;
            let mut marks = Vec::new();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert!(cursors.is_empty());
                        break;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
            match mode {
                EndRepair::Stop => cmd.send(Cmd::Stop).unwrap(),
                EndRepair::Unavailable => {
                    reply(
                        &mut socket,
                        ServerFrame::error(ErrorCode::Unavailable, "repair store unavailable"),
                    )
                    .await
                }
                EndRepair::Deadline => {}
            }
            let mut closed = Box::pin(closed);
            let mut heartbeat = tokio::time::interval(Duration::from_millis(20));
            loop {
                tokio::select! {
                    done = &mut closed => { done.unwrap(); break; }
                    _ = heartbeat.tick() => {
                        // No SyncDone: transport liveness cannot extend the repair budget.
                        let _ = socket.send(Message::Binary(ServerFrame::Pong { server_time_ms: None }.encode().into())).await;
                    }
                    frame = socket.next() => match frame {
                        Some(Ok(Message::Binary(bytes))) => {
                            let frame = ClientFrame::decode(&bytes).unwrap();
                            assert!(!matches!(frame, ClientFrame::Sync { .. }), "one repair in flight only");
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => { closed.await.unwrap(); break; }
                    }
                }
            }
        };
        let mut backoff = Duration::from_millis(500);
        let client = async {
            let exit = session_with_repair_policy(
                &ctx,
                &mut commands,
                &mut backoff,
                CursorRepairPolicy {
                    interval: Duration::from_millis(40),
                    timeout: Duration::from_millis(120),
                    initial_inactivity: Duration::from_secs(5),
                },
            )
            .await;
            finished.send(()).unwrap();
            exit
        };
        let (exit, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(client, server)
        })
        .await
        .expect("repair failure or stop retained the session");
        match mode {
            EndRepair::Stop => assert!(matches!(exit, Exit::Stop)),
            EndRepair::Unavailable => {
                assert!(matches!(exit, Exit::Retry(reason) if reason == "repair store unavailable"))
            }
            EndRepair::Deadline => assert!(
                matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out")
            ),
        }
        ctx.engine.revoke();
    }
}
