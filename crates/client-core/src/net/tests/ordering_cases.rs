use super::*;

pub(super) async fn a_control_notice_is_dispatched_between_slow_maintenance_passes() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let mut engine = Engine::open(":memory:").unwrap();
    engine.create_account("Bruno", "bruno", &url).unwrap();
    engine.set_registered(true).unwrap();
    // This is a top-up of a device whose initial batch was already acknowledged.
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    assert!(engine.mls_settled(), "the fixture has no other MLS work");
    let device = engine.net.account.as_ref().unwrap().device.clone();
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Online,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let passes = Arc::new(AtomicUsize::new(0));
    let (maintenance_started, during_maintenance) = channel::channel();
    let (notice_sent, notice_written) = channel::channel();
    let notice_written = Mutex::new(notice_written);
    // An independent executor puts the notice on the real socket while this
    // session's sole worker is busy. No server task can drive its I/O for it.
    let relay = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let (socket, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
                let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
                    panic!("hello")
                };
                reply(
                    &mut socket,
                    ServerFrame::Challenge {
                        nonce: "maintenance-test".into(),
                        relay: "maintenance-test".into(),
                        protocol: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                server_time_ms: None,
                    },
                )
                .await;
                assert!(matches!(
                    request(&mut socket).await,
                    ClientFrame::Auth { .. }
                ));
                reply(
                    &mut socket,
                    ServerFrame::Ready {
                        identity,
                        registered: true,
                    },
                )
                .await;
                assert!(matches!(
                    request(&mut socket).await,
                    ClientFrame::Sync { all: true, .. }
                ));
                reply(&mut socket, ServerFrame::SyncDone).await;
                during_maintenance
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                reply(
                    &mut socket,
                    ServerFrame::KeyPackagesLow {
                        device,
                        remaining: 7,
                    },
                )
                .await;
                notice_sent.send(()).unwrap();
                loop {
                    match request(&mut socket).await {
                        ClientFrame::Req {
                            id,
                            op:
                                Op::PublishKeyPackages {
                                    packages,
                                    last_resort,
                                },
                        } => {
                            assert_eq!(
                                packages.len(),
                                25,
                                "the notice for 7 remaining packages requests the exact top-up to 32"
                            );
                            assert!(
                                last_resort.is_none(),
                                "the existing last-resort package is retained"
                            );
                            reply(
                                &mut socket,
                                ServerFrame::Res {
                                    id,
                                    result: Ok(Reply::Done),
                                },
                            )
                            .await;
                            cmd.send(Cmd::Stop).unwrap();
                            return;
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
                        ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                        _ => {}
                    }
                }
            })
    });
    let work_passes = passes.clone();
    let work_engine = engine.clone();
    let ctx = Ctx {
        engine: EngineLease::new(engine),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: Some(Arc::new(move || {
            let pass = work_passes.fetch_add(1, Ordering::SeqCst) + 1;
            if pass == 1 {
                return;
            }
            if pass == 2 {
                maintenance_started.send(()).unwrap();
                notice_written
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
            if pass == 3 {
                assert!(
                    !lock(&work_engine).mls_settled(),
                    "the queued low-stock notice must be dispatched before another slow pass"
                );
            }
            // Synchronous engine work exceeds the 50ms period on every later pass.
            std::thread::sleep(MAINTENANCE_PERIOD * 2);
        })),
    };
    let mut backoff = Duration::from_millis(500);
    let _ = session(&ctx, &mut commands, &mut backoff).await;
    relay.join().unwrap();
}

pub(super) async fn member_seen_beyond_the_next_sequence_is_fetched_once_then_forks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let mut engine = Engine::open(":memory:").unwrap();
    engine.create_account("Ana", "ana", &url).unwrap();
    engine.set_registered(true).unwrap();
    let author = engine.net.author.clone().unwrap();
    let space = "seen-loop";
    let mut head = None;
    let created = sequence(
        Envelope::plain(&author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: space.into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )),
        &mut head,
    );
    assert_eq!(engine.ingest(created), Ingest::Applied);
    let peer = event_log::Author::root(event_log::Signer::generate()?);
    let added = sequence(
        Envelope::plain(&author.sign_event(
            space,
            "add-peer",
            2,
            head.clone(),
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        )),
        &mut head,
    );
    assert_eq!(engine.ingest(added), Ingest::Applied);
    let next = engine.next_seq(space);
    let bad = sequence(
        Envelope::plain(&peer.sign_event(
            space,
            "seen-loop",
            3,
            Some(Seen {
                seq: next + 8,
                hash: "12".repeat(32),
            }),
            EventBody::MessagePosted {
                message: "other-fork".into(),
                text: "other-fork".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )),
        &mut head,
    );
    assert_eq!(bad.seq, next);
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let relay_engine = engine.clone();
    let relay_space = space.to_string();
    let relay = async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
            panic!("hello");
        };
        reply(
            &mut socket,
            ServerFrame::Challenge {
                nonce: "seen-loop".into(),
                relay: "seen-loop".into(),
                protocol: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                server_time_ms: None,
            },
        )
        .await;
        assert!(matches!(
            request(&mut socket).await,
            ClientFrame::Auth { .. }
        ));
        reply(
            &mut socket,
            ServerFrame::Ready {
                identity,
                registered: true,
            },
        )
        .await;
        assert!(matches!(
            request(&mut socket).await,
            ClientFrame::Sync { all: true, .. }
        ));
        reply(&mut socket, ServerFrame::Event { ev: bad }).await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut refetches = 0u32;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                break;
            }
            let frame = match tokio::time::timeout(left, request(&mut socket)).await {
                Ok(frame) => frame,
                Err(_) => break,
            };
            match frame {
                ClientFrame::Sync { .. } => {
                    refetches += 1;
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
                    .await;
                }
                ClientFrame::Ping => {
                    reply(
                        &mut socket,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await;
                }
                ClientFrame::Publish { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        let guard = lock(&relay_engine);
        let integrity = guard.state.spaces[&relay_space].integrity_error.clone();
        let stored = guard.logs[&relay_space]
            .events()
            .iter()
            .any(|event| event.client_id == "seen-loop");
        drop(guard);
        cmd.send(Cmd::Stop).unwrap();
        (refetches, integrity, stored)
    };
    let ctx = Ctx {
        engine: EngineLease::new(engine),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let mut backoff = Duration::from_millis(500);
    let (_exit, (refetches, integrity, stored)) =
        tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(session(&ctx, &mut commands, &mut backoff), relay)
        })
        .await
        .expect("seen-loop sync stalled");
    assert_eq!(refetches, 0, "sync asked for the same next event again");
    assert_eq!(
        integrity.as_deref(),
        Some("Members see different groups in this chat (the server may have forked it).")
    );
    assert!(!stored, "the refused event was appended");

    Ok(())
}
