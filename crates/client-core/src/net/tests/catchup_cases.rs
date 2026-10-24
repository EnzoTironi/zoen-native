use super::*;

pub(super) async fn live_event_overtaking_final_replay_page_reconciles_once_without_later_message(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    fn assert_history(engine: &Engine, space: &str) {
        let messages: Vec<_> = engine
            .timeline(space)
            .unwrap()
            .into_iter()
            .filter_map(|entry| match entry.kind {
                crate::EntryKind::Message { text, .. } => Some((entry.id, entry.seq, text)),
                _ => None,
            })
            .collect();
        assert_eq!(
            messages,
            vec![
                (
                    "replay-event".into(),
                    2,
                    "Already in the replay snapshot".into()
                ),
                (
                    "boundary-event".into(),
                    3,
                    "Committed at the replay/live boundary".into()
                ),
            ]
        );
        let events = engine.store.events(space).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event.client_id.as_str())
                .collect::<Vec<_>>(),
            ["created", "add-peer", "replay-event", "boundary-event"]
        );
        assert_eq!(engine.next_seq(space), 4);
    }

    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(
        std::env::temp_dir().join(format!("zoen-replay-handoff-{}", Signer::generate()?.id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let reader_path = directory.0.join("reader.sqlite");
    let relay_path = directory.0.join("canonical.sqlite");
    let mut engine = Engine::open(reader_path.to_str().unwrap()).unwrap();
    let (_, device_secret, _) = engine.create_account("Ana", "ana", &url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let author = engine.net.author.clone().unwrap();
    let peer = Author::root(Signer::generate()?);
    let space = "sp_replay_handoff";
    let mut head = None;
    let canonical = store::Store::open(relay_path.to_str().unwrap()).unwrap();
    for (id, body) in [
        (
            "created",
            EventBody::SpaceCreated {
                title: "Replay handoff".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "add-peer",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let ev = sequence(
            Envelope::plain(&author.sign_event(space, id, 1, head.clone(), body)),
            &mut head,
        );
        canonical.append_event(&ev.plain_event().unwrap()).unwrap();
        assert_eq!(engine.ingest(ev), Ingest::Applied);
    }
    let replay = sequence(
        Envelope::plain(&peer.sign_event(
            space,
            "replay-event",
            2,
            head.clone(),
            EventBody::MessagePosted {
                message: "replay-message".into(),
                text: "Already in the replay snapshot".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )),
        &mut head,
    );
    canonical
        .append_event(&replay.plain_event().unwrap())
        .unwrap();
    // This immutable last replay page excludes the event that will commit at the cut.
    let replay_snapshot = canonical.events(space).unwrap();
    assert_eq!(replay_snapshot.len(), 3);
    let boundary = sequence(
        Envelope::plain(&peer.sign_event(
            space,
            "boundary-event",
            3,
            head.clone(),
            EventBody::MessagePosted {
                message: "boundary-message".into(),
                text: "Committed at the replay/live boundary".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )),
        &mut head,
    );
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
    let (finished, close_socket) = oneshot::channel();
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        // Persist the concurrent event before its live delivery, after the replay cut.
        canonical
            .append_event(&boundary.plain_event().unwrap())
            .unwrap();
        drop(canonical);
        let canonical = store::Store::open(relay_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.events(space).unwrap().len(), 4);
        // Live fanout can overtake an older replay page; no later content wakes repair.
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: boundary.clone(),
            },
        )
        .await;
        let gap_request = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match request(&mut socket).await {
                    ClientFrame::Sync { cursors, all } => break (cursors, all),
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
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await
                    }
                    other => panic!("unexpected before gap repair: {other:?}"),
                }
            }
        })
        .await
        .expect("overtaking live event did not request its missing replay cursor");
        assert!(!gap_request.1);
        assert_eq!(gap_request.0.len(), 1);
        assert_eq!(gap_request.0[0].space, space);
        assert_eq!(gap_request.0[0].next_seq, 2);
        assert!(
            !ctx.status.borrow().synced,
            "repair starts before initial SyncDone"
        );
        reply(&mut socket, ServerFrame::Event { ev: replay }).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        // The real worker requested cursor2; replay from retained canonical storage.
        for event in canonical
            .events(space)
            .unwrap()
            .into_iter()
            .filter(|event| event.seq >= 2)
        {
            let ev = Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(&event),
            };
            reply(&mut socket, ServerFrame::Event { ev }).await;
        }
        reply(&mut socket, ServerFrame::SyncDone).await;
        // An ordinary request/response is an ordered wire barrier after all event
        // frames; it introduces no message that could conceal a missing wake.
        let (marker, marked) = oneshot::channel();
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: "handoff-barrier".into(),
            },
            reply: marker,
        })
        .unwrap();
        loop {
            match request(&mut socket).await {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: link },
                } if link == "handoff-barrier" => {
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
                ClientFrame::Ephemeral { .. } => {}
                ClientFrame::Ping => {
                    reply(
                        &mut socket,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await
                }
                other => panic!("unexpected after gap repair: {other:?}"),
            }
        }
        assert!(matches!(marked.await.unwrap(), Ok(Reply::Link(None))));
        assert!(ctx.status.borrow().synced);
        assert_history(&lock(&engine), space);
        cmd.send(Cmd::Stop).unwrap();
        close_socket.await.unwrap();
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session(&ctx, &mut commands, &mut backoff).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("replay/live handoff did not finish");
    assert!(matches!(exit, Exit::Stop));
    // Aborting the spawned media task does not synchronously drop its lease.
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(reader_path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device_secret.to_vec())).unwrap());
    assert_history(&reopened, space);

    Ok(())
}

