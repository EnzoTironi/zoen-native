use super::*;

pub(super) async fn queued_messages_round_trip(
    catch_up: CatchUp,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let mut engine = Engine::open(":memory:").unwrap();
    let (root_secret, device_secret, _) = engine.create_account("Ana", "ana", &url).unwrap();
    engine.set_registered(true).unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let author = engine.net.author.clone().unwrap();
    let space = "sp_initial_catch_up";
    let mut head = None;
    let genesis = sequence(
        Envelope::plain(&author.sign_event(
            space,
            "created",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Offline messages".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?)?,
        &mut head,
    )?;
    assert_eq!(engine.ingest(genesis), Ingest::Applied);
    engine.create_mls_group(space).unwrap();

    // A second enrolled leaf of the same identity can read both offline messages
    // and supplies a genuine MLS commit that this session has not caught up with.
    let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate()?);
    let mut peer_store = rusqlite::Connection::open_in_memory().unwrap();
    ::mls::migrate(&mut peer_store).unwrap();
    let peer_secret = peer_author.key.secret();
    let peer = Device::new(
        &peer_store,
        state_key(&peer_secret)?,
        &peer_author.identity,
        peer_secret,
        peer_author.cert.as_deref().unwrap(),
    )
    .unwrap();
    let packages = peer.key_packages(1, false).unwrap();
    let adding = Device::new(
        engine.store.conn(),
        state_key(&device_secret)?,
        &author.identity,
        device_secret,
        author.cert.as_deref().unwrap(),
    )
    .unwrap()
    .commit(space, &packages, &Default::default())
    .unwrap();
    let commit = sequence(
        Envelope::sealed(
            &author,
            space,
            "add-peer",
            2,
            head.as_ref(),
            Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
        )?,
        &mut head,
    )?;
    assert_eq!(engine.ingest(commit), Ingest::Applied);
    let roster = [author.identity.clone()].into_iter().collect();
    let welcome = adding.welcome.unwrap();
    assert!(peer.join(space, &welcome, &roster).unwrap());
    let welcome = sequence(
        Envelope::sealed(
            &author,
            space,
            "welcome-peer",
            3,
            head.as_ref(),
            Sealed::new(SealedKind::Welcome, SUITE_ID, welcome),
        )?,
        &mut head,
    )?;
    assert_eq!(engine.ingest(welcome), Ingest::Applied);
    assert_eq!(engine.mls_status(space).unwrap().0, 1);

    let cached = engine
        .append_synced(
            space,
            &author,
            100,
            EventBody::MessagePosted {
                message: "cached-message".into(),
                text: "Written offline, already sealed".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    assert!(engine.mls_seal_outbox().unwrap());
    let cached_key = format!("mls.sealed:{}", cached.client_id);
    let cached_copy = engine.store.meta(&cached_key).unwrap().unwrap();
    let fresh = engine
        .append_synced(
            space,
            &author,
            101,
            EventBody::MessagePosted {
                message: "plaintext-message".into(),
                text: "Written offline, still plaintext".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )
        .unwrap();
    let fresh_key = format!("mls.sealed:{}", fresh.client_id);
    assert!(engine.store.meta(&fresh_key).unwrap().is_none());
    let advance = peer.commit(space, &[], &Default::default()).unwrap();
    assert!(matches!(
        peer.open(space, &advance.commit, &roster, peer.leaf())
            .unwrap(),
        Opened::Commit { epoch: 2 }
    ));
    let missed_commit = sequence(
        Envelope::sealed(
            &peer_author,
            space,
            "missed-commit",
            102,
            head.as_ref(),
            Sealed::new(SealedKind::Commit, SUITE_ID, advance.commit),
        )?,
        &mut head,
    )?;
    let losing_commit = if matches!(
        catch_up,
        CatchUp::HandshakeCollision | CatchUp::StorageRefusal
    ) {
        engine.commit_now(space, &[], &Default::default()).unwrap();
        Some(
            engine
                .store
                .outbox_handshakes()
                .unwrap()
                .into_iter()
                .find(|(_, _, kind)| kind == SealedKind::Commit.name())
                .unwrap()
                .0,
        )
    } else {
        None
    };
    if matches!(catch_up, CatchUp::StorageRefusal) {
        engine.store.conn().execute_batch("CREATE TRIGGER fail_refusal BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT, 'injected handshake storage failure'); END;").unwrap();
    }
    let notices = Arc::new(RejectionListener::default());
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    if matches!(catch_up, CatchUp::TransientOrder) {
        let poke = cmd.clone();
        lock(&engine).net.poke = Some(Box::new(move || {
            let _ = poke.send(Cmd::Flush);
        }));
    }
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let (maintenance, mut maintenance_started) = mpsc::unbounded_channel();
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: Some(notices.clone()),
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: Some(Arc::new(move || {
            let _ = maintenance.send(());
        })),
    };
    let (client_stopped, stopped) = oneshot::channel();
    let client = async {
        let mut backoff = Duration::from_millis(500);
        assert!(matches!(
            session(&ctx, &mut commands, &mut backoff).await,
            Exit::Stop
        ));
        let _ = client_stopped.send(());
    };
    let relay = async {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
            panic!("hello")
        };
        reply(
            &mut socket,
            ServerFrame::Challenge {
                nonce: "catch-up-test".into(),
                relay: "catch-up-test".into(),
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
        let mut current_copies = HashMap::new();
        let mut held_commit = None;
        if matches!(
            catch_up,
            CatchUp::HandshakeCollision | CatchUp::StorageRefusal
        ) {
            held_commit = queue_phases::collision(
                &mut socket,
                &catch_up,
                &engine,
                &notices,
                space,
                &losing_commit,
                &missed_commit,
            )
            .await;
        }
        if matches!(catch_up, CatchUp::PermissionRefusal) {
            reply(
                &mut socket,
                ServerFrame::Event {
                    ev: missed_commit.clone(),
                },
            )
            .await;
            reply(&mut socket, ServerFrame::SyncDone).await;
        }
        if matches!(catch_up, CatchUp::Incremental) {
            queue_phases::incremental(
                &mut socket,
                &engine,
                &ctx,
                queue_phases::QueuedPair {
                    space,
                    cached: &cached,
                    fresh: &fresh,
                    cached_key: &cached_key,
                    fresh_key: &fresh_key,
                },
                &missed_commit,
                &mut maintenance_started,
                &mut current_copies,
            )
            .await;
        }
        if matches!(catch_up, CatchUp::Initial | CatchUp::TransientOrder) {
            queue_phases::initial(
                &mut socket,
                &engine,
                &cmd,
                space,
                (&cached_key, &fresh_key, cached_copy),
                missed_commit,
                &mut maintenance_started,
            )
            .await;
        }
        let mut readable = HashSet::new();
        let mut transient_copy = None;
        while readable.len() < 2 {
            match request(&mut socket).await {
                ClientFrame::Publish { env }
                    if env.sealed_kind() == Some(SealedKind::Application) =>
                {
                    assert!(ctx.status.borrow().synced);
                    let original = [&cached, &fresh]
                        .into_iter()
                        .find(|event| event.client_id == env.client_id())
                        .expect("only the two queued messages are published");
                    if matches!(catch_up, CatchUp::Incremental) {
                        assert_eq!(
                            current_copies.get(env.client_id()),
                            Some(&env),
                            "a late rejection must preserve the newer sealed copy"
                        );
                    }
                    if matches!(catch_up, CatchUp::PermissionRefusal)
                        && env.client_id() == cached.client_id
                    {
                        assert!(readable.insert(env.client_id().to_string()));
                        reply(
                            &mut socket,
                            ServerFrame::Rejected {
                                space: space.into(),
                                client_id: env.client_id().into(),
                                reason: "permission denied".into(),
                                permanent: true,
                            },
                        )
                        .await;
                        continue;
                    }
                    if matches!(catch_up, CatchUp::TransientOrder) {
                        if env.client_id() == cached.client_id {
                            if let Some(original) = &transient_copy {
                                assert_eq!(
                                    &env, original,
                                    "retry keeps the exact ciphertext and CID"
                                );
                            } else {
                                transient_copy = Some(env.clone());
                                reply(
                                    &mut socket,
                                    ServerFrame::Rejected {
                                        space: space.into(),
                                        client_id: env.client_id().into(),
                                        reason: "transient rate-limit refusal".into(),
                                        permanent: false,
                                    },
                                )
                                .await;
                                continue;
                            }
                        } else {
                            assert!(readable.contains(&cached.client_id),
                                "a newer queued message overtook its transiently refused predecessor");
                        }
                    }
                    let (_, bytes) = env.sealed_data().unwrap();
                    let epoch = ::mls::application_epoch(bytes);
                    if epoch == Some(1)
                        && matches!(
                            catch_up,
                            CatchUp::HandshakeCollision | CatchUp::StorageRefusal
                        )
                    {
                        // `original` above restricts this to the same two queued IDs.
                        // A sent copy can precede catch-up even after its server reply.
                        reply(
                            &mut socket,
                            ServerFrame::Rejected {
                                space: space.into(),
                                client_id: env.client_id().into(),
                                reason: protocol::STALE_SEAL.into(),
                                permanent: true,
                            },
                        )
                        .await;
                        if let Some(ev) = held_commit.take() {
                            reply(&mut socket, ServerFrame::Event { ev }).await;
                        }
                        continue;
                    }
                    assert_eq!(epoch, Some(2));
                    let Opened::Application { plaintext, .. } = peer
                        .open(
                            space,
                            bytes,
                            &roster,
                            &::mls::Leaf {
                                identity: env.author().into(),
                                device: env.device().unwrap().into(),
                                cert: env.cert.clone().unwrap(),
                            },
                        )
                        .unwrap()
                    else {
                        panic!("the peer must read the message at the caught-up epoch")
                    };
                    let inner = InnerEvent::decode(&plaintext).unwrap();
                    assert_eq!(inner.content, original.content);
                    assert_eq!(inner.sig, original.sig);
                    assert!(readable.insert(env.client_id().to_string()));
                    let ev = sequence(env, &mut head)?;
                    reply(&mut socket, ServerFrame::Event { ev }).await;
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
                    .await
                }
                ClientFrame::Publish { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        if matches!(catch_up, CatchUp::TransientOrder) {
            assert!(
                transient_copy.is_some(),
                "the transient refusal was exercised"
            );
        }
        assert!(held_commit.is_none(), "the old-epoch gap was not exercised");
        // A response ordered after both event echoes confirms that the client
        // applied them and cleared their pending records before stopping.
        let (marker, marked) = oneshot::channel();
        let marker_box = "22".repeat(32);
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: marker_box.clone(),
            },
            reply: marker,
        })
        .unwrap();
        loop {
            match request(&mut socket).await {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: link },
                } if link == marker_box => {
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
                ClientFrame::Ping => {
                    reply(
                        &mut socket,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await
                }
                ClientFrame::Publish { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(matches!(marked.await.unwrap(), Ok(Reply::Link(None))));
        {
            let engine = lock(&engine);
            assert!(!engine.net.pending.contains_key(&cached.client_id));
            assert!(!engine.net.pending.contains_key(&fresh.client_id));
            engine.logs[space].verify().unwrap();
            let errors = notices.errors.lock().unwrap();
            if matches!(catch_up, CatchUp::PermissionRefusal) {
                assert_eq!(errors.as_slice(), ["Could not send. Try again."]);
                assert!(engine.store.outbox().unwrap().iter().any(|p| p.failed
                    && p.event.client_id == cached.client_id
                    && p.last_error.as_deref() == Some("permission denied")));
            } else if matches!(catch_up, CatchUp::StorageRefusal) {
                assert_eq!(errors.as_slice(), ["Could not finish syncing. Try again."]);
                assert_eq!(engine.outbox_len(), 0);
            } else {
                assert!(
                    errors.is_empty(),
                    "handled reconciliation is not a user error: {errors:?}"
                );
                assert_eq!(engine.outbox_len(), 0);
            }
        }
        cmd.send(Cmd::Stop).unwrap();
        stopped.await.unwrap();
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (_, produced) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("catch-up ordering stalled");
    produced?;

    Ok(())
}
