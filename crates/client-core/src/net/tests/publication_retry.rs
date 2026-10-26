//! A refused original waits without stopping ordered inbound dispatch.
use super::publication_fifo::{marker, next_publish};
use super::*;

type R<T> = Result<T, Box<dyn std::error::Error>>;

enum Recovery {
    Live,
    ReconnectAndLostEcho,
}

/// A ready task prevents Tokio from automatically advancing paused time while
/// the real socket waits for I/O. Only the explicit advances below move time.
async fn bounded(work: impl std::future::Future<Output = R<()>>) -> R<()> {
    let _awake = AbortOnDrop(tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    }));
    let (cancel, canceled) = channel::channel();
    let (expired, elapsed) = oneshot::channel();
    let watchdog = std::thread::spawn(move || {
        if canceled.recv_timeout(Duration::from_secs(10)).is_err() {
            let _ = expired.send(());
        }
    });
    let result = tokio::select! {
        result = work => result,
        _ = elapsed => Err("publication retry fixture exceeded real-wall watchdog".into()),
    };
    let _ = cancel.send(());
    watchdog.join().map_err(|_| "retry watchdog panicked")?;
    result
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn hinted_publication_wait_survives_reconnect_and_lost_confirmation() -> R<()> {
    bounded(exercise(
        Duration::from_secs(12),
        "slow down, retry in 12.0 s",
        Recovery::ReconnectAndLostEcho,
    ))
    .await
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unhinted_publication_wait_keeps_dispatch_live_for_half_a_second() -> R<()> {
    bounded(exercise(
        Duration::from_millis(500),
        "temporarily unavailable",
        Recovery::Live,
    ))
    .await
}

async fn exercise(minimum: Duration, reason: &str, recovery: Recovery) -> R<()> {
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
    // The real Net installs a poke; retain that baseline dispatch behavior.
    engine.net.poke = Some(Box::new(|| {}));
    let author = engine.net.author.clone().ok_or("fixture author")?;
    let mut heads = HashMap::new();
    for space in ["retry-a", "retry-b"] {
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
        ("retry-a", "a1"),
        ("retry-a", "a2"),
        ("retry-b", "b1"),
        ("retry-b", "b2"),
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
                },
            )
            .map_err(Into::into)
    })
    .collect::<R<Vec<_>>>()?;
    let original = engine.outbox_envelopes();
    assert_eq!(original.len(), queued.len());
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, commands) = mpsc::unbounded_channel();
    let (status, mut connection) = watch::channel(NetStatus {
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
        assert_eq!(
            marker(&mut socket, &cmd, &"81".repeat(32)).await?,
            [original[2].clone()]
        );
        let refused_at = tokio::time::Instant::now();
        reply(
            &mut socket,
            ServerFrame::Rejected {
                space: first.space().into(),
                client_id: first.client_id().into(),
                reason: reason.into(),
                permanent: false,
            },
        )
        .await;
        if matches!(recovery, Recovery::ReconnectAndLostEcho) {
            // Observe the actual retained refusal before advancing, so the
            // baseline's inline 500ms sleep has definitely started. This
            // produces a concrete early retry failure, not a timeout oracle.
            loop {
                let refused = lock(&engine).store.outbox_heads()?.into_iter().any(|head| {
                    head.client_id == first.client_id()
                        && head.last_error.as_deref() == Some(reason)
                });
                if refused {
                    break;
                }
                tokio::task::yield_now().await;
            }
            tokio::time::advance(Duration::from_millis(500)).await;
        }
        let dispatch_at = tokio::time::Instant::now();
        let b1 = sequence(
            original[2].clone(),
            heads.get_mut("retry-b").ok_or("B head")?,
        )?;
        reply(&mut socket, ServerFrame::Event { ev: b1.clone() }).await;
        let mut b_progress = marker(&mut socket, &cmd, &"82".repeat(32)).await?;
        // The first request may be written before B1 is read. Its reply follows
        // B1 on the inbound socket; a second roundtrip collects every offer
        // released by that confirmation before making the cross-Space claim.
        b_progress.extend(marker(&mut socket, &cmd, &"88".repeat(32)).await?);
        assert_eq!(
            tokio::time::Instant::now(),
            dispatch_at,
            "ordered confirmation and marker must finish without advancing retry time"
        );
        assert!(
            b_progress
                .iter()
                .all(|env| env.client_id() != first.client_id()),
            "the refused original retried before its relay minimum: {b_progress:?}"
        );
        assert_eq!(
            b_progress,
            [original[3].clone()],
            "the refused original fences its own successors while another Space advances"
        );
        let b2 = sequence(
            b_progress[0].clone(),
            heads.get_mut("retry-b").ok_or("B head")?,
        )?;
        reply(&mut socket, ServerFrame::Event { ev: b2.clone() }).await;
        assert!(marker(&mut socket, &cmd, &"83".repeat(32))
            .await?
            .is_empty());
        {
            let engine = lock(&engine);
            assert_eq!(engine.outbox_len(), 2);
            assert_eq!(
                engine.store.outbox_get(&queued[0].client_id)?,
                Some(queued[0].clone())
            );
            assert_eq!(
                engine.store.outbox_get(&queued[1].client_id)?,
                Some(queued[1].clone())
            );
        }

        if matches!(recovery, Recovery::ReconnectAndLostEcho) {
            socket.close(None).await?;
            drop(socket);
            while connection.borrow().state != ConnState::Offline {
                connection.changed().await?;
            }
            // Normal reconnect's existing jitter is at most 1010ms.
            tokio::time::advance(Duration::from_secs(2)).await;
            socket = catch_up_socket(&listener).await;
            reply(&mut socket, ServerFrame::SyncDone).await;
            assert!(
                marker(&mut socket, &cmd, &"84".repeat(32))
                    .await?
                    .is_empty(),
                "a new socket must preserve the original retry minimum"
            );
        }
        let before_deadline = refused_at + minimum - Duration::from_millis(1);
        tokio::time::advance(before_deadline - tokio::time::Instant::now()).await;
        assert!(
            marker(&mut socket, &cmd, &"85".repeat(32))
                .await?
                .is_empty(),
            "flush and maintenance cannot publish one millisecond before the minimum"
        );
        tokio::time::advance(Duration::from_millis(1)).await;
        let retry = next_publish(&mut socket).await?;
        assert_eq!(
            retry, original[0],
            "retry keeps the exact original envelope"
        );
        assert_eq!(tokio::time::Instant::now(), refused_at + minimum);
        let confirmed = sequence(retry, heads.get_mut("retry-a").ok_or("A head")?)?;

        if matches!(recovery, Recovery::ReconnectAndLostEcho) {
            // This retry committed, but both Accepted and the echo were lost.
            assert!(marker(&mut socket, &cmd, &"86".repeat(32))
                .await?
                .is_empty());
            socket.close(None).await?;
            drop(socket);
            while connection.borrow().state != ConnState::Offline {
                connection.changed().await?;
            }
            tokio::time::advance(Duration::from_secs(2)).await;
            socket = catch_up_socket(&listener).await;
            reply(&mut socket, ServerFrame::Event { ev: confirmed }).await;
            reply(&mut socket, ServerFrame::Event { ev: b1 }).await;
            reply(&mut socket, ServerFrame::Event { ev: b2 }).await;
            reply(&mut socket, ServerFrame::SyncDone).await;
        } else {
            reply(&mut socket, ServerFrame::Event { ev: confirmed }).await;
        }
        let successor = next_publish(&mut socket).await?;
        assert_eq!(
            successor, original[1],
            "confirmation releases the exact successor, never a duplicate or replacement"
        );
        let successor = sequence(successor, heads.get_mut("retry-a").ok_or("A head")?)?;
        reply(&mut socket, ServerFrame::Event { ev: successor }).await;
        assert!(marker(&mut socket, &cmd, &"87".repeat(32))
            .await?
            .is_empty());
        let engine = lock(&engine);
        assert_eq!(engine.outbox_len(), 0);
        for (space, expected) in [("retry-a", ["a1", "a2"]), ("retry-b", ["b1", "b2"])] {
            let log = engine.logs.get(space).ok_or("confirmed log")?;
            let seen = log
                .events()
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::MessagePosted { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(seen, expected);
            log.verify()?;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let relay = async {
        let result = relay.await;
        let _ = cmd.send(Cmd::Stop);
        result
    };
    let (_, result) = tokio::join!(run(ctx, commands), relay);
    result
}