pub(super) async fn initial_catch_up_unavailable_retries_without_resetting_backoff() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let mut engine = Engine::open(":memory:").unwrap();
    engine.create_account("Ana", "ana", &url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (status, mut observed) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let ctx = Ctx {
        engine: EngineLease::new(Arc::new(Mutex::new(engine))),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, shutdown) = oneshot::channel();
    let relay = async {
        let mut failed = catch_up_socket(&listener).await;
        reply(
            &mut failed,
            ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
        )
        .await;
        // Keep the failed socket open, exactly as the real relay does. A client
        // that only logs the error will never reach this second connection.
        let mut recovered = catch_up_socket(&listener).await;
        reply(&mut recovered, ServerFrame::SyncDone).await;
        loop {
            match request(&mut recovered).await {
                ClientFrame::Req {
                    id,
                    op: Op::Profiles { ids },
                } if ids.len() == 1 && ids[0] == "catch-up-probe" => {
                    // After catch-up this unrelated error keeps its existing
                    // behavior: it must not abort an in-flight request.
                    reply(
                        &mut recovered,
                        ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
                    )
                    .await;
                    reply(
                        &mut recovered,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Req { id, .. } => {
                    reply(
                        &mut recovered,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await;
                }
                ClientFrame::Ping => {
                    reply(
                        &mut recovered,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await;
                }
                _ => {}
            }
        }
        shutdown.await.unwrap();
        drop((failed, recovered));
    };
    let client = async {
        let mut backoff = Duration::from_secs(4);
        assert!(matches!(
            session(&ctx, &mut commands, &mut backoff).await,
            Exit::Retry(message) if message == "database unavailable"
        ));
        assert!(!ctx.status.borrow().synced);
        assert_eq!(
            backoff,
            Duration::from_secs(4),
            "Ready must not erase retries"
        );
        let exercise = async {
            while !observed.borrow().synced {
                observed.changed().await.unwrap();
            }
            let (reply, answer) = oneshot::channel();
            cmd.send(Cmd::Req {
                op: Op::Profiles {
                    ids: vec!["catch-up-probe".into()],
                },
                reply,
            })
            .unwrap();
            assert!(matches!(answer.await, Ok(Ok(Reply::Done))));
            cmd.send(Cmd::Stop).unwrap();
        };
        let (exit, ()) = tokio::join!(session(&ctx, &mut commands, &mut backoff), exercise,);
        assert!(matches!(exit, Exit::Stop));
        assert_eq!(backoff, Duration::from_millis(500));
        finished.send(()).unwrap();
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("an unavailable initial catch-up must retry while its socket remains open");
}
