use super::*;

#[tokio::test]
async fn login_handles_ping_while_presence_is_stalled() {
    let test = TestRelay::new().await;
    let primary = Author::device(&Signer::generate(), Signer::generate());
    let mut first = connect(&test.ws_url, &primary).await;
    request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    first.close(None).await.unwrap();
    test.log.block_presence.store(true, Ordering::Release);

    let (mut socket, ready) = authenticate(&test.ws_url, &primary).await;
    assert!(matches!(
        ready,
        ServerFrame::Ready {
            registered: true,
            ..
        }
    ));
    tokio::time::timeout(Duration::from_secs(1), test.log.presence_entered.notified())
        .await
        .expect("login should start its presence snapshot");
    // Much shorter than co_members' two-second deadline: this tests decoupling,
    // not merely a smaller presence timeout. Use the real socket/session path.
    send(&mut socket, ClientFrame::Ping).await;
    tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            if matches!(recv(&mut socket).await, Some(ServerFrame::Pong { .. })) {
                break;
            }
        }
    })
    .await
    .expect("presence must not block login frame handling");
    socket.close(None).await.unwrap();
    test.close().await;
}

/// Learn Signal's cancellation/late-result contract without importing its watch
/// objects: Zoen's pending presence lookup belongs to the real socket Session.
#[tokio::test]
async fn closed_session_cancels_presence_lookup_before_late_result_and_reconnect() {
    // This fixture budget starts at the probe's first poll, not at client Close.
    // It is deliberately shorter than the production two-second lookup deadline.
    const CANCEL_WITHIN: Duration = Duration::from_secs(1);
    let test = TestRelay::new().await;
    let primary = Author::device(&Signer::generate(), Signer::generate());
    let mut socket = connect(&test.ws_url, &primary).await;
    let (result, result_rx) = oneshot::channel();
    let (entered_tx, entered) = oneshot::channel();
    let (retired_tx, retired) = oneshot::channel();
    let drops = Arc::new(AtomicUsize::new(0));
    let continuations = Arc::new(AtomicUsize::new(0));
    *test.log.presence_lookup.lock().unwrap() = Some(PendingPresenceLookup {
        identity: primary.identity.clone(),
        result: result_rx,
        entered: entered_tx,
        retired: retired_tx,
        drops: drops.clone(),
        continuations: continuations.clone(),
        hub: test.state.fanout.hub.clone(),
    });
    request(
        &mut socket,
        1,
        Op::Register {
            profile: profile(&primary, "presence_owner"),
        },
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), entered)
        .await
        .expect("registered session must poll its owned presence lookup")
        .unwrap();
    assert!(test.state.fanout.is_online_here(&primary.identity));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(continuations.load(Ordering::SeqCst), 0);

    socket.close(None).await.unwrap();
    let (detached_before_drop, lookup_lifetime) = tokio::time::timeout(CANCEL_WITHIN, retired)
        .await
        .expect("closing the socket must cancel presence before its normal lookup timeout")
        .unwrap();
    assert!(
        detached_before_drop,
        "the lookup must drop after Hub detach, not merely hit its independent deadline"
    );
    assert!(
        lookup_lifetime < CANCEL_WITHIN,
        "lookup cancellation must finish within {CANCEL_WITHIN:?} of its first poll, before its normal timeout; observed {lookup_lifetime:?}"
    );
    assert!(!test.state.fanout.is_online_here(&primary.identity));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(continuations.load(Ordering::SeqCst), 0);
    assert!(
        result.send(Vec::new()).is_err(),
        "a late empty fetch result must find the retired Session receiver gone"
    );
    assert_eq!(continuations.load(Ordering::SeqCst), 0);
    assert!(!test.state.fanout.is_online_here(&primary.identity));

    let (mut replacement, ready) = authenticate(&test.ws_url, &primary).await;
    assert!(matches!(
        ready,
        ServerFrame::Ready {
            registered: true,
            ..
        }
    ));
    send(&mut replacement, ClientFrame::Ping).await;
    loop {
        match recv(&mut replacement).await.expect("replacement closed") {
            ServerFrame::Pong { .. } => break,
            ServerFrame::Error { code, message } => {
                panic!("replacement refused: {code:?} {message}")
            }
            _ => {}
        }
    }
    assert!(test.state.fanout.is_online_here(&primary.identity));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(continuations.load(Ordering::SeqCst), 0);
    replacement.close(None).await.unwrap();
    test.close().await;
}
