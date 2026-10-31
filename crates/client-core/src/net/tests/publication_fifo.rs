//! One real transport reconnects after an accepted effect loses its echo.
use super::*;

type R<T> = Result<T, Box<dyn std::error::Error>>;

pub(super) async fn next_publish(socket: &mut RelaySocket) -> R<Envelope> {
    loop {
        match request(socket).await {
            ClientFrame::Publish { env } => return Ok(env),
            ClientFrame::Req { id, .. } => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await;
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await;
            }
            frame => return Err(format!("unexpected before publication: {frame:?}").into()),
        }
    }
}

/// A FIFO command/response marker proves the offer pass ended without sleeping
/// to infer that no later message was emitted. Its response follows prior echoes.
pub(super) async fn marker(
    socket: &mut RelaySocket,
    cmd: &mpsc::UnboundedSender<Cmd>,
    id: &str,
) -> R<Vec<Envelope>> {
    let (answer, answered) = oneshot::channel();
    cmd.send(Cmd::Flush).map_err(|_| "flush receiver closed")?;
    cmd.send(Cmd::Req {
        op: Op::FetchLink { id: id.into() },
        reply: answer,
    })
    .map_err(|_| "marker receiver closed")?;
    let mut published = Vec::new();
    loop {
        match request(socket).await {
            ClientFrame::Publish { env } => published.push(env),
            ClientFrame::Req {
                id: request_id,
                op: Op::FetchLink { id: target },
            } if target == id => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id: request_id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await;
                break;
            }
            ClientFrame::Req { id, .. } => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await;
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await;
            }
            frame => return Err(format!("unexpected before marker: {frame:?}").into()),
        }
    }
    assert!(matches!(answered.await?, Ok(Reply::Link(None))));
    Ok(published)
}

pub(super) async fn unsettled_space_does_not_block_another_and_reconnect_recovers_lost_echo(
) -> R<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("ws://{}", listener.local_addr()?);
    let mut engine = Engine::open(":memory:")?;
    engine.create_account("Ana", "ana", &url)?;
    engine.set_registered(true)?;
    let _ = engine
        .mls_key_packages_to_publish()
        .ok_or("initial key packages")?;
    engine.mls_key_packages_published(Ok(()));
    let author = engine.net.author.clone().ok_or("fixture author")?;
    let mut heads = HashMap::new();
    for space in ["fifo-a", "fifo-b"] {
        let mut head = None;
        let genesis = sequence(
            Envelope::plain(&author.sign_event(
                space,
                space,
                1,
                None,
                EventBody::SpaceCreated {
                    title: space.into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )?)?,
            &mut head,
        )?;
        assert_eq!(engine.ingest(genesis), Ingest::Applied);
        heads.insert(space.to_string(), head);
    }
    let queued = [
        ("fifo-a", "a1"),
        ("fifo-a", "a2"),
        ("fifo-b", "b1"),
        ("fifo-a", "a3"),
        ("fifo-b", "b2"),
    ]
    .into_iter()
    .map(|(space, text)| {
        engine
            .append_synced(
                space,
                &author,
                100,
                EventBody::MessagePosted {
                    message: text.into(),
                    text: text.into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                    forwarded_author: None,
                },
            )
            .map_err(Into::into)
    })
    .collect::<R<Vec<_>>>()?;
    let original = engine.outbox_envelopes();
    assert_eq!(original.len(), queued.len());
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, commands) = mpsc::unbounded_channel();
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
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let first = next_publish(&mut socket).await?;
        assert_eq!(first, original[0]);
        let others = marker(&mut socket, &cmd, &"55".repeat(32)).await?;
        assert_eq!(
            others,
            [original[2].clone()],
            "one unsettled signed head per Space; independent Spaces share the same pass"
        );

        // Space A's first effect is canonical at the relay, but neither its
        // Accepted notice nor Event echo reaches this connection.
        let lost = sequence(first, heads.get_mut("fifo-a").ok_or("A head")?)?;
        let b1 = sequence(others[0].clone(), heads.get_mut("fifo-b").ok_or("B head")?)?;
        reply(&mut socket, ServerFrame::Event { ev: b1.clone() }).await;
        let b2 = next_publish(&mut socket).await?;
        assert_eq!(
            b2, original[4],
            "B progresses while A still has no confirmation"
        );
        let b2 = sequence(b2, heads.get_mut("fifo-b").ok_or("B head")?)?;
        reply(&mut socket, ServerFrame::Event { ev: b2.clone() }).await;
        assert!(
            marker(&mut socket, &cmd, &"66".repeat(32))
                .await?
                .is_empty(),
            "flush/maintenance cannot release A2 merely because another Space advanced"
        );
        {
            let engine = lock(&engine);
            assert_eq!(
                engine.store.outbox_get(&queued[0].client_id)?,
                Some(queued[0].clone())
            );
            assert_eq!(
                engine.store.outbox_get(&queued[1].client_id)?,
                Some(queued[1].clone())
            );
        }
        socket.close(None).await?;
        drop(socket);

        // The normal run loop reconnects. Ordered catch-up reconciles the lost
        // effect before another offer; the original CID is never replaced.
        let mut socket = catch_up_socket(&listener).await;
        for ev in [lost, b1, b2] {
            reply(&mut socket, ServerFrame::Event { ev }).await;
        }
        reply(&mut socket, ServerFrame::SyncDone).await;
        for index in [1, 3] {
            let env = next_publish(&mut socket).await?;
            assert_eq!(
                env, original[index],
                "reconnect preserves bytes and queue order"
            );
            let ev = sequence(env, heads.get_mut("fifo-a").ok_or("A head")?)?;
            reply(&mut socket, ServerFrame::Event { ev }).await;
        }
        assert!(
            marker(&mut socket, &cmd, &"77".repeat(32))
                .await?
                .is_empty(),
            "no accepted effect is emitted again after catch-up or final confirmation"
        );
        let engine = lock(&engine);
        assert_eq!(engine.outbox_len(), 0);
        for (space, expected) in [
            ("fifo-a", vec!["a1", "a2", "a3"]),
            ("fifo-b", vec!["b1", "b2"]),
        ] {
            let log = engine.logs.get(space).ok_or("confirmed log")?;
            let seen = log
                .events()
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::MessagePosted { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                seen, expected,
                "canonical history retains each sender's actual order"
            );
            log.verify()?;
        }
        drop(engine);
        cmd.send(Cmd::Stop).map_err(|_| "stop receiver closed")?;
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (_, outcome) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(run(ctx, commands), relay)
    })
    .await?;
    outcome
}
