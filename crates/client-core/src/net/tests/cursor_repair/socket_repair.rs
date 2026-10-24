//! Preserved socket repair scenarios; stable test entries stay in the parent.
use super::*;

pub(super) async fn healthy_socket_recovers_final_unannounced_event_without_reconnect_or_followup(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("final-unannounced")?));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("reader.sqlite");
    let canonical_path = directory.0.join("canonical.sqlite");
    let root = Signer::from_secret(&[121; 32]);
    let device = Signer::from_secret(&[122; 32]);
    let peer = Author::root(Signer::from_secret(&[123; 32]));
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    for (id, name) in [(root.id(), "Owner"), (peer.identity.clone(), "Peer")] {
        engine
            .store
            .put_identity(
                &Identity {
                    id,
                    kind: IdentityKind::Person,
                    name: name.into(),
                    handle: name.to_lowercase(),
                    tint_hex: "#246844".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: "Synthetic final-wake fixture".into(),
                },
                None,
            )
            .unwrap();
    }
    engine
        .install_restored_account(&root, device.clone(), &url)
        .unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let owner = engine.net.author.clone().unwrap();
    let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
    let mut head = None;
    for (id, body) in [
        (
            "repair-create",
            EventBody::SpaceCreated {
                title: "Final wake".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "repair-member",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let ev = sequence(
            Envelope::plain(&owner.sign_event(SPACE, id, AT, head.clone(), body)),
            &mut head,
        );
        canonical.append_event(&ev.plain_event().unwrap()).unwrap();
        assert_eq!(engine.ingest(ev), Ingest::Applied);
    }
    let missing = sequence(
        Envelope::plain(&peer.sign_event(
            SPACE,
            EVENT,
            AT + 2,
            head.clone(),
            EventBody::MessagePosted {
                message: "body-final-unannounced".into(),
                text: TEXT.into(),
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
    let (finished, closed) = oneshot::channel();
    let server = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let mut marks = Vec::new();
        barrier(&cmd, &mut socket, "initial-complete", &mut marks).await;
        assert!(ctx.status.borrow().synced);
        assert_eq!(lock(&engine).next_seq(SPACE), 2);
        // Persist ONLY after initial catch-up has completed; drop the sole live frame.
        canonical
            .append_event(&missing.plain_event().unwrap())
            .unwrap();
        drop(canonical);
        let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.events(SPACE).unwrap().len(), 3);
        // No Event, Joined, later content or explicit refresh is sent. Only
        // transport liveness continues while the durable row is unannounced.
        let mut heartbeats = tokio::time::interval(Duration::from_millis(20));
        let repair = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    frame = request(&mut socket) => match frame {
                        ClientFrame::Sync { cursors, all } => break (cursors, all),
                        other => ordinary(&mut socket, other, &mut marks).await,
                    },
                    _ = heartbeats.tick() => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                }
            }
        }).await.expect("healthy Pongs must not hide a completely lost final live frame");
        assert!(
            repair.1,
            "repair includes current Spaces even if Joined itself was lost"
        );
        assert_eq!(repair.0.len(), 1);
        assert_eq!(repair.0[0].space, SPACE);
        assert_eq!(repair.0[0].next_seq, 2);
        assert!(marks.iter().all(|seq| *seq <= 1));
        let mut forged = missing.clone();
        let replacement = if forged.env.sig.starts_with('0') {
            "1"
        } else {
            "0"
        };
        forged.env.sig.replace_range(..1, replacement);
        reply(&mut socket, ServerFrame::Event { ev: forged }).await;
        barrier(&cmd, &mut socket, "rejected-repair-content", &mut marks).await;
        assert_eq!(
            lock(&engine).next_seq(SPACE),
            2,
            "repair never bypasses signature admission"
        );
        assert_eq!(lock(&engine).store.events(SPACE).unwrap().len(), 2);
        assert!(marks.iter().all(|seq| *seq <= 1));
        // Serve the actual retained row twice: overlap cannot duplicate content.
        let row = canonical.events(SPACE).unwrap().pop().unwrap();
        let recovered = Sequenced {
            seq: row.seq,
            prev: row.prev.clone(),
            hash: row.hash.clone(),
            env: Envelope::plain(&row),
        };
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: recovered.clone(),
            },
        )
        .await;
        reply(&mut socket, ServerFrame::Event { ev: recovered }).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        barrier(&cmd, &mut socket, "repaired-content", &mut marks).await;
        let expected = assert_history(&lock(&engine), &peer.identity);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marks.contains(&2) {
                let frame = request(&mut socket).await;
                match frame {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert_eq!(cursors.len(), 1);
                        assert_eq!(cursors[0].space, SPACE);
                        assert_eq!(cursors[0].next_seq, 3);
                        reply(&mut socket, ServerFrame::SyncDone).await;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
        })
        .await
        .expect("processed readable content must eventually announce its durable cursor");
        assert_eq!(marks.iter().copied().max(), Some(2));
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
        // The client used this one socket: no reconnect can conceal the missing wake.
        assert!(futures_util::FutureExt::now_or_never(listener.accept()).is_none());
        expected
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, expected) = tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(client, server)
    })
    .await
    .expect("final lost-wake recovery stalled");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device.secret().to_vec())).unwrap());
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
    assert_eq!(reopened.ingest(missing), Ingest::Duplicate);
    assert_eq!(assert_history(&reopened, &peer.identity), expected);

    Ok(())
}

