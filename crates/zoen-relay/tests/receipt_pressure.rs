//! Bounded backlog under actual SQL failures and authenticated WS overload/replay.
#[path = "support/receipt_pressure.rs"]
mod support;
use event_log::{Author, Signer};
use protocol::{ClientFrame, EphemeralKind, ErrorCode, ServerFrame};
use std::sync::atomic::Ordering::Relaxed;
use support::{barrier, recv, send, Relay, SPACE};
use zoen_relay::receipts::Receipts;

#[tokio::test]
async fn overload_disconnects_before_forwarding_then_reconnect_replays() {
    let source = Author::device(&Signer::generate(), Signer::generate());
    let observer = Author::device(&Signer::generate(), Signer::generate());
    let relay = Relay::new(vec![source.identity.clone(), observer.identity.clone()], 1).await;
    let mut source_socket = relay.connect(&source, true).await;
    let mut observer_socket = relay.connect(&observer, true).await;
    let retained = Signer::generate().id();
    assert!(relay.state.receipts.raise(SPACE, &retained, 9, 7));
    // A real write failure while directory admission/delivery remain healthy.
    sqlx::query("CREATE FUNCTION receipt_outage() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned receipt write outage'; END $$")
        .execute(&relay.state.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER receipt_outage BEFORE INSERT ON space_receipts FOR EACH ROW EXECUTE FUNCTION receipt_outage()")
        .execute(&relay.state.pool).await.unwrap();
    assert!(relay
        .state
        .receipts
        .flush(&relay.state.pool, &relay.state.metrics)
        .await
        .is_err());
    send(
        &mut source_socket,
        ClientFrame::Ephemeral {
            space: SPACE.into(),
            kind: EphemeralKind::Read { seq: u64::MAX },
        },
    )
    .await;
    assert!(matches!(
        recv(&mut source_socket).await,
        Some(ServerFrame::Error {
            code: ErrorCode::Unavailable,
            ..
        })
    ));
    assert!(recv(&mut source_socket).await.is_none());
    // Ordered Pong proves the refused receipt never entered the observer's queue.
    barrier(&mut observer_socket).await;
    assert_eq!(relay.state.receipts.backlog(), (1, SPACE.len() + 64));
    assert_eq!(relay.state.metrics.receipts_overloaded.load(Relaxed), 1);
    assert_eq!(relay.state.metrics.ephemeral_forwarded.load(Relaxed), 0);
    sqlx::query("DROP TRIGGER receipt_outage ON space_receipts")
        .execute(&relay.state.pool)
        .await
        .unwrap();
    assert_eq!(
        relay
            .state
            .receipts
            .flush(&relay.state.pool, &relay.state.metrics)
            .await
            .unwrap(),
        1
    );
    let mut reconnected = relay.connect(&source, false).await;
    send(
        &mut reconnected,
        ClientFrame::Sync {
            cursors: vec![],
            all: true,
        },
    )
    .await;
    for kind in [
        EphemeralKind::Delivered { seq: 9 },
        EphemeralKind::Read { seq: 7 },
    ] {
        assert!(
            matches!(recv(&mut reconnected).await, Some(ServerFrame::Ephemeral { space, from, kind: actual }) if space == SPACE && from == retained && actual == kind)
        );
    }
    assert!(matches!(
        recv(&mut reconnected).await,
        Some(ServerFrame::SyncDone)
    ));
    // Native Client SyncDone reconstructs this same frame from durable read history.
    // This test uses the normal protocol, without claiming a joined native/FDB run.
    send(
        &mut reconnected,
        ClientFrame::Ephemeral {
            space: SPACE.into(),
            kind: EphemeralKind::Read { seq: u64::MAX },
        },
    )
    .await;
    assert!(
        matches!(recv(&mut observer_socket).await, Some(ServerFrame::Ephemeral { space, from, kind: EphemeralKind::Read { seq: u64::MAX } }) if space == SPACE && from == source.identity)
    );
    barrier(&mut reconnected).await;
    relay
        .state
        .receipts
        .flush(&relay.state.pool, &relay.state.metrics)
        .await
        .unwrap();
    let replay = Receipts::default()
        .page(&relay.state.pool, SPACE, "", &"f".repeat(64))
        .await
        .unwrap();
    assert!(replay.rows.contains(&(retained, 9, 7)));
    assert!(replay
        .rows
        .contains(&(source.identity.clone(), u64::MAX, u64::MAX)));
    assert_eq!(relay.state.receipts.backlog(), (0, 0));
    reconnected.close(None).await.unwrap();
    observer_socket.close(None).await.unwrap();
    relay.close().await;
}
