//! inactivity cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn initial_progress_does_not_revive_expiry_or_extend_completed_periodic_work() {
    let now = tokio::time::Instant::now();
    let policy = policy();
    let mut repair = CursorRepair::new(policy, now);
    assert_eq!(
        repair.abort_deadline(),
        Some(now + policy.initial_inactivity)
    );
    assert_eq!(repair.admission(), None);
    assert!(repair.submitted(), "a gap can overlap the initial scan");
    assert!(repair.progressed(now + Duration::from_millis(100)));
    let renewed = now + Duration::from_millis(100) + policy.initial_inactivity;
    assert_eq!(repair.abort_deadline(), Some(renewed));
    assert!(
        !repair.progressed(renewed),
        "an expired owner cannot revive"
    );
    assert_eq!(repair.abort_deadline(), Some(renewed));
    assert!(repair.abandon_transport(false, renewed));
    // The first ordered SyncDone belongs to the full initial scan even with a
    // partial request outstanding, then the ordinary periodic policy takes over.
    repair.completed(renewed);
    let periodic = renewed + policy.interval + policy.timeout;
    assert_eq!(repair.abort_deadline(), Some(periodic));
    assert!(!repair.progressed(renewed + Duration::from_millis(1)));
    assert_eq!(repair.abort_deadline(), Some(periodic));
    repair.completed(renewed + Duration::from_millis(2));
    assert_eq!(repair.abort_deadline(), Some(periodic));
}

#[tokio::test(flavor = "current_thread")]
pub(super) async fn duplicate_historical_gap_invalid_and_ephemeral_traffic_cannot_renew_initial_sync(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (mut engine, _, author, mut head, genesis) = pending_engine(&url, ":memory:")?;
    let mut other_head = None;
    let historical = sequence(
        Envelope::plain(&author.sign_event(
            "sp_already_caught_up",
            "other-initial-create",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Historical cross-Space traffic".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )?)?,
        &mut other_head,
    )?;
    assert_eq!(engine.ingest(historical.clone()), Ingest::Applied);
    let next = sequence(
        Envelope::plain(&author.sign_event(
            SPACE,
            "never-valid",
            2,
            head.clone(),
            EventBody::MessagePosted {
                message: "untrusted-body".into(),
                text: "Untrusted".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?)?,
        &mut head,
    )?;
    let mut invalid = next;
    let replacement = if invalid.env.sig.starts_with('0') {
        "1"
    } else {
        "0"
    };
    invalid.env.sig.replace_range(..1, replacement);
    let gap = sequence(
        Envelope::plain(&author.sign_event(
            SPACE,
            "future-gap",
            3,
            head.clone(),
            EventBody::MessagePosted {
                message: "future-body".into(),
                text: "Not a prefix".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?)?,
        &mut head,
    )?;
    let engine = Arc::new(Mutex::new(engine));
    let before = serde_json::to_vec(&lock(&engine).store.outbox().unwrap()[0].event).unwrap();
    let ctx = context(&engine);
    let (_cmd, mut commands) = mpsc::unbounded_channel();
    let (finished, closed) = oneshot::channel();
    let rounds = AtomicUsize::new(0);
    let gap_requests = AtomicUsize::new(0);
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        let mut closed = Box::pin(closed);
        let mut traffic = tokio::time::interval(Duration::from_millis(40));
        loop {
            tokio::select! {
                result = &mut closed => { result.unwrap(); break; }
                _ = traffic.tick() => {
                    for frame in [
                        ServerFrame::Pong { server_time_ms: None },
                        ServerFrame::Event { ev: genesis.clone() },
                        ServerFrame::Event { ev: historical.clone() },
                        ServerFrame::Event { ev: invalid.clone() },
                        ServerFrame::Event { ev: gap.clone() },
                        ServerFrame::Ephemeral { space: SPACE.into(), from: author.identity.clone(), kind: EphemeralKind::Typing },
                    ] {
                        if socket.send(Message::Binary(frame.encode().into())).await.is_err() { break; }
                    }
                    rounds.fetch_add(1, Ordering::SeqCst);
                }
                frame = socket.next() => match frame {
                    Some(Ok(Message::Binary(bytes))) => match ClientFrame::decode(&bytes).unwrap() {
                        ClientFrame::Sync { all: false, cursors } => {
                            assert_eq!(cursors.len(), 1);
                            assert_eq!(cursors[0].space, SPACE);
                            assert_eq!(cursors[0].next_seq, 1);
                            gap_requests.fetch_add(1, Ordering::SeqCst);
                        }
                        ClientFrame::Ping | ClientFrame::Ephemeral { .. } => {}
                        other => panic!("unexpected initial traffic: {other:?}"),
                    },
                    Some(Ok(_)) => {},
                    Some(Err(_)) | None => { closed.await.unwrap(); break; }
                }
            }
        }
    };
    let mut backoff = Duration::from_secs(3);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("nonprogress traffic must not keep the initial owner alive");
    assert!(matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out"));
    assert!(rounds.load(Ordering::SeqCst) >= 3);
    assert!(
        gap_requests.load(Ordering::SeqCst) >= 2,
        "gap frames were actually dispatched"
    );
    assert_eq!(backoff, Duration::from_secs(3));
    assert!(!ctx.status.borrow().synced);
    let e = lock(&engine);
    assert_eq!(e.next_seq(SPACE), 1);
    assert_eq!(e.next_seq("sp_already_caught_up"), 1);
    assert_eq!(e.store.events(SPACE).unwrap().len(), 1);
    assert_eq!(
        serde_json::to_vec(&e.store.outbox().unwrap()[0].event).unwrap(),
        before
    );
    drop(e);
    ctx.engine.revoke();
    Ok(())
}