pub(super) async fn healthy_socket_discovers_new_authorized_space_when_joined_and_all_live_events_are_lost(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("new-space-lost-joined")?));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("reader.sqlite");
    let canonical_path = directory.0.join("canonical.sqlite");
    let root = Signer::from_secret(&[124; 32]);
    let device = Signer::from_secret(&[125; 32]);
    let peer = Author::root(Signer::from_secret(&[126; 32]));
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    for (id, name) in [(root.id(), "Owner"), (peer.identity.clone(), "Peer")] {
        engine
            .store
            .put_identity(
                &Identity {
                    id,
                    kind: IdentityKind::Person,
                    name: name.into(),
                    handle: name.to_lowercase(),
                    tint_hex: "#246844".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: "Synthetic missing-Joined fixture".into(),
                },
                None,
            )
            .unwrap();
    }
    engine
        .install_restored_account(&root, device.clone(), &url)
        .unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    assert!(
        engine.cursors().is_empty(),
        "the initial full Sync has no known Space"
    );
    assert!(engine.store.events(SPACE).unwrap().is_empty());
    let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
    assert!(canonical.events(SPACE).unwrap().is_empty());
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
        // Reuse the same non-content request barrier, but permit the legitimate
        // empty-cursor periodic scan if this initial handshake is scheduled slowly.
        let (answer_to, answer) = oneshot::channel();
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: "new-space-initial-complete".into(),
            },
            reply: answer_to,
        })
        .unwrap();
        loop {
            let frame = request(&mut socket).await;
            match &frame {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: link },
                } if link == "new-space-initial-complete" => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id: *id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Sync { cursors, all } => {
                    assert!(*all);
                    assert!(cursors.is_empty());
                    reply(&mut socket, ServerFrame::SyncDone).await;
                }
                _ => ordinary(&mut socket, frame, &mut marks).await,
            }
        }
        assert!(matches!(answer.await.unwrap(), Ok(Reply::Link(None))));
        assert!(ctx.status.borrow().synced);
        assert!(lock(&engine).cursors().is_empty());
        assert!(marks.is_empty());
        // Only now does a new durable room exist. Peer creates it and grants
        // this reader membership; no local create/ingest/outbox or refresh wakes it.
        let mut head = None;
        for (id, body) in [
            (
                "repair-create",
                EventBody::SpaceCreated {
                    title: "Missing Joined".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            ),
            (
                "repair-member",
                EventBody::MemberAdded {
                    identity: root.id(),
                    role: Role::Member,
                },
            ),
        ] {
            let event = sequence(
                Envelope::plain(&peer.sign_event(SPACE, id, AT, head.clone(), body)),
                &mut head,
            );
            canonical
                .append_event(&event.plain_event().unwrap())
                .unwrap();
        }
        let missing = sequence(
            Envelope::plain(&peer.sign_event(
                SPACE,
                EVENT,
                AT + 2,
                head.clone(),
                EventBody::MessagePosted {
                    message: "body-final-unannounced".into(),
                    text: TEXT.into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )),
            &mut head,
        );
        canonical
            .append_event(&missing.plain_event().unwrap())
            .unwrap();
        drop(canonical);
        let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.events(SPACE).unwrap().len(), 3);
        assert!(lock(&engine).store.events(SPACE).unwrap().is_empty());
        assert!(lock(&engine).cursors().is_empty());
        // Suppress BOTH Joined and every live Event for this new room. There is
        // no later content or explicit refresh: only normal healthy transport.
        let mut heartbeats = tokio::time::interval(Duration::from_millis(20));
        let (cursors, all) = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    frame = request(&mut socket) => match frame {
                        ClientFrame::Sync { cursors, all } => break (cursors, all),
                        other => ordinary(&mut socket, other, &mut marks).await,
                    },
                    _ = heartbeats.tick() => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                }
            }
        }).await.expect("healthy Pongs must not conceal a new Space when Joined and every live Event are lost");
        assert!(
            all,
            "unknown authorized Spaces require the full membership scan"
        );
        assert!(
            cursors.is_empty(),
            "no known cursor can name the completely missed room"
        );
        assert!(
            marks.is_empty(),
            "no delivery watermark exists before the room is ingested"
        );
        // This is the mock relay's authorized full-scan result, backed by its
        // actual reopened SQLite rows. Real membership enforcement is separate.
        for row in canonical.events(SPACE).unwrap() {
            let recovered = Sequenced {
                seq: row.seq,
                prev: row.prev.clone(),
                hash: row.hash.clone(),
                env: Envelope::plain(&row),
            };
            reply(
                &mut socket,
                ServerFrame::Event {
                    ev: recovered.clone(),
                },
            )
            .await;
            reply(&mut socket, ServerFrame::Event { ev: recovered }).await;
        }
        reply(&mut socket, ServerFrame::SyncDone).await;
        barrier(&cmd, &mut socket, "repaired-content", &mut marks).await;
        let expected = assert_history(&lock(&engine), &peer.identity);
        assert!(
            lock(&engine).is_member(SPACE, &root.id()),
            "the real projection applied the signed membership grant"
        );
        let current = lock(&engine).cursors();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].space, SPACE);
        assert_eq!(current[0].next_seq, 3);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marks.contains(&2) {
                match request(&mut socket).await {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert_eq!(cursors.len(), 1);
                        assert_eq!(cursors[0].space, SPACE);
                        assert_eq!(cursors[0].next_seq, 3);
                        reply(&mut socket, ServerFrame::SyncDone).await;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
        })
        .await
        .expect("newly discovered readable content must announce its durable cursor");
        assert_eq!(marks.iter().copied().max(), Some(2));
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
        assert!(
            futures_util::FutureExt::now_or_never(listener.accept()).is_none(),
            "recovery used the original healthy socket"
        );
        (expected, missing)
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, (expected, missing)) = tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(client, server)
    })
    .await
    .expect("new Space discovery stalled without its lost Joined notification");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device.secret().to_vec())).unwrap());
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
    assert_eq!(reopened.ingest(missing), Ingest::Duplicate);
    assert_eq!(assert_history(&reopened, &peer.identity), expected);

    Ok(())
}
