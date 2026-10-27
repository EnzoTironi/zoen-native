use super::*;

async fn receipt_sync_fixture(
) -> std::result::Result<(TestRelay, Author, String, Socket), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    *test.log.sync_spaces.lock().unwrap() =
        Some(vec![RECEIPT_SPACE.into(), EMPTY_RECEIPT_SPACE.into()]);
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut socket = connect(&test.ws_url, &author).await;
    request(
        &mut socket,
        1,
        Op::Register {
            profile: profile(&author, "qa_receipt_replay"),
        },
    )
    .await
    .unwrap();
    // This LogStore double must authorize exactly the registered fixture reader
    // at each receipt-page boundary; discovery alone is not current membership.
    test.log.sync_members.lock().unwrap().extend([
        (RECEIPT_SPACE.into(), author.identity.clone()),
        (EMPTY_RECEIPT_SPACE.into(), author.identity.clone()),
    ]);
    let peer = Signer::generate()?.id();
    test.state.receipts.raise(RECEIPT_SPACE, &peer, 9, 7);
    assert_eq!(
        test.state
            .receipts
            .flush(&test.state.pool, &test.state.metrics)
            .await
            .unwrap(),
        1
    );
    Ok((test, author, peer, socket))
}

async fn assert_sync_receipt_failure(socket: &mut Socket, timeout: Duration) {
    assert!(
        matches!(
            recv_with_timeout(socket, timeout).await,
            Some(ServerFrame::Error {
                code: ErrorCode::Unavailable,
                ..
            })
        ),
        "receipt-query failure must be typed Unavailable, never SyncDone"
    );
    // This is an ordered frame barrier, not a sleep-based absence assertion: Sync
    // returns before the same session handles Ping, and the writer preserves order.
    send(socket, ClientFrame::Ping).await;
    assert!(
        matches!(recv(socket).await, Some(ServerFrame::Pong { .. })),
        "failed receipt replay must not enqueue SyncDone before Pong"
    );
}

async fn assert_healthy_receipt_replay(socket: &mut Socket, peer: &str) {
    send(
        socket,
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        },
    )
    .await;
    for kind in [
        protocol::EphemeralKind::Delivered { seq: 9 },
        protocol::EphemeralKind::Read { seq: 7 },
    ] {
        let frame = recv(socket).await.expect("receipt replay closed");
        assert!(
            matches!(frame,
                ServerFrame::Ephemeral { space, from, kind: actual }
                    if space == RECEIPT_SPACE && from == peer && actual == kind
            ),
            "healthy retry must replay each exact receipt before SyncDone"
        );
    }
    assert!(matches!(recv(socket).await, Some(ServerFrame::SyncDone)));
}

#[tokio::test]
async fn receipt_sync_requires_current_membership_even_when_discovery_is_stale(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (test, author, peer, mut socket) = receipt_sync_fixture().await?;
    assert_healthy_receipt_replay(&mut socket, &peer).await;
    let pair = (RECEIPT_SPACE.into(), author.identity.clone());
    assert!(test.log.sync_members.lock().unwrap().remove(&pair));
    assert!(test
        .log
        .spaces_of(&author.identity)
        .await
        .unwrap()
        .contains(&RECEIPT_SPACE.into()));
    send(
        &mut socket,
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        },
    )
    .await;
    // A strict ordered barrier: no receipt may precede SyncDone or follow it
    // before this same Session handles Ping. Discovery did not grant a role.
    assert!(matches!(
        recv(&mut socket).await,
        Some(ServerFrame::SyncDone)
    ));
    send(&mut socket, ClientFrame::Ping).await;
    assert!(matches!(
        recv(&mut socket).await,
        Some(ServerFrame::Pong { .. })
    ));
    assert!(test.log.sync_members.lock().unwrap().insert(pair));
    assert_healthy_receipt_replay(&mut socket, &peer).await;
    socket.close(None).await.unwrap();
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn sync_receipt_query_failure_is_unavailable_then_reconnect_replays(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (test, author, peer, mut socket) = receipt_sync_fixture().await?;
    // No receipt SELECT has run on this fresh pool, so renaming the table makes
    // the real query fail independently of statement-cache invalidation behavior.
    sqlx::query("ALTER TABLE space_receipts RENAME TO receipt_replay_unavailable")
        .execute(&test.state.pool)
        .await
        .unwrap();
    send(
        &mut socket,
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        },
    )
    .await;
    assert_sync_receipt_failure(&mut socket, Duration::from_secs(3)).await;
    sqlx::query("ALTER TABLE receipt_replay_unavailable RENAME TO space_receipts")
        .execute(&test.state.pool)
        .await
        .unwrap();
    socket.close(None).await.unwrap();
    let mut recovered = connect(&test.ws_url, &author).await;
    assert_healthy_receipt_replay(&mut recovered, &peer).await;

    // A known Space with no stored or pending rows is a successful empty replay.
    send(
        &mut recovered,
        ClientFrame::Sync {
            cursors: vec![protocol::Cursor {
                space: EMPTY_RECEIPT_SPACE.into(),
                next_seq: 0,
            }],
            all: false,
        },
    )
    .await;
    assert!(matches!(
        recv(&mut recovered).await,
        Some(ServerFrame::SyncDone)
    ));
    recovered.close(None).await.unwrap();
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn sync_receipt_query_deadline_is_unavailable_then_reconnect_replays(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (test, author, peer, mut socket) = receipt_sync_fixture().await?;
    // A separate connection blocks only the receipt SELECT. Request admission
    // and outgoing delivery keep their ordinary independent authorization pools.
    let mut lock = PgConnection::connect(&test.database_url).await.unwrap();
    let mut transaction = lock.begin().await.unwrap();
    sqlx::query("LOCK TABLE space_receipts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    send(
        &mut socket,
        ClientFrame::Sync {
            cursors: Vec::new(),
            all: true,
        },
    )
    .await;
    assert_sync_receipt_failure(&mut socket, Duration::from_secs(8)).await;
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "the real SELECT should wait for the sync deadline"
    );
    transaction.rollback().await.unwrap();
    socket.close(None).await.unwrap();
    let mut recovered = connect(&test.ws_url, &author).await;
    assert_healthy_receipt_replay(&mut recovered, &peer).await;
    recovered.close(None).await.unwrap();
    drop(lock);
    test.close().await;
    Ok(())
}
